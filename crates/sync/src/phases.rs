//! Phase functions for sync pipeline decomposition
//!
//! Each phase is a standalone function with typed inputs/outputs,
//! making them independently testable and keeping the orchestration
//! in `job.rs` focused on sequencing.

use std::fs;
use std::io::Write;
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rayon::prelude::*;

use cache::{CacheEntry, ChecksumBatch, ControlDb, StorageDb};
use foundation::util::format_bytes;
use progress::{AtomicProgress, ProgressDisplay};
use scanner::{CrawlProgress, FileEntry, ScanOptions, scan_collect_with_progress};
use transfer::{
    ChunkedCopyOptions, CopyOptions, copy_chunked_with_resume, copy_file_with_hash_progress,
    copy_file_with_progress, get_partial_size, remove_partial, should_use_chunked,
};

use crate::types::{
    DiffSummary, FolderStats, SyncAtomicProgress, SyncError, SyncPhaseAtomic, SyncResult,
};

/// Type alias for collected hashes during sync (`rel_path`, size, mtime, `hash_bytes`)
type CollectedHashes = Mutex<Vec<(String, i64, i64, Vec<u8>)>>;

// ---------------------------------------------------------------------------
// Phase result types
// ---------------------------------------------------------------------------

/// Output of the scan phase
pub struct ScanResult {
    pub source_files: Vec<FileEntry>,
    pub dest_files: Vec<FileEntry>,
    pub source_stats: FolderStats,
    pub dest_bytes_before: u64,
}

/// Output of the transfer phase
pub struct TransferResult {
    pub files_transferred: usize,
    pub bytes_transferred: u64,
    pub errors: usize,
    pub collected_hashes: Vec<(String, i64, i64, Vec<u8>)>,
}

// ---------------------------------------------------------------------------
// Phase 1: Scanning
// ---------------------------------------------------------------------------

/// Scan source and destination directories in parallel.
///
/// Optimizes for empty destinations by skipping the dest scan entirely.
pub fn phase_scan(
    source: &Path,
    dest: &Path,
    scan_options: &ScanOptions,
    dest_is_empty: bool,
    sync_progress: Option<&Arc<SyncAtomicProgress>>,
) -> Result<ScanResult, SyncError> {
    eprint!("Scanning source...");
    let _ = std::io::stderr().flush();
    let scan_start = Instant::now();

    // Create progress trackers for scanning
    let source_scan_progress = Arc::new(CrawlProgress::new());
    let dest_scan_progress = Arc::new(CrawlProgress::new());

    // Spawn a thread to update atomic progress during scanning
    let scan_updater_handle = if let Some(sp) = sync_progress {
        let sp_clone = Arc::clone(sp);
        let source_progress = Arc::clone(&source_scan_progress);
        let dest_progress = Arc::clone(&dest_scan_progress);
        Some(thread::spawn(move || {
            while !sp_clone.is_cancelled() {
                let scanned = source_progress.files() + dest_progress.files();
                sp_clone.set_files_scanned(scanned);
                thread::sleep(Duration::from_millis(50));
                if source_progress.is_cancelled() && dest_progress.is_cancelled() {
                    break;
                }
            }
        }))
    } else {
        None
    };

    // Parallel scan of source and destination using rayon
    let (source_files, dest_files) = if dest_is_empty {
        let source_files = scan_collect_with_progress(
            source,
            scan_options,
            Some(Arc::clone(&source_scan_progress)),
        )?;

        source_scan_progress.cancel();
        eprintln!(
            " {} files ({:.1}s)",
            source_files.len(),
            scan_start.elapsed().as_secs_f64()
        );
        (source_files, Vec::new())
    } else {
        let source_path = source.to_path_buf();
        let dest_path = dest.to_path_buf();
        let src_progress = Arc::clone(&source_scan_progress);
        let dst_progress = Arc::clone(&dest_scan_progress);

        let scan_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            rayon::join(
                || scan_collect_with_progress(&source_path, scan_options, Some(src_progress)),
                || scan_collect_with_progress(&dest_path, scan_options, Some(dst_progress)),
            )
        }));

        source_scan_progress.cancel();
        dest_scan_progress.cancel();

        let (src_result, dest_result) = match scan_result {
            Ok(results) => results,
            Err(panic_payload) => {
                eprintln!("\n⚠ Fatal error during parallel directory scan");
                panic::resume_unwind(panic_payload);
            }
        };

        let source_files = src_result?;
        let dest_files = dest_result?;

        eprintln!(
            " {} files ({:.1}s)",
            source_files.len(),
            scan_start.elapsed().as_secs_f64()
        );
        eprint!("Scanning destination...");
        let _ = std::io::stderr().flush();
        eprintln!(" {} files", dest_files.len());

        (source_files, dest_files)
    };

    // Wait for scan updater thread to finish
    if let Some(handle) = scan_updater_handle {
        let _ = handle.join();
    }

    // Final update of files scanned
    if let Some(sp) = sync_progress {
        sp.set_files_scanned(source_files.len() + dest_files.len());
    }

    let source_stats = FolderStats::from_entries(&source_files);
    let dest_bytes_before: u64 = dest_files.iter().map(|e| e.size).sum();

    Ok(ScanResult {
        source_files,
        dest_files,
        source_stats,
        dest_bytes_before,
    })
}

