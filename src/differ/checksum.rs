//! Checksum-based diff comparison
//!
//! Provides thorough file comparison using cryptographic hashes.
//! Supports multiple progress tracking modes and hash caching.
//!
//! ## Panic Safety (Early Exit Strategy)
//!
//! Parallel operations are wrapped in catch_unwind to ensure
//! clean process exit on fatal errors.

use std::collections::HashMap;
use std::panic;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use rayon::prelude::*;

use crate::hasher::{HashAlgorithm, hash_file_with_atomic_progress, hash_file_with_buffer};
use crate::progress::AtomicProgress;
use crate::scanner::FileEntry;

use super::compare::{ComputedHash, DiffOptions, DiffProgress};
use super::result::{DiffEntry, DiffResult};

/// Configuration for checksum diff operation
pub(crate) struct ChecksumDiffConfig<'a> {
    pub source: &'a [FileEntry],
    pub dest: &'a [FileEntry],
    pub options: &'a DiffOptions,
    pub source_hash_tx: Option<Sender<ComputedHash>>,
    pub dest_hash_tx: Option<Sender<ComputedHash>>,
}

/// Progress tracking mode for checksum operations
#[allow(dead_code)]
pub(crate) enum ProgressMode<'a, F: Fn(DiffProgress) + Send + Sync> {
    /// No progress tracking
    None,
    /// Callback-based progress
    Callback(&'a F),
    /// Atomic progress for real-time display
    Atomic(Arc<AtomicProgress>),
}

/// Perform checksum-based diff with configurable progress tracking
pub(crate) fn diff_with_checksums<F>(
    config: ChecksumDiffConfig<'_>,
    progress_mode: ProgressMode<'_, F>,
) -> (DiffResult, Vec<ComputedHash>, Vec<ComputedHash>)
where
    F: Fn(DiffProgress) + Send + Sync,
{
    let ChecksumDiffConfig {
        source,
        dest,
        options,
        source_hash_tx,
        dest_hash_tx,
    } = config;

    // Collected hashes for caching (thread-safe)
    let source_hashes: Arc<Mutex<Vec<ComputedHash>>> = Arc::new(Mutex::new(Vec::new()));
    let dest_hashes: Arc<Mutex<Vec<ComputedHash>>> = Arc::new(Mutex::new(Vec::new()));

    // Wrap channels in Arc for sharing across threads
    let source_tx: Option<Arc<Sender<ComputedHash>>> = source_hash_tx.map(Arc::new);
    let dest_tx: Option<Arc<Sender<ComputedHash>>> = dest_hash_tx.map(Arc::new);

    // Build HashMap of dest paths for O(1) lookup
    let dest_map: HashMap<&Path, &FileEntry> =
        HashMap::from_iter(dest.iter().map(|e| (e.path.as_path(), e)));

    // Find files that exist in both and need checksum comparison
    // (same size, so we can't tell from metadata alone)
    let pairs_to_compare: Vec<(&FileEntry, &FileEntry)> = source
        .iter()
        .filter_map(|src_entry| {
            dest_map
                .get(src_entry.path.as_path())
                .and_then(|dest_entry| {
                    if src_entry.size == dest_entry.size {
                        Some((src_entry, *dest_entry))
                    } else {
                        None
                    }
                })
        })
        .collect();

    // Perform parallel hash comparison based on progress mode
    let comparison_results = match progress_mode {
        ProgressMode::None => hash_pairs_no_progress(&pairs_to_compare, options.hash_algorithm),
        ProgressMode::Callback(callback) => hash_pairs_with_callback(
            &pairs_to_compare,
            options.hash_algorithm,
            callback,
            &source_hashes,
            &dest_hashes,
            &source_tx,
            &dest_tx,
        ),
        ProgressMode::Atomic(progress) => hash_pairs_with_atomic_progress(
            &pairs_to_compare,
            options.hash_algorithm,
            progress,
            &source_hashes,
            &dest_hashes,
            &source_tx,
            &dest_tx,
        ),
    };

    // Build result map from parallel comparisons
    let comparison_map: HashMap<&Path, bool> = comparison_results
        .iter()
        .map(|(src, _, is_same)| (src.path.as_path(), *is_same))
        .collect();

    // Build the final result
    let result = build_diff_result(source, dest, &dest_map, &comparison_map, options);

    // Extract collected hashes
    let src_hashes = extract_hashes(source_hashes);
    let dst_hashes = extract_hashes(dest_hashes);

    (result, src_hashes, dst_hashes)
}

