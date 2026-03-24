//! File copy implementation
//!
//! Uses `std::fs::copy` for fast transfers (leverages OS-level optimizations like APFS clonefile).
//! This is ~28x faster than chunked I/O for local copies.
//!
//! For chunked/resumable transfers (e.g., network), see the `chunked` module.
//!
//! ## Hash-on-transfer
//!
//! When `--verify` is enabled, we can compute the hash while copying to avoid
//! re-reading the source file. Use `copy_file_with_hash` for this.
//!
//! ## Real-time Progress
//!
//! Use `copy_file_with_hash_progress` with an `AtomicProgress` tracker for
//! real-time byte-level progress updates during large file transfers.
//!
//! For fast copy (`std::fs::copy`) with progress, use `copy_file_with_progress`
//! which polls the destination file size during the copy.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use xxhash_rust::xxh3::Xxh3;

use hasher::{HashAlgorithm, hash_file_with_buffer};
use progress::AtomicProgress;

// Re-export from chunked_types for backwards compatibility
pub use super::chunked_types::DEFAULT_CHUNK_SIZE;

/// Options for file copy
#[derive(Debug, Clone, Default)]
pub struct CopyOptions {
    /// Whether to verify checksum after copy
    pub verify: bool,
    /// Hash algorithm for verification
    pub hash_algorithm: HashAlgorithm,
}

/// Progress information for a copy operation
#[derive(Debug, Clone)]
pub struct CopyProgress {
    /// Total bytes to copy
    pub total_bytes: u64,
    /// Bytes copied so far
    pub bytes_copied: u64,
    /// Whether the copy is complete
    pub complete: bool,
}

impl CopyProgress {
    /// Percentage complete (0.0 to 100.0)
    pub fn percent(&self) -> f64 {
        if self.total_bytes == 0 {
            100.0
        } else {
            (self.bytes_copied as f64 / self.total_bytes as f64) * 100.0
        }
    }
}

