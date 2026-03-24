//! Chunked transfer with smart resume capability
//!
//! This module provides chunk-based file transfer that can resume interrupted
//! transfers by verifying existing destination chunks against source.
//!
//! ## Use Cases
//!
//! - Large file transfers over slow USB drives
//! - Resuming after Mac sleep, USB disconnect, or user cancel
//! - Any transfer where restart cost exceeds verification cost
//!
//! ## Algorithm
//!
//! 1. Split file into fixed-size chunks (default 4MB)
//! 2. If destination exists, hash existing chunks and compare with source
//! 3. Find first mismatched chunk (resume point)
//! 4. Truncate destination to resume point
//! 5. Copy remaining chunks with progress updates
//! 6. Optionally verify final file hash
//!
//! ## Performance
//!
//! For local copies, `std::fs::copy()` is faster (~870 MB/s vs ~650 MB/s)
//! due to OS-level optimizations. Use chunked transfer when:
//! - File is large (>100MB) and resume matters
//! - Transfer medium is slow/unreliable (USB, network)
//! - Fine-grained progress is needed

use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;

use xxhash_rust::xxh3::Xxh3;

use progress::AtomicProgress;

// Use types from sibling module
use super::chunked_types::{
    ChunkedCopyError, ChunkedCopyOptions, ChunkedCopyResult, IO_BUFFER_SIZE,
};

/// Hash a chunk of a file at a specific offset
///
/// Returns the xxh3 hash of the chunk, or None if the chunk is beyond EOF.
fn hash_chunk_at(
    file: &mut File,
    offset: u64,
    chunk_size: usize,
    buffer: &mut [u8],
) -> Result<Option<u128>, std::io::Error> {
    file.seek(SeekFrom::Start(offset))?;

    let mut hasher = Xxh3::new();
    let mut bytes_read_total = 0usize;

    // Read up to chunk_size bytes
    while bytes_read_total < chunk_size {
        let to_read = std::cmp::min(buffer.len(), chunk_size - bytes_read_total);
        // SAFETY: `to_read` is clamped to `buffer.len()` by `min` above;
        // `n` is bounded by `to_read` per the `Read::read` contract.
        #[allow(clippy::indexing_slicing)]
        let read_slice = &mut buffer[..to_read];
        let n = file.read(read_slice)?;
        if n == 0 {
            break; // EOF
        }
        #[allow(clippy::indexing_slicing)]
        let data = &buffer[..n];
        hasher.update(data);
        bytes_read_total += n;
    }

    if bytes_read_total == 0 {
        Ok(None) // Beyond EOF
    } else {
        Ok(Some(hasher.digest128()))
    }
}

/// Find the resume point by comparing chunk hashes between source and destination
///
/// Returns the chunk index to resume from (0 = start fresh, N = resume from chunk N)
fn find_resume_point(
    source: &Path,
    dest: &Path,
    source_size: u64,
    chunk_size: usize,
    progress: Option<&Arc<AtomicProgress>>,
) -> Result<usize, ChunkedCopyError> {
    let source_str = source.to_string_lossy().to_string();
    let dest_str = dest.to_string_lossy().to_string();

    // Open both files
    let mut src_file = File::open(source).map_err(|e| ChunkedCopyError::SourceOpenError {
        path: source_str.clone(),
        source: e,
    })?;

    let mut dest_file = File::open(dest).map_err(|e| ChunkedCopyError::SourceOpenError {
        path: dest_str.clone(),
        source: e,
    })?;

    // Get destination size
    let dest_size = dest_file
        .seek(SeekFrom::End(0))
        .map_err(|e| ChunkedCopyError::SeekError {
            path: dest_str.clone(),
            source: e,
        })?;

    // If dest is larger than source, something is wrong
    if dest_size > source_size {
        return Err(ChunkedCopyError::DestLargerThanSource {
            dest_size,
            src_size: source_size,
        });
    }

    // If dest is empty, start fresh
    if dest_size == 0 {
        return Ok(0);
    }

    // Calculate number of complete chunks in destination
    let complete_dest_chunks = (dest_size / chunk_size as u64) as usize;

    if complete_dest_chunks == 0 {
        // Destination has less than one chunk, start fresh
        return Ok(0);
    }

    // Allocate buffer for hashing
    let mut buffer = vec![0u8; IO_BUFFER_SIZE];

    // Compare chunks
    let mut resume_from = 0usize;

    for chunk_idx in 0..complete_dest_chunks {
        let offset = (chunk_idx * chunk_size) as u64;

        // Hash source chunk
        let src_hash =
            hash_chunk_at(&mut src_file, offset, chunk_size, &mut buffer).map_err(|e| {
                ChunkedCopyError::ReadError {
                    path: source_str.clone(),
                    source: e,
                }
            })?;

        // Hash dest chunk
        let dest_hash =
            hash_chunk_at(&mut dest_file, offset, chunk_size, &mut buffer).map_err(|e| {
                ChunkedCopyError::ReadError {
                    path: dest_str.clone(),
                    source: e,
                }
            })?;

        match (src_hash, dest_hash) {
            (Some(s), Some(d)) if s == d => {
                // Chunks match, can resume from next chunk
                resume_from = chunk_idx + 1;

                // Update progress for verified bytes
                if let Some(p) = progress {
                    // Note: We're just tracking verification progress here
                    // Actual copy progress is separate
                    p.add_bytes(chunk_size as u64);
                }
            }
            _ => {
                // Mismatch or missing chunk, resume from this chunk
                break;
            }
        }
    }

    Ok(resume_from)
}