/// Simple checksum diff without progress tracking or hash collection
pub(crate) fn diff_with_checksums_simple(
    source: &[FileEntry],
    dest: &[FileEntry],
    options: &DiffOptions,
) -> DiffResult {
    // Build HashMap of dest paths for O(1) lookup
    let dest_map: HashMap<&Path, &FileEntry> =
        HashMap::from_iter(dest.iter().map(|e| (e.path.as_path(), e)));

    // Find files that need checksum comparison
    let pairs_to_compare: Vec<(&FileEntry, &FileEntry)> = source
        .iter()
        .filter_map(|src_entry| {
            dest_map
                .get(src_entry.path.as_path())
                .and_then(|dest_entry| {
                    if src_entry.size == dest_entry.size {
                        Some((src_entry, *dest_entry))
                    } else {
                        None
                    }
                })
        })
        .collect();

    // Parallel hash comparison
    let comparison_results = hash_pairs_no_progress(&pairs_to_compare, options.hash_algorithm);

    // Build result map
    let comparison_map: HashMap<&Path, bool> = comparison_results
        .iter()
        .map(|(src, _, is_same)| (src.path.as_path(), *is_same))
        .collect();

    build_diff_result(source, dest, &dest_map, &comparison_map, options)
}

/// Hash pairs without progress tracking
fn hash_pairs_no_progress<'a>(
    pairs: &[(&'a FileEntry, &'a FileEntry)],
    algorithm: HashAlgorithm,
) -> Vec<(&'a FileEntry, &'a FileEntry, bool)> {
    let parallel_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        pairs
            .par_iter()
            .map(|(src, dest)| {
                let is_same = compare_files_by_hash(src, dest, algorithm);
                (*src, *dest, is_same)
            })
            .collect::<Vec<_>>()
    }));

    match parallel_result {
        Ok(results) => results,
        Err(panic_payload) => {
            eprintln!("\n⚠ Fatal error during parallel diff comparison");
            panic::resume_unwind(panic_payload);
        }
    }
}

/// Hash pairs with callback-based progress
fn hash_pairs_with_callback<'a, F>(
    pairs: &[(&'a FileEntry, &'a FileEntry)],
    algorithm: HashAlgorithm,
    callback: &F,
    source_hashes: &Arc<Mutex<Vec<ComputedHash>>>,
    dest_hashes: &Arc<Mutex<Vec<ComputedHash>>>,
    source_tx: &Option<Arc<Sender<ComputedHash>>>,
    dest_tx: &Option<Arc<Sender<ComputedHash>>>,
) -> Vec<(&'a FileEntry, &'a FileEntry, bool)>
where
    F: Fn(DiffProgress) + Send + Sync,
{
    let files_total = pairs.len();
    let bytes_total: u64 = pairs.iter().map(|(s, d)| s.size + d.size).sum();

    let files_processed = Arc::new(AtomicUsize::new(0));
    let bytes_processed = Arc::new(AtomicU64::new(0));
    let current_file = Arc::new(Mutex::new(String::new()));
    let current_file_size = Arc::new(AtomicU64::new(0));

    let parallel_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        pairs
            .par_iter()
            .map(|(src_entry, dest_entry)| {
                // Update current file info
                {
                    let mut cf = current_file.lock().unwrap();
                    *cf = src_entry.path.to_string_lossy().to_string();
                }
                current_file_size.store(src_entry.size + dest_entry.size, Ordering::Relaxed);

                // Compare files and collect computed hashes
                let (is_same, src_hash, dest_hash) = compare_files_by_hash_and_collect(
                    src_entry,
                    dest_entry,
                    algorithm,
                    &bytes_processed,
                );

                // Send hashes to channels or collect them
                send_or_collect_hash(src_hash, source_tx, source_hashes);
                send_or_collect_hash(dest_hash, dest_tx, dest_hashes);

                // Update file count
                files_processed.fetch_add(1, Ordering::Relaxed);

                // Call progress callback
                callback(DiffProgress {
                    files_processed: files_processed.load(Ordering::Relaxed),
                    files_total,
                    bytes_processed: bytes_processed.load(Ordering::Relaxed),
                    bytes_total,
                    current_file: src_entry.path.to_string_lossy().to_string(),
                    current_file_size: src_entry.size + dest_entry.size,
                });

                (*src_entry, *dest_entry, is_same)
            })
            .collect::<Vec<_>>()
    }));

    match parallel_result {
        Ok(results) => results,
        Err(panic_payload) => {
            eprintln!("\n⚠ Fatal error during parallel diff comparison");
            panic::resume_unwind(panic_payload);
        }
    }
}