/// Error type for copy operations
#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    #[error("Failed to open source file: {path}")]
    SourceOpenError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to create destination file: {path}")]
    DestCreateError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to create destination directory: {path}")]
    DirCreateError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to copy file: {path}")]
    CopyError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to get file metadata: {path}")]
    MetadataError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Checksum mismatch after copy: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("Failed to verify checksum: {0}")]
    VerifyError(String),

    #[error("Failed to read source file: {path}")]
    ReadError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to write destination file: {path}")]
    WriteError {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Result of a copy operation with hash
#[derive(Debug, Clone)]
pub struct CopyWithHashResult {
    /// Bytes copied
    pub bytes_copied: u64,
    /// Source file hash (xxh3, 16 bytes as hex)
    pub source_hash: String,
    /// Source file modification time (unix timestamp)
    pub source_mtime: i64,
    /// Whether destination was verified (read back and hashed)
    pub dest_verified: bool,
    /// Destination hash if verified
    pub dest_hash: Option<String>,
}

/// Copy a file while computing its hash (streaming)
///
/// This is slightly slower than `std::fs::copy` but computes the source hash
/// during the copy, avoiding a separate read pass. Use this when `--verify`
/// is enabled to get the source hash "for free".
///
/// After copy, optionally reads back the destination to verify integrity.
///
/// # Arguments
/// * `source` - Source file path
/// * `dest` - Destination file path
/// * `verify_dest` - If true, read back dest and verify hash matches
///
/// # Returns
/// `CopyWithHashResult` containing bytes copied, source hash, and verification status
/// Copy a file while computing hash, without progress tracking
///
/// This is the simple version for backwards compatibility.
/// For real-time progress updates, use `copy_file_with_hash_progress`.
pub fn copy_file_with_hash(
    source: &Path,
    dest: &Path,
    verify_dest: bool,
) -> Result<CopyWithHashResult, CopyError> {
    copy_file_with_hash_progress(source, dest, verify_dest, None)
}

/// Copy a file while computing hash, with optional real-time progress tracking
///
/// When `progress` is provided, bytes are reported as they're written,
/// enabling real-time progress display for large files.
///
/// # Arguments
/// * `source` - Source file path
/// * `dest` - Destination file path
/// * `verify_dest` - If true, read back destination and verify hash matches
/// * `progress` - Optional atomic progress tracker for real-time updates
pub fn copy_file_with_hash_progress(
    source: &Path,
    dest: &Path,
    verify_dest: bool,
    progress: Option<&Arc<AtomicProgress>>,
) -> Result<CopyWithHashResult, CopyError> {
    let source_str = source.to_string_lossy().to_string();
    let dest_str = dest.to_string_lossy().to_string();

    // Get source metadata
    let metadata = fs::metadata(source).map_err(|e| CopyError::MetadataError {
        path: source_str.clone(),
        source: e,
    })?;
    let _total_bytes = metadata.len();
    let source_mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);

    // Create destination directory if needed
    if let Some(parent) = dest.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| CopyError::DirCreateError {
            path: parent.to_string_lossy().to_string(),
            source: e,
        })?;
    }

    // Open source and destination
    let src_file = File::open(source).map_err(|e| CopyError::SourceOpenError {
        path: source_str.clone(),
        source: e,
    })?;
    let dst_file = File::create(dest).map_err(|e| CopyError::DestCreateError {
        path: dest_str.clone(),
        source: e,
    })?;

    // Use buffered I/O with 128KB buffer
    let mut reader = BufReader::with_capacity(128 * 1024, src_file);
    let mut writer = BufWriter::with_capacity(128 * 1024, dst_file);
    let mut hasher = Xxh3::new();
    let mut buffer = vec![0u8; 128 * 1024];
    let mut bytes_copied = 0u64;

    // Copy while hashing
    loop {
        let n = reader.read(&mut buffer).map_err(|e| CopyError::ReadError {
            path: source_str.clone(),
            source: e,
        })?;

        if n == 0 {
            break;
        }

        // SAFETY: `n` is bounded by `buffer.len()` per the `Read::read` contract
        #[allow(clippy::indexing_slicing)]
        let chunk = &buffer[..n];
        hasher.update(chunk);
        writer.write_all(chunk).map_err(|e| CopyError::WriteError {
            path: dest_str.clone(),
            source: e,
        })?;

        bytes_copied += n as u64;

        // Update progress tracker if provided
        if let Some(p) = progress {
            p.add_bytes(n as u64);
        }
    }

    // Flush the writer
    writer.flush().map_err(|e| CopyError::WriteError {
        path: dest_str.clone(),
        source: e,
    })?;

    // Compute source hash
    let source_hash_bytes = hasher.digest128().to_le_bytes();
    let source_hash = bytes_to_hex(&source_hash_bytes);

    // Verify destination if requested
    let (dest_verified, dest_hash) = if verify_dest {
        let mut hash_buffer = vec![0u8; 128 * 1024];
        let dest_result = hash_file_with_buffer(dest, HashAlgorithm::Xxh3, &mut hash_buffer)
            .map_err(|e| CopyError::VerifyError(e.to_string()))?;

        let dest_hash_hex = dest_result.to_hex();

        if source_hash != dest_hash_hex {
            return Err(CopyError::ChecksumMismatch {
                expected: source_hash,
                actual: dest_hash_hex,
            });
        }

        (true, Some(dest_hash_hex))
    } else {
        (false, None)
    };

    // Preserve file permissions
    if let Ok(src_meta) = fs::metadata(source) {
        let _ = fs::set_permissions(dest, src_meta.permissions());
    }

    Ok(CopyWithHashResult {
        bytes_copied,
        source_hash,
        source_mtime,
        dest_verified,
        dest_hash,
    })
}

/// Convert bytes to hex string
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut acc, b| {
        use std::fmt::Write;
        write!(acc, "{b:02x}").expect("write to String is infallible");
        acc
    })
}

/// Copy a file using `std::fs::copy` (fast, uses OS optimizations)
///
/// This is the default and fastest method. On macOS/APFS it may use clonefile
/// for instant copies on the same volume.
///
/// For real-time progress updates during copy, use `copy_file_with_progress`.
///
/// # Arguments
/// * `source` - Source file path
/// * `dest` - Destination file path
/// * `options` - Copy options (verification, etc.)
///
/// # Returns
/// Tuple of (progress info, optional source hash if verify=true)
pub fn copy_file(
    source: &Path,
    dest: &Path,
    options: &CopyOptions,
) -> Result<(CopyProgress, Option<String>), CopyError> {
    copy_file_with_progress(source, dest, options, None)
}

