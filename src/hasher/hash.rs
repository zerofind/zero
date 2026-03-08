//! File hashing implementation with streaming and memory-mapped file support

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

use blake3::Hasher as Blake3Hasher;
use xxhash_rust::xxh3::Xxh3;

use crate::progress::AtomicProgress;

use super::algorithm::HashAlgorithm;

/// Threshold for using memory-mapped files (20MB)
const MMAP_THRESHOLD: u64 = 20 * 1024 * 1024;

/// Default buffer size for streaming reads (128KB)
const DEFAULT_BUFFER_SIZE: usize = 128 * 1024;

/// Result of hashing a file
#[derive(Debug, Clone)]
pub struct HashResult {
    /// The computed hash as bytes
    pub hash: Vec<u8>,
    /// The algorithm used
    pub algorithm: HashAlgorithm,
    /// Number of bytes hashed
    pub bytes_hashed: u64,
}

impl HashResult {
    /// Convert hash to hex string
    #[inline]
    pub fn to_hex(&self) -> String {
        hex::encode(&self.hash)
    }

    /// Get hash as fixed-size array (for blake3)
    pub fn as_blake3_hash(&self) -> Option<[u8; 32]> {
        if self.algorithm == HashAlgorithm::Blake3 && self.hash.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&self.hash);
            Some(arr)
        } else {
            None
        }
    }

    /// Get hash as fixed-size array (for xxh3)
    pub fn as_xxh3_hash(&self) -> Option<[u8; 16]> {
        if self.algorithm == HashAlgorithm::Xxh3 && self.hash.len() == 16 {
            let mut arr = [0u8; 16];
            arr.copy_from_slice(&self.hash);
            Some(arr)
        } else {
            None
        }
    }
}