/// Copy a file using chunked I/O with smart resume
///
/// This function will:
/// 1. Check if destination exists with partial content
/// 2. If so, verify existing chunks match source
/// 3. Resume from first mismatched or missing chunk
/// 4. Optionally verify the complete file after copy
///
/// # Arguments
/// * `source` - Source file path
/// * `dest` - Destination file path
/// * `options` - Copy options (chunk size, verification, etc.)
/// * `progress` - Optional atomic progress tracker for real-time updates
///
/// # Returns
/// Result containing transfer statistics and optional file hash
pub fn copy_chunked_with_resume(
    source: &Path,
    dest: &Path,
    options: &ChunkedCopyOptions,
    progress: Option<&Arc<AtomicProgress>>,
) -> Result<ChunkedCopyResult, ChunkedCopyError> {
    let source_str = source.to_string_lossy().to_string();
    let dest_str = dest.to_string_lossy().to_string();

    // Get source file size
    let source_size = std::fs::metadata(source)
        .map_err(|e| ChunkedCopyError::MetadataError {
            path: source_str.clone(),
            source: e,
        })?
        .len();

    // Calculate total chunks
    let total_chunks = if source_size == 0 {
        1
    } else {
        (source_size as usize).div_ceil(options.chunk_size)
    };

    // Determine resume point
    let resume_from_chunk = if options.enable_resume && dest.exists() {
        let resume_point =
            find_resume_point(source, dest, source_size, options.chunk_size, progress)?;
        if resume_point > 0 {
            let resumed_bytes = (resume_point * options.chunk_size) as u64;
            let remaining_bytes = source_size.saturating_sub(resumed_bytes);
            eprintln!(
                "  Resuming: {:.1} MB verified, {:.1} MB to transfer",
                resumed_bytes as f64 / 1_000_000.0,
                remaining_bytes as f64 / 1_000_000.0
            );
        }
        resume_point
    } else {
        0
    };

    let resume_offset = (resume_from_chunk * options.chunk_size) as u64;

    // Open source file and seek to resume position
    let src_file = File::open(source).map_err(|e| ChunkedCopyError::SourceOpenError {
        path: source_str.clone(),
        source: e,
    })?;
    let mut reader = BufReader::with_capacity(IO_BUFFER_SIZE, src_file);
    reader
        .seek(SeekFrom::Start(resume_offset))
        .map_err(|e| ChunkedCopyError::SeekError {
            path: source_str.clone(),
            source: e,
        })?;

    // Open or create destination file
    let dest_file = if resume_from_chunk > 0 {
        // Resuming - open existing file and truncate to resume point
        let file = OpenOptions::new().write(true).open(dest).map_err(|e| {
            ChunkedCopyError::DestCreateError {
                path: dest_str.clone(),
                source: e,
            }
        })?;

        // Truncate to resume point (removes any partial chunk at the end)
        file.set_len(resume_offset)
            .map_err(|e| ChunkedCopyError::TruncateError {
                path: dest_str.clone(),
                source: e,
            })?;

        file
    } else {
        // Fresh copy - create new file
        if let Some(parent) = dest.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent).map_err(|e| ChunkedCopyError::DestCreateError {
                path: parent.to_string_lossy().to_string(),
                source: e,
            })?;
        }
        File::create(dest).map_err(|e| ChunkedCopyError::DestCreateError {
            path: dest_str.clone(),
            source: e,
        })?
    };

    let mut writer = BufWriter::with_capacity(IO_BUFFER_SIZE, dest_file);

    // Seek to resume position in destination
    writer
        .seek(SeekFrom::Start(resume_offset))
        .map_err(|e| ChunkedCopyError::SeekError {
            path: dest_str.clone(),
            source: e,
        })?;

    // Allocate buffer
    let mut buffer = vec![0u8; IO_BUFFER_SIZE];
    let mut bytes_transferred = 0u64;
    let mut current_chunk = resume_from_chunk;

    // Copy remaining chunks
    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| ChunkedCopyError::ReadError {
                path: source_str.clone(),
                source: e,
            })?;

        if bytes_read == 0 {
            break;
        }

        // SAFETY: `bytes_read` is bounded by `buffer.len()` per the `Read::read` contract
        #[allow(clippy::indexing_slicing)]
        let data = &buffer[..bytes_read];
        writer
            .write_all(data)
            .map_err(|e| ChunkedCopyError::WriteError {
                path: dest_str.clone(),
                source: e,
            })?;

        bytes_transferred += bytes_read as u64;

        // Update progress
        if let Some(p) = progress {
            p.add_bytes(bytes_read as u64);
        }

        // Track chunk boundaries for progress display
        let total_bytes_done = resume_offset + bytes_transferred;
        let new_chunk = (total_bytes_done as usize) / options.chunk_size;
        if new_chunk > current_chunk {
            current_chunk = new_chunk;
        }
    }

    // Flush writer
    writer.flush().map_err(|e| ChunkedCopyError::WriteError {
        path: dest_str.clone(),
        source: e,
    })?;
    drop(writer);

    // Verify source hasn't changed during copy
    let final_source_size = std::fs::metadata(source)
        .map_err(|e| ChunkedCopyError::MetadataError {
            path: source_str.clone(),
            source: e,
        })?
        .len();

    if final_source_size != source_size {
        return Err(ChunkedCopyError::SourceChanged {
            original: source_size,
            current: final_source_size,
        });
    }

    // Optionally verify complete file hash
    let file_hash = if options.verify_after_copy {
        let hash = hash_file(dest, &mut buffer).map_err(|e| ChunkedCopyError::ReadError {
            path: dest_str.clone(),
            source: e,
        })?;

        // Also hash source to compare
        let src_hash = hash_file(source, &mut buffer).map_err(|e| ChunkedCopyError::ReadError {
            path: source_str.clone(),
            source: e,
        })?;

        if hash != src_hash {
            return Err(ChunkedCopyError::ChecksumMismatch {
                expected: format!("{src_hash:032x}"),
                actual: format!("{hash:032x}"),
            });
        }

        Some(format!("{hash:032x}"))
    } else {
        None
    };

    // Preserve file permissions
    if let Ok(src_meta) = std::fs::metadata(source) {
        let _ = std::fs::set_permissions(dest, src_meta.permissions());
    }

    // Calculate chunks transferred
    let chunks_transferred = total_chunks - resume_from_chunk;

    Ok(ChunkedCopyResult {
        total_bytes: source_size,
        bytes_transferred,
        file_hash,
        resumed_from_chunk: resume_from_chunk,
        total_chunks,
        chunks_transferred,
    })
}

/// Hash an entire file using xxh3
fn hash_file(path: &Path, buffer: &mut [u8]) -> Result<u128, std::io::Error> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(IO_BUFFER_SIZE, file);
    let mut hasher = Xxh3::new();

    loop {
        let n = reader.read(buffer)?;
        if n == 0 {
            break;
        }
        // SAFETY: `n` is bounded by `buffer.len()` per the `Read::read` contract
        #[allow(clippy::indexing_slicing)]
        let data = &buffer[..n];
        hasher.update(data);
    }

    Ok(hasher.digest128())
}

#[cfg(test)]
#[path = "chunked_test.rs"]
mod chunked_test;