/// Hash pairs with atomic progress tracking
fn hash_pairs_with_atomic_progress<'a>(
    pairs: &[(&'a FileEntry, &'a FileEntry)],
    algorithm: HashAlgorithm,
    progress: Arc<AtomicProgress>,
    source_hashes: &Arc<Mutex<Vec<ComputedHash>>>,
    dest_hashes: &Arc<Mutex<Vec<ComputedHash>>>,
    source_tx: &Option<Arc<Sender<ComputedHash>>>,
    dest_tx: &Option<Arc<Sender<ComputedHash>>>,
) -> Vec<(&'a FileEntry, &'a FileEntry, bool)> {
    // Set up progress tracker with totals
    let files_total = pairs.len();
    let bytes_total: u64 = pairs.iter().map(|(s, d)| s.size + d.size).sum();
    progress.set_files_total(files_total);
    progress.set_bytes_total(bytes_total);

    let parallel_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        pairs
            .par_iter()
            .map(|(src_entry, dest_entry)| {
                // Update current file info
                progress.set_current_file(&src_entry.path.to_string_lossy());

                // Compare files with per-chunk progress updates
                let (is_same, src_hash, dest_hash) = compare_files_by_hash_with_progress(
                    src_entry, dest_entry, algorithm, &progress,
                );

                // Send hashes to channels or collect them
                send_or_collect_hash(src_hash, source_tx, source_hashes);
                send_or_collect_hash(dest_hash, dest_tx, dest_hashes);

                // Mark file complete
                progress.file_done();

                (*src_entry, *dest_entry, is_same)
            })
            .collect::<Vec<_>>()
    }));

    match parallel_result {
        Ok(results) => results,
        Err(panic_payload) => {
            eprintln!("\n⚠ Fatal error during parallel diff comparison");
            panic::resume_unwind(panic_payload);
        }
    }
}