/// Error type for hashing operations
#[derive(Debug, thiserror::Error)]
pub enum HashError {
    #[error("Failed to open file: {path}")]
    OpenError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to read file: {path}")]
    ReadError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to get file size: {path}")]
    SizeError {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Hash only the first `prefix_bytes` of a file for cheap early rejection.
///
/// Two files with different prefix hashes cannot be duplicates, avoiding a
/// full-file read. Returns `None` if the file is smaller than `prefix_bytes`
/// (caller should fall through to a full hash instead).
pub fn hash_file_prefix(
    path: &Path,
    algorithm: HashAlgorithm,
    prefix_bytes: u64,
    buffer: &mut [u8],
) -> Result<Option<HashResult>, HashError> {
    let path_str = path.to_string_lossy().to_string();

    let file = File::open(path).map_err(|e| HashError::OpenError {
        path: path_str.clone(),
        source: e,
    })?;

    let file_size = file
        .metadata()
        .map_err(|e| HashError::SizeError {
            path: path_str.clone(),
            source: e,
        })?
        .len();

    // Skip prefix hashing for files smaller than the prefix — just do a full hash
    if file_size <= prefix_bytes {
        return Ok(None);
    }

    let mut reader = BufReader::new(&file);
    let mut remaining = prefix_bytes;

    let hash = match algorithm {
        HashAlgorithm::Blake3 => {
            let mut hasher = Blake3Hasher::new();
            while remaining > 0 {
                let to_read = (remaining as usize).min(buffer.len());
                let n = reader
                    .read(&mut buffer[..to_read])
                    .map_err(|e| HashError::ReadError {
                        path: path_str.clone(),
                        source: e,
                    })?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
                remaining -= n as u64;
            }
            hasher.finalize().as_bytes().to_vec()
        }
        HashAlgorithm::Xxh3 => {
            let mut hasher = Xxh3::new();
            while remaining > 0 {
                let to_read = (remaining as usize).min(buffer.len());
                let n = reader
                    .read(&mut buffer[..to_read])
                    .map_err(|e| HashError::ReadError {
                        path: path_str.clone(),
                        source: e,
                    })?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
                remaining -= n as u64;
            }
            hasher.digest128().to_le_bytes().to_vec()
        }
    };

    Ok(Some(HashResult {
        hash,
        algorithm,
        bytes_hashed: prefix_bytes - remaining,
    }))
}

/// Hash a file using the specified algorithm
///
/// Automatically chooses between memory-mapped I/O (for large files)
/// and buffered streaming (for smaller files).
pub fn hash_file(path: &Path, algorithm: HashAlgorithm) -> Result<HashResult, HashError> {
    let mut buffer = vec![0u8; DEFAULT_BUFFER_SIZE];
    hash_file_with_buffer(path, algorithm, &mut buffer)
}

/// Hash a file with real-time progress callbacks
///
/// Calls the progress callback periodically with bytes hashed so far.
/// For large files, this uses chunked reading instead of mmap to enable progress.
pub fn hash_file_with_progress<F>(
    path: &Path,
    algorithm: HashAlgorithm,
    buffer: &mut [u8],
    progress_callback: F,
) -> Result<HashResult, HashError>
where
    F: Fn(u64), // Called with bytes hashed so far
{
    let path_str = path.to_string_lossy().to_string();

    let file = File::open(path).map_err(|e| HashError::OpenError {
        path: path_str.clone(),
        source: e,
    })?;

    // Get file size
    let file_size = file
        .metadata()
        .map_err(|e| HashError::SizeError {
            path: path_str.clone(),
            source: e,
        })?
        .len();

    // Always use chunked reading for progress support (skip mmap)
    let hash = match algorithm {
        HashAlgorithm::Blake3 => {
            hash_blake3_with_progress(&file, buffer, &path_str, &progress_callback)?
        }
        HashAlgorithm::Xxh3 => {
            hash_xxh3_with_progress(&file, buffer, &path_str, &progress_callback)?
        }
    };

    Ok(HashResult {
        hash,
        algorithm,
        bytes_hashed: file_size,
    })
}

/// Hash a file using a provided buffer (avoids allocation per file)
///
/// This is more efficient when hashing many files in sequence.
pub fn hash_file_with_buffer(
    path: &Path,
    algorithm: HashAlgorithm,
    buffer: &mut [u8],
) -> Result<HashResult, HashError> {
    let path_str = path.to_string_lossy().to_string();

    let mut file = File::open(path).map_err(|e| HashError::OpenError {
        path: path_str.clone(),
        source: e,
    })?;

    // Get file size
    let file_size = file
        .seek(SeekFrom::End(0))
        .map_err(|e| HashError::SizeError {
            path: path_str.clone(),
            source: e,
        })?;

    file.seek(SeekFrom::Start(0))
        .map_err(|e| HashError::SizeError {
            path: path_str.clone(),
            source: e,
        })?;

    let hash = match algorithm {
        HashAlgorithm::Blake3 => hash_blake3(&file, file_size, buffer, &path_str)?,
        HashAlgorithm::Xxh3 => hash_xxh3(&file, file_size, buffer, &path_str)?,
    };

    Ok(HashResult {
        hash,
        algorithm,
        bytes_hashed: file_size,
    })
}

/// Hash a file with real-time atomic progress updates
///
/// Like `hash_file_with_buffer` but updates an `AtomicProgress` tracker
/// per-chunk during hashing. This enables real-time progress display
/// for large file hashing operations.
///
/// Note: For mmap'd large files, progress is updated in one big chunk at the end.
/// For buffered reads, progress is updated per 128KB chunk.
pub fn hash_file_with_atomic_progress(
    path: &Path,
    algorithm: HashAlgorithm,
    buffer: &mut [u8],
    progress: &Arc<AtomicProgress>,
) -> Result<HashResult, HashError> {
    let path_str = path.to_string_lossy().to_string();

    let mut file = File::open(path).map_err(|e| HashError::OpenError {
        path: path_str.clone(),
        source: e,
    })?;

    // Get file size
    let file_size = file
        .seek(SeekFrom::End(0))
        .map_err(|e| HashError::SizeError {
            path: path_str.clone(),
            source: e,
        })?;

    file.seek(SeekFrom::Start(0))
        .map_err(|e| HashError::SizeError {
            path: path_str.clone(),
            source: e,
        })?;

    let hash = match algorithm {
        HashAlgorithm::Blake3 => {
            hash_blake3_with_atomic_progress(&file, file_size, buffer, &path_str, progress)?
        }
        HashAlgorithm::Xxh3 => {
            hash_xxh3_with_atomic_progress(&file, file_size, buffer, &path_str, progress)?
        }
    };

    Ok(HashResult {
        hash,
        algorithm,
        bytes_hashed: file_size,
    })
}

/// Hash using Blake3
fn hash_blake3(
    file: &File,
    file_size: u64,
    buffer: &mut [u8],
    path_str: &str,
) -> Result<Vec<u8>, HashError> {
    let mut hasher = Blake3Hasher::new();

    if file_size > MMAP_THRESHOLD {
        // Use memory-mapped file for large files
        // SAFETY: We have exclusive read access to the file
        let mmap = unsafe {
            memmap2::Mmap::map(file).map_err(|e| HashError::ReadError {
                path: path_str.to_string(),
                source: e,
            })?
        };
        hasher.update(&mmap);
    } else {
        // Use buffered reading for smaller files
        let mut reader = BufReader::new(file);
        loop {
            let bytes_read = reader.read(buffer).map_err(|e| HashError::ReadError {
                path: path_str.to_string(),
                source: e,
            })?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
        }
    }

    Ok(hasher.finalize().as_bytes().to_vec())
}

/// Hash using Blake3 with progress callbacks (always chunked, no mmap)
fn hash_blake3_with_progress<F>(
    file: &File,
    buffer: &mut [u8],
    path_str: &str,
    progress_callback: &F,
) -> Result<Vec<u8>, HashError>
where
    F: Fn(u64),
{
    let mut hasher = Blake3Hasher::new();
    let mut reader = BufReader::new(file);
    let mut bytes_hashed: u64 = 0;

    loop {
        let bytes_read = reader.read(buffer).map_err(|e| HashError::ReadError {
            path: path_str.to_string(),
            source: e,
        })?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
        bytes_hashed += bytes_read as u64;
        progress_callback(bytes_hashed);
    }

    Ok(hasher.finalize().as_bytes().to_vec())
}

/// Hash using Blake3 with atomic progress updates
fn hash_blake3_with_atomic_progress(
    file: &File,
    file_size: u64,
    buffer: &mut [u8],
    path_str: &str,
    progress: &Arc<AtomicProgress>,
) -> Result<Vec<u8>, HashError> {
    let mut hasher = Blake3Hasher::new();

    if file_size > MMAP_THRESHOLD {
        // Use memory-mapped file for large files
        // SAFETY: We have exclusive read access to the file
        let mmap = unsafe {
            memmap2::Mmap::map(file).map_err(|e| HashError::ReadError {
                path: path_str.to_string(),
                source: e,
            })?
        };
        hasher.update(&mmap);
        // Update progress after mmap (all at once)
        progress.add_bytes(file_size);
    } else {
        // Use buffered reading for smaller files with per-chunk progress
        let mut reader = BufReader::new(file);
        loop {
            let bytes_read = reader.read(buffer).map_err(|e| HashError::ReadError {
                path: path_str.to_string(),
                source: e,
            })?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
            progress.add_bytes(bytes_read as u64);
        }
    }

    Ok(hasher.finalize().as_bytes().to_vec())
}

/// Hash using XXH3 (128-bit)
fn hash_xxh3(
    file: &File,
    file_size: u64,
    buffer: &mut [u8],
    path_str: &str,
) -> Result<Vec<u8>, HashError> {
    let mut hasher = Xxh3::new();

    if file_size > MMAP_THRESHOLD {
        // Use memory-mapped file for large files
        // SAFETY: We have exclusive read access to the file
        let mmap = unsafe {
            memmap2::Mmap::map(file).map_err(|e| HashError::ReadError {
                path: path_str.to_string(),
                source: e,
            })?
        };
        hasher.update(&mmap);
    } else {
        // Use buffered reading for smaller files
        let mut reader = BufReader::new(file);
        loop {
            let bytes_read = reader.read(buffer).map_err(|e| HashError::ReadError {
                path: path_str.to_string(),
                source: e,
            })?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
        }
    }

    // XXH3 128-bit digest
    Ok(hasher.digest128().to_le_bytes().to_vec())
}

/// Hash using XXH3 with progress callbacks (always chunked, no mmap)
fn hash_xxh3_with_progress<F>(
    file: &File,
    buffer: &mut [u8],
    path_str: &str,
    progress_callback: &F,
) -> Result<Vec<u8>, HashError>
where
    F: Fn(u64),
{
    let mut hasher = Xxh3::new();
    let mut reader = BufReader::new(file);
    let mut bytes_hashed: u64 = 0;

    loop {
        let bytes_read = reader.read(buffer).map_err(|e| HashError::ReadError {
            path: path_str.to_string(),
            source: e,
        })?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
        bytes_hashed += bytes_read as u64;
        progress_callback(bytes_hashed);
    }

    // XXH3 128-bit digest
    Ok(hasher.digest128().to_le_bytes().to_vec())
}

/// Hash using XXH3 (128-bit) with atomic progress updates
fn hash_xxh3_with_atomic_progress(
    file: &File,
    file_size: u64,
    buffer: &mut [u8],
    path_str: &str,
    progress: &Arc<AtomicProgress>,
) -> Result<Vec<u8>, HashError> {
    let mut hasher = Xxh3::new();

    if file_size > MMAP_THRESHOLD {
        // Use memory-mapped file for large files
        // SAFETY: We have exclusive read access to the file
        let mmap = unsafe {
            memmap2::Mmap::map(file).map_err(|e| HashError::ReadError {
                path: path_str.to_string(),
                source: e,
            })?
        };
        hasher.update(&mmap);
        // Update progress after mmap (all at once)
        progress.add_bytes(file_size);
    } else {
        // Use buffered reading for smaller files with per-chunk progress
        let mut reader = BufReader::new(file);
        loop {
            let bytes_read = reader.read(buffer).map_err(|e| HashError::ReadError {
                path: path_str.to_string(),
                source: e,
            })?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
            progress.add_bytes(bytes_read as u64);
        }
    }

    Ok(hasher.digest128().to_le_bytes().to_vec())
}

#[cfg(test)]
#[path = "hash_test.rs"]
mod hash_test;