// ---------------------------------------------------------------------------
// Phase 5: Transfer
// ---------------------------------------------------------------------------

/// Context for the parallel file transfer phase.
///
/// Replaces the 18-variable closure capture with a typed struct.
pub struct TransferContext<'a> {
    pub dest: &'a Path,
    pub copy_options: CopyOptions,
    pub chunked: bool,
    pub chunk_threshold: u64,
    pub verify: bool,
    pub parallel_copies: usize,
    pub sync_progress: Option<Arc<SyncAtomicProgress>>,
}

/// Execute parallel file transfer with progress tracking.
///
/// Returns the number of files transferred, bytes transferred, errors,
/// and collected hashes (for cache insertion when verify is enabled).
pub fn phase_transfer(ctx: &TransferContext<'_>, files_to_copy: &[&FileEntry]) -> TransferResult {
    let files_transferred = AtomicUsize::new(0);
    let bytes_transferred = AtomicU64::new(0);
    let errors = AtomicUsize::new(0);

    // Collect hashes for cache insertion (only when verify mode is enabled)
    let collected_hashes: Option<CollectedHashes> = if ctx.verify {
        Some(Mutex::new(Vec::new()))
    } else {
        None
    };

    // Configure thread pool with larger stack size to prevent overflow
    let pool = if ctx.parallel_copies > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(ctx.parallel_copies)
            .stack_size(8 * 1024 * 1024)
            .panic_handler(|_| {})
            .build()
            .ok()
    } else {
        rayon::ThreadPoolBuilder::new()
            .stack_size(8 * 1024 * 1024)
            .panic_handler(|_| {})
            .build()
            .ok()
    };

    // Progress tracking
    let last_progress_print = Mutex::new(Instant::now());
    let total_files_to_copy = files_to_copy.len();
    let grand_total_files = files_to_copy.len();
    let grand_total_bytes: u64 = files_to_copy.iter().map(|f| f.size).sum();
    let start_time_transfer = Instant::now();

    // Create atomic progress tracker for real-time byte updates
    let atomic_progress = Arc::new(AtomicProgress::new(total_files_to_copy, grand_total_bytes));

    // Track consecutive errors to detect drive disconnection
    let consecutive_errors = AtomicUsize::new(0);
    let abort_flag = AtomicUsize::new(0);

    let sync_progress_for_copy = ctx.sync_progress.clone();

    let copy_file_fn = |file: &&FileEntry| {
        if abort_flag.load(Ordering::Relaxed) == 1 {
            return;
        }

        if let Some(ref sp) = sync_progress_for_copy
            && sp.is_cancelled()
        {
            return;
        }

        let dest_path = ctx.dest.join(&file.path);

        // Create parent directory if needed
        if let Some(parent) = dest_path.parent()
            && !parent.exists()
        {
            let _ = fs::create_dir_all(parent);
        }

        let use_chunked = ctx.chunked && should_use_chunked(file.size, ctx.chunk_threshold);

        // For non-chunked transfers: remove partial files and pre-allocate
        if !use_chunked {
            if let Some(partial_size) = get_partial_size(&dest_path)
                && partial_size < file.size
            {
                let _ = remove_partial(&dest_path);
            }

            if file.size > 0 {
                let _ = crate::job::preallocate_file(&dest_path, file.size);
            }
        }

        // Set current file for progress display
        atomic_progress.set_current_file(&file.path.to_string_lossy());

        if let Some(ref sp) = sync_progress_for_copy {
            sp.set_current_file(&file.path.to_string_lossy());
        }

        // Copy the file
        let copy_result = if use_chunked {
            let chunked_options = ChunkedCopyOptions {
                chunk_size: 4 * 1024 * 1024,
                verify_after_copy: true,
                enable_resume: true,
                ..Default::default()
            };

            copy_chunked_with_resume(
                &file.absolute_path,
                &dest_path,
                &chunked_options,
                Some(&atomic_progress),
            )
            .map(|r| (r.bytes_transferred, r.file_hash, None))
            .map_err(|e| transfer::CopyError::CopyError {
                path: file.absolute_path.to_string_lossy().to_string(),
                source: std::io::Error::other(e.to_string()),
            })
        } else if ctx.verify {
            copy_file_with_hash_progress(
                &file.absolute_path,
                &dest_path,
                true,
                Some(&atomic_progress),
            )
            .map(|r| (r.bytes_copied, Some(r.source_hash), Some(r.source_mtime)))
        } else {
            copy_file_with_progress(
                &file.absolute_path,
                &dest_path,
                &ctx.copy_options,
                Some(&atomic_progress),
            )
            .map(|(p, h)| (p.bytes_copied, h, None))
        };

        match copy_result {
            Ok((_, hash, mtime)) => {
                // Collect hash for cache insertion
                if let (Some(h), Some(m), Some(hashes_mutex)) = (&hash, mtime, &collected_hashes)
                    && let Ok(hash_bytes) = crate::job::hex_to_bytes(h)
                    && let Ok(mut hashes) = hashes_mutex.lock()
                {
                    hashes.push((
                        file.path.to_string_lossy().to_string(),
                        file.size as i64,
                        m,
                        hash_bytes,
                    ));
                }

                let done = files_transferred.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = bytes_transferred.fetch_add(file.size, Ordering::Relaxed);

                atomic_progress.file_done();
                atomic_progress.clear_current_file();

                if let Some(ref sp) = sync_progress_for_copy {
                    sp.file_done();
                    sp.add_bytes(file.size);
                    sp.clear_current_file();
                }

                consecutive_errors.store(0, Ordering::Relaxed);

                // Print progress every 500ms or every 100 files
                let should_print = {
                    if let Ok(mut last) = last_progress_print.lock() {
                        if last.elapsed().as_millis() > 500 || done.is_multiple_of(100) {
                            *last = Instant::now();
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };

                if should_print {
                    print_transfer_progress(
                        done,
                        grand_total_files,
                        grand_total_bytes,
                        &atomic_progress,
                        &start_time_transfer,
                        &file.path,
                    );
                }
            }
            Err(e) => {
                errors.fetch_add(1, Ordering::Relaxed);
                atomic_progress.add_error();
                atomic_progress.clear_current_file();

                if let Some(ref sp) = sync_progress_for_copy {
                    sp.add_error();
                    sp.clear_current_file();
                }

                let consec = consecutive_errors.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::warn!(path = %file.path.display(), "Failed to copy: {}", e);

                if consec >= 50 {
                    abort_flag.store(1, Ordering::Relaxed);
                    eprintln!(
                        "\r\n⚠ Too many consecutive errors ({consec}) - source may be disconnected. Aborting...\n"
                    );
                }
            }
        }
    };

    // Start progress display thread
    let progress_display = ProgressDisplay::builder(Arc::clone(&atomic_progress))
        .with_prefix("Copying")
        .start();

    // Execute parallel copy with custom thread pool
    let parallel_result: Result<(), _> = if let Some(pool) = pool {
        panic::catch_unwind(panic::AssertUnwindSafe(|| {
            pool.install(|| {
                files_to_copy.par_iter().for_each(copy_file_fn);
            });
        }))
    } else {
        let fallback_pool = rayon::ThreadPoolBuilder::new()
            .stack_size(8 * 1024 * 1024)
            .panic_handler(|_| {})
            .build()
            .expect("Failed to create thread pool");
        panic::catch_unwind(panic::AssertUnwindSafe(|| {
            fallback_pool.install(|| {
                files_to_copy.par_iter().for_each(copy_file_fn);
            });
        }))
    };

    progress_display.stop();

    if let Err(panic_payload) = parallel_result {
        eprintln!("\n⚠ Fatal error during parallel file copy");
        panic::resume_unwind(panic_payload);
    }

    // Print final status
    let final_errors = errors.load(Ordering::Relaxed);
    let was_aborted = abort_flag.load(Ordering::Relaxed) == 1;

    if total_files_to_copy > 0 {
        let final_done = files_transferred.load(Ordering::Relaxed);
        let final_bytes = bytes_transferred.load(Ordering::Relaxed);
        let (final_size, total_size, unit) = if grand_total_bytes < 1_000_000_000 {
            (
                final_bytes as f64 / 1_000_000.0,
                grand_total_bytes as f64 / 1_000_000.0,
                "MB",
            )
        } else {
            (
                final_bytes as f64 / 1_000_000_000.0,
                grand_total_bytes as f64 / 1_000_000_000.0,
                "GB",
            )
        };

        if was_aborted {
            eprintln!(
                "\r[{final_done}/{grand_total_files}] ABORTED ({final_size:.1}/{total_size:.1} {unit}) - {final_errors} errors                    "
            );
        } else if final_errors > 0 {
            eprintln!(
                "\r[{final_done}/{grand_total_files}] ({final_size:.1}/{total_size:.1} {unit}) - Completed with {final_errors} errors              "
            );
        } else {
            eprintln!(
                "\r[{final_done}/{grand_total_files}] 100% ({final_size:.1}/{total_size:.1} {unit}) - Done!                              "
            );
        }
    }

    let final_hashes = collected_hashes
        .map(|m| m.into_inner().unwrap_or_default())
        .unwrap_or_default();

    TransferResult {
        files_transferred: files_transferred.load(Ordering::Relaxed),
        bytes_transferred: bytes_transferred.load(Ordering::Relaxed),
        errors: errors.load(Ordering::Relaxed),
        collected_hashes: final_hashes,
    }
}

/// Print real-time transfer progress line to stderr.
fn print_transfer_progress(
    done: usize,
    total_files: usize,
    total_bytes: u64,
    progress: &AtomicProgress,
    start_time: &Instant,
    current_path: &Path,
) {
    let realtime_bytes = progress.bytes_done();

    let percent = if total_bytes > 0 {
        (realtime_bytes as f64 / total_bytes as f64) * 100.0
    } else {
        100.0
    };

    let (done_size, total_size, unit) = if total_bytes < 1_000_000_000 {
        (
            realtime_bytes as f64 / 1_000_000.0,
            total_bytes as f64 / 1_000_000.0,
            "MB",
        )
    } else {
        (
            realtime_bytes as f64 / 1_000_000_000.0,
            total_bytes as f64 / 1_000_000_000.0,
            "GB",
        )
    };

    let elapsed = start_time.elapsed().as_secs_f64();
    let (speed_str, eta_str) = if realtime_bytes > 0 && elapsed > 1.0 {
        let bytes_per_sec = realtime_bytes as f64 / elapsed;
        let remaining_bytes = total_bytes.saturating_sub(realtime_bytes) as f64;
        let eta_secs = remaining_bytes / bytes_per_sec;

        let speed = if bytes_per_sec >= 1_000_000_000.0 {
            format!("{:.1} GB/s", bytes_per_sec / 1_000_000_000.0)
        } else if bytes_per_sec >= 1_000_000.0 {
            format!("{:.1} MB/s", bytes_per_sec / 1_000_000.0)
        } else if bytes_per_sec >= 1_000.0 {
            format!("{:.0} KB/s", bytes_per_sec / 1_000.0)
        } else {
            format!("{bytes_per_sec:.0} B/s")
        };

        let eta = if eta_secs < 60.0 {
            format!("~{eta_secs:.0}s left")
        } else if eta_secs < 3600.0 {
            format!("~{:.0}m left", eta_secs / 60.0)
        } else {
            format!("~{:.1}h left", eta_secs / 3600.0)
        };

        (speed, eta)
    } else {
        ("-- MB/s".to_string(), "calculating...".to_string())
    };

    let filename = current_path.to_string_lossy();
    let filename_display = if filename.chars().count() > 50 {
        let suffix: String = filename
            .chars()
            .rev()
            .take(47)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("...{suffix}")
    } else {
        filename.to_string()
    };

    eprint!(
        "\r  [{done}/{total_files}] {percent:.1}% ({done_size:.1}/{total_size:.1} {unit}) {speed_str} {eta_str} | {filename_display}        "
    );
    let _ = std::io::stderr().flush();
}

// ---------------------------------------------------------------------------
// Phase 7: Cache flush
// ---------------------------------------------------------------------------

/// Batch-insert collected hashes into cache databases.
pub fn phase_cache_flush(
    control_db: &ControlDb,
    source_db: &StorageDb,
    dest_db: &StorageDb,
    source_id: i64,
    dest_id: i64,
    hashes: &[(String, i64, i64, Vec<u8>)],
) {
    if hashes.is_empty() {
        return;
    }

    eprint!("Caching {} checksums...", hashes.len());
    let _ = std::io::stderr().flush();

    let cache_result: Result<(), ()> = {
        let mut source_batch = ChecksumBatch::new();
        let mut dest_batch = ChecksumBatch::new();

        for (path, size, mtime, hash_bytes) in hashes {
            let source_entry =
                CacheEntry::with_xxh3(path.clone(), *size, *mtime, hash_bytes.clone());
            source_batch.add(source_entry);

            let dest_entry = CacheEntry::with_xxh3(path.clone(), *size, *mtime, hash_bytes.clone());
            dest_batch.add(dest_entry);

            if source_batch.should_flush() {
                let _ = source_db.flush_batch(&mut source_batch);
            }
            if dest_batch.should_flush() {
                let _ = dest_db.flush_batch(&mut dest_batch);
            }
        }

        let _ = source_db.flush_batch(&mut source_batch);
        let _ = dest_db.flush_batch(&mut dest_batch);

        let source_count = source_db.count_files().unwrap_or(0);
        let source_bytes = source_db.total_bytes().unwrap_or(0);
        let _ = control_db.update_storage_stats(source_id, source_count, source_bytes);

        let dest_count = dest_db.count_files().unwrap_or(0);
        let dest_bytes = dest_db.total_bytes().unwrap_or(0);
        let _ = control_db.update_storage_stats(dest_id, dest_count, dest_bytes);

        Ok(())
    };

    if cache_result.is_ok() {
        eprintln!(" done");
    } else {
        eprintln!(" failed (checksums not cached)");
    }
}

// ---------------------------------------------------------------------------
// Phase 8: Delete orphans
// ---------------------------------------------------------------------------

/// Delete orphan files from destination in parallel.
///
/// Returns (`files_deleted`, `delete_errors`).
pub fn phase_delete(
    dest: &Path,
    files_to_delete: &[PathBuf],
    sync_progress: Option<&Arc<SyncAtomicProgress>>,
) -> (usize, usize) {
    if files_to_delete.is_empty() {
        return (0, 0);
    }

    if let Some(sp) = sync_progress {
        sp.set_phase(SyncPhaseAtomic::Deleting);
        sp.set_files_to_delete(files_to_delete.len());
    }

    let delete_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        files_to_delete
            .par_iter()
            .map(|rel_path| {
                let full_path = dest.join(rel_path);
                match fs::remove_file(&full_path) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        tracing::warn!(path = %rel_path.display(), "Failed to delete: {}", e);
                        Err(())
                    }
                }
            })
            .collect::<Vec<_>>()
    }));

    let delete_results: Vec<_> = match delete_result {
        Ok(results) => results,
        Err(panic_payload) => {
            eprintln!("\n⚠ Fatal error during parallel file deletion");
            panic::resume_unwind(panic_payload);
        }
    };

    let mut deleted = 0;
    let mut delete_errors = 0;
    for result in delete_results {
        match result {
            Ok(()) => deleted += 1,
            Err(()) => delete_errors += 1,
        }
    }

    (deleted, delete_errors)
}