/// Build the final DiffResult from comparison results
fn build_diff_result(
    source: &[FileEntry],
    dest: &[FileEntry],
    dest_map: &HashMap<&Path, &FileEntry>,
    comparison_map: &HashMap<&Path, bool>,
    options: &DiffOptions,
) -> DiffResult {
    let mut result = DiffResult::new();

    // Process source files
    for src_entry in source {
        match dest_map.get(src_entry.path.as_path()) {
            Some(dest_entry) => {
                if src_entry.size != dest_entry.size {
                    // Different sizes = definitely different
                    result.push(DiffEntry::modified(
                        src_entry.path.clone(),
                        src_entry.size,
                        src_entry.mtime,
                        dest_entry.size,
                        dest_entry.mtime,
                    ));
                } else {
                    // Same size - use hash comparison result
                    let is_same = comparison_map
                        .get(src_entry.path.as_path())
                        .copied()
                        .unwrap_or(false);

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
        HashMap::from_iter(source.iter().map(|e| (e.path.as_path(), e)));

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

/// Send hash to channel or collect in vec as fallback
fn send_or_collect_hash(
    hash: Option<ComputedHash>,
    tx: &Option<Arc<Sender<ComputedHash>>>,
    hashes: &Arc<Mutex<Vec<ComputedHash>>>,
) {
    if let Some(h) = hash {
        let sent = tx
            .as_ref()
            .map(|tx| tx.send(h.clone()).is_ok())
            .unwrap_or(false);
        if !sent
            && let Ok(mut vec) = hashes.lock() {
                vec.push(h);
            }
    }
}

/// Extract hashes from Arc<Mutex<Vec>>
fn extract_hashes(hashes: Arc<Mutex<Vec<ComputedHash>>>) -> Vec<ComputedHash> {
    match Arc::try_unwrap(hashes) {
        Ok(mutex) => mutex.into_inner().unwrap_or_default(),
        Err(arc) => arc.lock().unwrap().clone(),
    }
}

/// Compare two files by computing and comparing their hashes
fn compare_files_by_hash(src: &FileEntry, dest: &FileEntry, algorithm: HashAlgorithm) -> bool {
    thread_local! {
        static BUFFER: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(vec![0u8; 128 * 1024]);
    }

    BUFFER.with(|buf| {
        let mut buffer = buf.borrow_mut();

        // If both already have hashes, compare them
        match algorithm {
            HashAlgorithm::Xxh3 => {
                if let (Some(src_hash), Some(dest_hash)) = (&src.hash_xxh3, &dest.hash_xxh3) {
                    return src_hash == dest_hash;
                }
            }
            HashAlgorithm::Blake3 => {
                if let (Some(src_hash), Some(dest_hash)) = (&src.hash, &dest.hash) {
                    return src_hash == dest_hash;
                }
            }
        }

        // Compute hashes
        let src_hash = hash_file_with_buffer(&src.absolute_path, algorithm, &mut buffer);
        let dest_hash = hash_file_with_buffer(&dest.absolute_path, algorithm, &mut buffer);

        match (src_hash, dest_hash) {
            (Ok(src_result), Ok(dest_result)) => src_result.hash == dest_result.hash,
            _ => false,
        }
    })
}

/// Compare files by hash, collecting computed hashes for caching
fn compare_files_by_hash_and_collect(
    src: &FileEntry,
    dest: &FileEntry,
    algorithm: HashAlgorithm,
    bytes_processed: &Arc<AtomicU64>,
) -> (bool, Option<ComputedHash>, Option<ComputedHash>) {
    thread_local! {
        static BUFFER: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(vec![0u8; 128 * 1024]);
    }

    BUFFER.with(|buf| {
        let mut buffer = buf.borrow_mut();

        // If both already have hashes, compare them (no need to return computed hashes)
        match algorithm {
            HashAlgorithm::Xxh3 => {
                if let (Some(src_hash), Some(dest_hash)) = (&src.hash_xxh3, &dest.hash_xxh3) {
                    bytes_processed.fetch_add(src.size + dest.size, Ordering::Relaxed);
                    return (src_hash == dest_hash, None, None);
                }
            }
            HashAlgorithm::Blake3 => {
                if let (Some(src_hash), Some(dest_hash)) = (&src.hash, &dest.hash) {
                    bytes_processed.fetch_add(src.size + dest.size, Ordering::Relaxed);
                    return (src_hash == dest_hash, None, None);
                }
            }
        }

        // Hash source file
        let src_hash_result = hash_file_with_buffer(&src.absolute_path, algorithm, &mut buffer);
        bytes_processed.fetch_add(src.size, Ordering::Relaxed);

        // Hash dest file
        let dest_hash_result = hash_file_with_buffer(&dest.absolute_path, algorithm, &mut buffer);
        bytes_processed.fetch_add(dest.size, Ordering::Relaxed);

        match (&src_hash_result, &dest_hash_result) {
            (Ok(src_result), Ok(dest_result)) => {
                let is_same = src_result.hash == dest_result.hash;

                let src_computed = ComputedHash {
                    path: src.path.to_string_lossy().to_string(),
                    size: src.size,
                    mtime: src.mtime,
                    hash: src_result.hash.clone(),
                    algorithm,
                };
                let dest_computed = ComputedHash {
                    path: dest.path.to_string_lossy().to_string(),
                    size: dest.size,
                    mtime: dest.mtime,
                    hash: dest_result.hash.clone(),
                    algorithm,
                };

                (is_same, Some(src_computed), Some(dest_computed))
            }
            _ => (false, None, None),
        }
    })
}

/// Compare files by hash with per-chunk atomic progress updates
fn compare_files_by_hash_with_progress(
    src: &FileEntry,
    dest: &FileEntry,
    algorithm: HashAlgorithm,
    progress: &Arc<AtomicProgress>,
) -> (bool, Option<ComputedHash>, Option<ComputedHash>) {
    thread_local! {
        static BUFFER: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(vec![0u8; 128 * 1024]);
    }

    BUFFER.with(|buf| {
        let mut buffer = buf.borrow_mut();

        // If both already have hashes, compare them
        match algorithm {
            HashAlgorithm::Xxh3 => {
                if let (Some(src_hash), Some(dest_hash)) = (&src.hash_xxh3, &dest.hash_xxh3) {
                    progress.add_bytes(src.size + dest.size);
                    return (src_hash == dest_hash, None, None);
                }
            }
            HashAlgorithm::Blake3 => {
                if let (Some(src_hash), Some(dest_hash)) = (&src.hash, &dest.hash) {
                    progress.add_bytes(src.size + dest.size);
                    return (src_hash == dest_hash, None, None);
                }
            }
        }

        // Hash source file with per-chunk progress
        let src_hash_result =
            hash_file_with_atomic_progress(&src.absolute_path, algorithm, &mut buffer, progress);

        // Hash dest file with per-chunk progress
        let dest_hash_result =
            hash_file_with_atomic_progress(&dest.absolute_path, algorithm, &mut buffer, progress);

        match (&src_hash_result, &dest_hash_result) {
            (Ok(src_result), Ok(dest_result)) => {
                let is_same = src_result.hash == dest_result.hash;

                let src_computed = ComputedHash {
                    path: src.path.to_string_lossy().to_string(),
                    size: src.size,
                    mtime: src.mtime,
                    hash: src_result.hash.clone(),
                    algorithm,
                };
                let dest_computed = ComputedHash {
                    path: dest.path.to_string_lossy().to_string(),
                    size: dest.size,
                    mtime: dest.mtime,
                    hash: dest_result.hash.clone(),
                    algorithm,
                };

                (is_same, Some(src_computed), Some(dest_computed))
            }
            _ => (false, None, None),
        }
    })
}