/// Copy a file using `std::fs::copy` with real-time progress tracking
///
/// This uses a background thread to poll the destination file size every 50ms
/// while `std::fs::copy` runs. This gives real-time progress with minimal overhead
/// while keeping the fast kernel-level copy.
///
/// # Arguments
/// * `source` - Source file path
/// * `dest` - Destination file path
/// * `options` - Copy options (verification, etc.)
/// * `progress` - Optional atomic progress tracker for real-time updates
///
/// # Returns
/// Tuple of (progress info, optional source hash if verify=true)
/// Threshold for polling progress: 10MB
/// Below this, thread spawn overhead exceeds benefit of progress updates
const PROGRESS_POLL_THRESHOLD: u64 = 10 * 1024 * 1024;

pub fn copy_file_with_progress(
    source: &Path,
    dest: &Path,
    options: &CopyOptions,
    progress: Option<&Arc<AtomicProgress>>,
) -> Result<(CopyProgress, Option<String>), CopyError> {
    let source_str = source.to_string_lossy().to_string();

    // Get source file size
    let metadata = fs::metadata(source).map_err(|e| CopyError::MetadataError {
        path: source_str.clone(),
        source: e,
    })?;
    let total_bytes = metadata.len();

    // Create destination directory if needed
    if let Some(parent) = dest.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| CopyError::DirCreateError {
            path: parent.to_string_lossy().to_string(),
            source: e,
        })?;
    }

    // Use std::fs::copy for fast OS-level copy
    // Only spawn polling thread for large files (>10MB) to avoid thread overhead
    let bytes_copied =
        if let Some(prog) = progress.filter(|_| total_bytes >= PROGRESS_POLL_THRESHOLD) {
            // Flag to signal poller thread to stop
            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_flag_clone = Arc::clone(&stop_flag);

            // Clone what we need for the poller thread
            let dest_path = dest.to_path_buf();
            let prog_clone = Arc::clone(prog);

            // Track bytes we've reported (to calculate delta)
            let mut last_reported: u64 = 0;

            // Spawn poller thread
            let poller = thread::spawn(move || {
                while !stop_flag_clone.load(Ordering::Relaxed) {
                    if let Ok(meta) = fs::metadata(&dest_path) {
                        let current_size = meta.len();
                        // Only report if size increased
                        if current_size > last_reported {
                            let delta = current_size - last_reported;
                            prog_clone.add_bytes(delta);
                            last_reported = current_size;
                        }
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                last_reported
            });

            // Do the actual copy (this is the fast kernel call)
            let result = fs::copy(source, dest);

            // Signal poller to stop and wait for it
            stop_flag.store(true, Ordering::Relaxed);
            let last_polled = poller.join().unwrap_or(0);

            // Handle copy result
            let bytes = result.map_err(|e| CopyError::CopyError {
                path: source_str.clone(),
                source: e,
            })?;

            // Report any remaining bytes not caught by poller
            if bytes > last_polled {
                prog.add_bytes(bytes - last_polled);
            }

            bytes
        } else {
            // No progress tracking (or small file), just copy
            let bytes = fs::copy(source, dest).map_err(|e| CopyError::CopyError {
                path: source_str.clone(),
                source: e,
            })?;

            // Report bytes after copy for small files
            if let Some(prog) = progress {
                prog.add_bytes(bytes);
            }

            bytes
        };

    let progress = CopyProgress {
        total_bytes,
        bytes_copied,
        complete: true,
    };

    // Verify checksum if requested
    let source_hash = if options.verify {
        let mut hash_buffer = vec![0u8; 128 * 1024];

        let src_hash = hash_file_with_buffer(source, options.hash_algorithm, &mut hash_buffer)
            .map_err(|e| CopyError::VerifyError(e.to_string()))?;

        let dest_hash = hash_file_with_buffer(dest, options.hash_algorithm, &mut hash_buffer)
            .map_err(|e| CopyError::VerifyError(e.to_string()))?;

        if src_hash.hash != dest_hash.hash {
            return Err(CopyError::ChecksumMismatch {
                expected: src_hash.to_hex(),
                actual: dest_hash.to_hex(),
            });
        }

        Some(src_hash.to_hex())
    } else {
        None
    };

    Ok((progress, source_hash))
}

/// Check if a file exists at destination and return its size
///
/// Used to detect partial/incomplete files from interrupted transfers.
pub fn get_partial_size(dest: &Path) -> Option<u64> {
    fs::metadata(dest).ok().map(|m| m.len())
}

/// Remove a partial/incomplete file
///
/// Called before recopying a file that was interrupted mid-transfer.
pub fn remove_partial(dest: &Path) -> std::io::Result<()> {
    if dest.exists() {
        fs::remove_file(dest)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "chunk_test.rs"]
mod chunk_test;