// ---------------------------------------------------------------------------
// Phase 9: Dry run
// ---------------------------------------------------------------------------

/// Display dry-run results and return a `SyncResult` without executing transfers.
#[allow(clippy::too_many_arguments)]
pub fn phase_dry_run(
    files_to_copy: &[&FileEntry],
    files_to_delete: &[PathBuf],
    bytes_to_transfer: u64,
    identical_count: usize,
    source_stats: FolderStats,
    dest_files_count: usize,
    dest_bytes_before: u64,
    diff_summary: DiffSummary,
    start_time: Instant,
) -> SyncResult {
    eprintln!();
    eprintln!("=== DRY RUN RESULTS ===");
    eprintln!();

    if !files_to_copy.is_empty() {
        eprintln!(
            "Would copy {} files ({}):",
            files_to_copy.len(),
            format_bytes(bytes_to_transfer)
        );
        for (i, file) in files_to_copy.iter().take(20).enumerate() {
            eprintln!(
                "  {} {}",
                if i < 19 { "├" } else { "└" },
                file.path.display()
            );
        }
        if files_to_copy.len() > 20 {
            eprintln!("  ... and {} more files", files_to_copy.len() - 20);
        }
        eprintln!();
    }

    if !files_to_delete.is_empty() {
        eprintln!("Would delete {} orphan files:", files_to_delete.len());
        for (i, file) in files_to_delete.iter().take(20).enumerate() {
            eprintln!("  {} {}", if i < 19 { "├" } else { "└" }, file.display());
        }
        if files_to_delete.len() > 20 {
            eprintln!("  ... and {} more files", files_to_delete.len() - 20);
        }
        eprintln!();
    }

    if files_to_copy.is_empty() && files_to_delete.is_empty() {
        eprintln!("Nothing to do - directories are in sync!");
    }

    eprintln!("=== END DRY RUN ===");
    eprintln!();

    SyncResult {
        files_transferred: 0,
        bytes_transferred: 0,
        files_deleted: 0,
        files_unchanged: identical_count,
        errors: 0,
        source_stats,
        dest_stats: FolderStats {
            files: dest_files_count,
            bytes: dest_bytes_before,
        },
        diff_summary,
        duration: start_time.elapsed(),
        was_resumed: false,
        dirs_permissions_synced: 0,
    }
}
