//! Diff comparison logic
//!
//! Compares two directory scans and produces a diff.
//! Supports both metadata-based (fast) and checksum-based (thorough) comparison.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crossfire::mpsc as cf_mpsc;

use hasher::HashAlgorithm;
use progress::AtomicProgress;
use scanner::FileEntry;

use super::checksum::{self, ChecksumDiffConfig, ProgressMode};
use super::result::{DiffEntry, DiffResult};

/// Sender type for hash streaming channels
pub type HashSender = crossfire::MTx<cf_mpsc::List<ComputedHash>>;

/// A computed hash that can be cached
#[derive(Debug, Clone)]
pub struct ComputedHash {
    /// Relative path from storage root
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Modification time (unix timestamp)
    pub mtime: u64,
    /// Hash bytes (16 bytes for xxh3, 32 for blake3)
    pub hash: Vec<u8>,
    /// Which algorithm was used
    pub algorithm: HashAlgorithm,
}

/// Options for diff comparison
#[derive(Debug, Clone, Default)]
pub struct DiffOptions {
    /// Use checksums instead of mtime+size for comparison
    pub use_checksum: bool,
    /// Include identical files in results (default: false to save memory)
    pub include_identical: bool,
    /// Hash algorithm to use when `use_checksum` is true
    pub hash_algorithm: HashAlgorithm,
}

/// Progress information for diff with checksums
#[derive(Debug, Clone)]
pub struct DiffProgress {
    pub files_processed: usize,
    pub files_total: usize,
    pub bytes_processed: u64,
    pub bytes_total: u64,
    pub current_file: String,
    /// Size of the current file being processed (for display)
    pub current_file_size: u64,
}

/// Compare two directory scans and return the differences
///
/// Uses metadata (size + mtime) for fast comparison by default.
/// Set `options.use_checksum` for content-based comparison.
pub fn diff(source: &[FileEntry], dest: &[FileEntry]) -> DiffResult {
    diff_with_options(source, dest, &DiffOptions::default())
}

/// Compare two directory scans with custom options
pub fn diff_with_options(
    source: &[FileEntry],
    dest: &[FileEntry],
    options: &DiffOptions,
) -> DiffResult {
    if options.use_checksum {
        checksum::diff_with_checksums_simple(source, dest, options)
    } else {
        diff_with_metadata(source, dest, options)
    }
}

/// Compare two directory scans with progress reporting
///
/// The callback is called periodically during checksum computation.
pub fn diff_with_progress<F>(
    source: &[FileEntry],
    dest: &[FileEntry],
    options: &DiffOptions,
    progress_callback: F,
) -> DiffResult
where
    F: Fn(DiffProgress) + Send + Sync,
{
    if options.use_checksum {
        let config = ChecksumDiffConfig {
            source,
            dest,
            options,
            source_hash_tx: None,
            dest_hash_tx: None,
        };
        let (result, _, _) =
            checksum::diff_with_checksums(config, ProgressMode::Callback(&progress_callback));
        result
    } else {
        diff_with_metadata(source, dest, options)
    }
}

/// Compare two directory scans with progress reporting, returning computed hashes for caching
///
/// Returns (`DiffResult`, `source_hashes`, `dest_hashes`) where hashes are the computed
/// checksums that can be inserted into cache databases.
pub fn diff_with_progress_and_hashes<F>(
    source: &[FileEntry],
    dest: &[FileEntry],
    options: &DiffOptions,
    progress_callback: F,
) -> (DiffResult, Vec<ComputedHash>, Vec<ComputedHash>)
where
    F: Fn(DiffProgress) + Send + Sync,
{
    if options.use_checksum {
        let config = ChecksumDiffConfig {
            source,
            dest,
            options,
            source_hash_tx: None,
            dest_hash_tx: None,
        };
        checksum::diff_with_checksums(config, ProgressMode::Callback(&progress_callback))
    } else {
        (
            diff_with_metadata(source, dest, options),
            Vec::new(),
            Vec::new(),
        )
    }
}

/// Compare two directory scans with progress reporting and streaming hash output
///
/// Like `diff_with_progress_and_hashes` but streams computed hashes to channels
/// for incremental caching. This allows hashes to be persisted as they're computed,
/// so cancellation doesn't lose all progress.
///
/// # Arguments
/// * `source_hash_tx` - Channel to send source file hashes (optional)
/// * `dest_hash_tx` - Channel to send dest file hashes (optional)
///
/// Returns (`DiffResult`, `source_hashes`, `dest_hashes`) - the vecs contain any hashes
/// that couldn't be sent (if channels provided) or all hashes (if no channels).
pub fn diff_with_progress_and_hash_streaming<F>(
    source: &[FileEntry],
    dest: &[FileEntry],
    options: &DiffOptions,
    progress_callback: F,
    source_hash_tx: Option<HashSender>,
    dest_hash_tx: Option<HashSender>,
) -> (DiffResult, Vec<ComputedHash>, Vec<ComputedHash>)
where
    F: Fn(DiffProgress) + Send + Sync,
{
    if options.use_checksum {
        let config = ChecksumDiffConfig {
            source,
            dest,
            options,
            source_hash_tx,
            dest_hash_tx,
        };
        checksum::diff_with_checksums(config, ProgressMode::Callback(&progress_callback))
    } else {
        (
            diff_with_metadata(source, dest, options),
            Vec::new(),
            Vec::new(),
        )
    }
}

/// Compare two directory scans with atomic progress tracking for real-time display
///
/// This variant accepts an `AtomicProgress` tracker that can be polled by a separate
/// display thread for real-time progress updates during long file hashing operations.
///
/// # Arguments
/// * `progress` - Shared atomic progress tracker (caller should start `ProgressDisplay` before calling)
/// * `source_hash_tx` - Channel to send source file hashes (optional)
/// * `dest_hash_tx` - Channel to send dest file hashes (optional)
///
/// Returns (`DiffResult`, `source_hashes`, `dest_hashes`)
pub fn diff_with_atomic_progress(
    source: &[FileEntry],
    dest: &[FileEntry],
    options: &DiffOptions,
    progress: Arc<AtomicProgress>,
    source_hash_tx: Option<HashSender>,
    dest_hash_tx: Option<HashSender>,
) -> (DiffResult, Vec<ComputedHash>, Vec<ComputedHash>) {
    if options.use_checksum {
        let config = ChecksumDiffConfig {
            source,
            dest,
            options,
            source_hash_tx,
            dest_hash_tx,
        };
        checksum::diff_with_checksums(config, ProgressMode::<fn(DiffProgress)>::Atomic(progress))
    } else {
        (
            diff_with_metadata(source, dest, options),
            Vec::new(),
            Vec::new(),
        )
    }
}

/// Fast comparison using only metadata (size + mtime)
fn diff_with_metadata(
    source: &[FileEntry],
    dest: &[FileEntry],
    options: &DiffOptions,
) -> DiffResult {
    // Build HashMap of dest paths for O(1) lookup
    let dest_map: HashMap<&Path, &FileEntry> = dest.iter().map(|e| (e.path.as_path(), e)).collect();

    let mut result = DiffResult::new();

    // Check each source file against destination
    for src_entry in source {
        match dest_map.get(src_entry.path.as_path()) {
            Some(dest_entry) => {
                let is_same = files_match_by_metadata(src_entry, dest_entry);

                if is_same {
                    if options.include_identical {
                        result.push(DiffEntry::identical(
                            src_entry.path.clone(),
                            src_entry.size,
                            src_entry.mtime,
                        ));
                    } else {
                        result.identical_count += 1;
                    }
                } else {
                    result.push(DiffEntry::modified(
                        src_entry.path.clone(),
                        src_entry.size,
                        src_entry.mtime,
                        dest_entry.size,
                        dest_entry.mtime,
                    ));
                }
            }
            None => {
                result.push(DiffEntry::added(
                    src_entry.path.clone(),
                    src_entry.size,
                    src_entry.mtime,
                ));
            }
        }
    }

    // Find files only in destination (removed/orphans)
    let source_map: HashMap<&Path, &FileEntry> =
        source.iter().map(|e| (e.path.as_path(), e)).collect();

    for dest_entry in dest {
        if !source_map.contains_key(dest_entry.path.as_path()) {
            result.push(DiffEntry::removed(
                dest_entry.path.clone(),
                dest_entry.size,
                dest_entry.mtime,
            ));
        }
    }

    result
}

/// Check if two files match by metadata (size + mtime)
fn files_match_by_metadata(src: &FileEntry, dest: &FileEntry) -> bool {
    src.size == dest.size && src.mtime == dest.mtime
}

#[cfg(test)]
#[path = "compare_test.rs"]
mod tests;
