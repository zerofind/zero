//! Sync job orchestration
//!
//! Combines scanner, differ, and transfer to sync only changed files.
//!
//! ## Features
//!
//! - Exclude patterns: skip files matching patterns like "*.tmp", "node_modules"
//! - Dry-run mode: preview what would happen without making changes
//!
//! ## Panic Safety (Early Exit Strategy)
//!
//! All rayon thread pools are configured with panic handlers to ensure
//! clean process exit on fatal errors. This prevents zombie rayon threads
//! from hanging around consuming CPU after a crash (e.g., stack overflow).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::cache::{CacheEntry, ChecksumBatch, ControlDb, StorageDb};
use crate::differ::{
    ComputedHash, DiffOptions, DiffStatus, diff_with_atomic_progress, diff_with_options,
};
use crate::progress::{AtomicProgress, ProgressDisplay};
use crate::scanner::ScanFilter;
use crate::scanner::{FileEntry, ScanOptions};
use crate::transfer::{CopyOptions, sync_dir_permissions};

// Re-export types from types module
pub use super::types::{
    DiffSummary, FolderStats, SyncAtomicProgress, SyncError, SyncOptions, SyncPhase,
    SyncPhaseAtomic, SyncProgress, SyncResult,
};

/// A sync job that synchronizes source to destination
pub struct SyncJob {
    source: PathBuf,
    dest: PathBuf,
    options: SyncOptions,
    /// Control database for storage registry (when --verify is used)
    control_db: Option<Box<ControlDb>>,
    /// Source storage database for file hashes
    source_storage_db: Option<Box<StorageDb>>,
    /// Dest storage database for file hashes
    dest_storage_db: Option<Box<StorageDb>>,
    /// Source storage ID in control DB
    source_storage_id: Option<i64>,
    /// Dest storage ID in control DB
    dest_storage_id: Option<i64>,
}

impl SyncJob {
    /// Create a new sync job
    pub fn new(
        source: impl AsRef<Path>,
        dest: impl AsRef<Path>,
        options: SyncOptions,
    ) -> Result<Self, SyncError> {
        let source = source.as_ref().to_path_buf();
        let dest = dest.as_ref().to_path_buf();

        if !source.exists() {
            return Err(SyncError::SourceNotFound(source));
        }

        // Create destination if it doesn't exist
        if !dest.exists() {
            fs::create_dir_all(&dest)?;
        }

        // Initialize cache if verify mode is enabled
        let (control_db, source_storage_db, dest_storage_db, source_storage_id, dest_storage_id) =
            if options.verify {
                // Canonicalize paths to avoid duplicate storage entries
                let canonical_source = source.canonicalize().unwrap_or_else(|_| source.clone());
                let canonical_dest = dest.canonicalize().unwrap_or_else(|_| dest.clone());

                let control = ControlDb::open().map_err(|e| {
                    SyncError::IoError(std::io::Error::other(format!(
                        "Failed to open control database: {}",
                        e
                    )))
                })?;

                let source_storage = control
                    .get_or_create_storage(&canonical_source, None)
                    .map_err(|e| {
                        SyncError::IoError(std::io::Error::other(format!(
                            "Failed to create source storage: {}",
                            e
                        )))
                    })?;

                let dest_storage = control
                    .get_or_create_storage(&canonical_dest, None)
                    .map_err(|e| {
                        SyncError::IoError(std::io::Error::other(format!(
                            "Failed to create dest storage: {}",
                            e
                        )))
                    })?;

                let source_db = control.open_storage_db(&source_storage).map_err(|e| {
                    SyncError::IoError(std::io::Error::other(format!(
                        "Failed to open source storage database: {}",
                        e
                    )))
                })?;

                let dest_db = control.open_storage_db(&dest_storage).map_err(|e| {
                    SyncError::IoError(std::io::Error::other(format!(
                        "Failed to open dest storage database: {}",
                        e
                    )))
                })?;

                (
                    Some(Box::new(control)),
                    Some(Box::new(source_db)),
                    Some(Box::new(dest_db)),
                    Some(source_storage.id),
                    Some(dest_storage.id),
                )
            } else {
                (None, None, None, None, None)
            };

        Ok(Self {
            source,
            dest,
            options,
            control_db,
            source_storage_db,
            dest_storage_db,
            source_storage_id,
            dest_storage_id,
        })
    }

    /// Get cache database info (source_db_path, dest_db_path) if caching is enabled
    pub fn get_cache_info(&self) -> Option<(String, String)> {
        match (&self.source_storage_db, &self.dest_storage_db) {
            (Some(src_db), Some(dest_db)) => Some((
                src_db.db_path().to_string_lossy().to_string(),
                dest_db.db_path().to_string_lossy().to_string(),
            )),
            _ => None,
        }
    }

    /// Check if destination is empty (optimization: skip dest scan)
    fn is_dest_empty(&self) -> bool {
        if !self.dest.exists() {
            return true;
        }

        // Check if directory has any entries
        match fs::read_dir(&self.dest) {
            Ok(mut entries) => entries.next().is_none(),
            Err(_) => true,
        }
    }

    /// Run the sync job with atomic progress tracking
    ///
    /// This is the preferred method for FFI integration. The `SyncAtomicProgress`
    /// is updated continuously and can be polled from another thread.
    ///
    /// # Arguments
    /// * `atomic_progress` - Shared progress tracker that is updated in real-time
    ///
    /// # Returns
    /// The sync result, or an error if the sync failed or was cancelled
    pub fn run_with_atomic_progress(
        &self,
        sync_atomic_progress: Arc<SyncAtomicProgress>,
    ) -> Result<SyncResult, SyncError> {
        self.run_internal(Some(sync_atomic_progress), None)
    }

    /// Run the sync job with a callback for progress updates
    ///
    /// This is the original CLI-oriented method. For FFI, prefer `run_with_atomic_progress`.
    pub fn run(&self, mut on_progress: impl FnMut(&SyncProgress)) -> Result<SyncResult, SyncError> {
        // Wrap the callback - we can't use Box<dyn> due to lifetime issues
        // so we call run_internal_with_callback directly
        self.run_internal_with_callback(None, &mut on_progress)
    }

    /// Internal run implementation with atomic progress only (for FFI)
    fn run_internal(
        &self,
        sync_progress: Option<Arc<SyncAtomicProgress>>,
        _on_progress: Option<()>, // Placeholder to keep signature similar
    ) -> Result<SyncResult, SyncError> {
        self.run_internal_with_callback(sync_progress, &mut |_| {})
    }

    /// Internal run implementation that supports both atomic progress and callbacks
    fn run_internal_with_callback(
        &self,
        sync_progress: Option<Arc<SyncAtomicProgress>>,
        on_progress: &mut impl FnMut(&SyncProgress),
    ) -> Result<SyncResult, SyncError> {
        let start_time = Instant::now();

        let mut progress = SyncProgress {
            phase: SyncPhase::Scanning,
            total_files: 0,
            files_done: 0,
            total_bytes: 0,
            bytes_done: 0,
            current_file: None,
            files_to_delete: 0,
            files_deleted: 0,
            errors: 0,
        };

        // Update progress callback
        on_progress(&progress);

        // Update atomic progress if provided
        if let Some(ref sp) = sync_progress {
            sp.set_phase(SyncPhaseAtomic::Scanning);
        }

        // Build scan filter with exclude patterns
        let scan_filter = if self.options.exclude_patterns.is_empty() {
            ScanFilter::new() // Default macOS ignores
        } else {
            ScanFilter::new().with_patterns(self.options.exclude_patterns.iter().cloned())
        };

        let scan_options = ScanOptions {
            max_depth: self.options.max_depth.unwrap_or(usize::MAX),
            filter: scan_filter,
            ..Default::default()
        };

        // Optimization: Check if dest is empty to skip dest scan
        let dest_is_empty = self.is_dest_empty();

        // Check for cancellation
        if let Some(ref sp) = sync_progress
            && sp.is_cancelled()
        {
            return Err(SyncError::IoError(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Sync cancelled",
            )));
        }

        // Phase 1: Scan source and destination
        let scan = super::phases::phase_scan(
            &self.source,
            &self.dest,
            scan_options,
            dest_is_empty,
            &sync_progress,
        )?;
        let source_files = scan.source_files;
        let dest_files = scan.dest_files;
        let source_stats = scan.source_stats;
        let dest_bytes_before = scan.dest_bytes_before;

        progress.phase = SyncPhase::Diffing;
        on_progress(&progress);
        if let Some(ref sp) = sync_progress {
            sp.set_phase(SyncPhaseAtomic::Diffing);
            sp.set_status(&format!(
                "Comparing {} vs {} files...",
                source_files.len(),
                dest_files.len()
            ));
        }

        // Check for cancellation
        if let Some(ref sp) = sync_progress
            && sp.is_cancelled()
        {
            return Err(SyncError::IoError(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Sync cancelled",
            )));
        }

        // Pre-populate cached hashes to avoid re-hashing unchanged files
        let (source_files, dest_files) = if self.options.use_checksum {
            if let (Some(source_db), Some(dest_db)) =
                (&self.source_storage_db, &self.dest_storage_db)
            {
                eprint!("Loading cached checksums...");
                let _ = std::io::stderr().flush();

                let mut src_cache_hits = 0usize;
                let mut dest_cache_hits = 0usize;

                // Populate source file hashes from cache
                let mut new_source_files = Vec::with_capacity(source_files.len());
                for mut entry in source_files {
                    if let Ok(Some(cached)) = source_db.lookup_valid_file(
                        &entry.path.to_string_lossy(),
                        entry.size as i64,
                        entry.mtime as i64,
                    ) && let Some(hash_bytes) = cached.hash_xxh3
                        && hash_bytes.len() == 16
                    {
                        let mut arr = [0u8; 16];
                        arr.copy_from_slice(&hash_bytes);
                        entry.hash_xxh3 = Some(arr);
                        src_cache_hits += 1;
                    }
                    new_source_files.push(entry);
                }

                // Populate dest file hashes from cache
                let mut new_dest_files = Vec::with_capacity(dest_files.len());
                for mut entry in dest_files {
                    if let Ok(Some(cached)) = dest_db.lookup_valid_file(
                        &entry.path.to_string_lossy(),
                        entry.size as i64,
                        entry.mtime as i64,
                    ) && let Some(hash_bytes) = cached.hash_xxh3
                        && hash_bytes.len() == 16
                    {
                        let mut arr = [0u8; 16];
                        arr.copy_from_slice(&hash_bytes);
                        entry.hash_xxh3 = Some(arr);
                        dest_cache_hits += 1;
                    }
                    new_dest_files.push(entry);
                }

                eprintln!(
                    " {} source, {} dest cached",
                    src_cache_hits, dest_cache_hits
                );
                (new_source_files, new_dest_files)
            } else {
                (source_files, dest_files)
            }
        } else {
            (source_files, dest_files)
        };

        // Compute diff
        let diff_start = Instant::now();

        let diff_options = DiffOptions {
            use_checksum: self.options.use_checksum,
            include_identical: false,
            hash_algorithm: self.options.hash_algorithm,
        };

        // Use progress reporting for checksum-based comparison (can be slow)
        let diff_result = if self.options.use_checksum {
            // Calculate how many file pairs need comparison (exist in both with same size)
            let dest_map: std::collections::HashMap<&Path, &FileEntry> =
                dest_files.iter().map(|e| (e.path.as_path(), e)).collect();
            let pairs_to_compare: Vec<_> = source_files
                .iter()
                .filter_map(|src| {
                    dest_map.get(src.path.as_path()).and_then(|dest| {
                        if src.size == dest.size {
                            Some((src, *dest))
                        } else {
                            None
                        }
                    })
                })
                .collect();
            let pairs_count = pairs_to_compare.len();

            // Files not in pairs = already known to need copying (different size or missing)
            let to_copy = source_files.len() - pairs_count;

            // Count how many pairs have BOTH source and dest cached (instant compare)
            let cached_pairs = pairs_to_compare
                .iter()
                .filter(|(src, dest)| src.hash_xxh3.is_some() && dest.hash_xxh3.is_some())
                .count();
            let to_hash = pairs_count - cached_pairs;

            eprintln!(
                "Comparing {} of {} files ({} cached, {} to hash, {} to copy)...",
                pairs_count,
                source_files.len(),
                cached_pairs,
                to_hash,
                to_copy
            );

            // Create channels for streaming hashes to background writer
            let (source_hash_tx, source_hash_rx) = mpsc::channel::<ComputedHash>();
            let (dest_hash_tx, dest_hash_rx) = mpsc::channel::<ComputedHash>();

            // Get DB paths for the writer thread (it will open its own connections)
            let source_db_path = self
                .source_storage_db
                .as_ref()
                .map(|db| db.db_path().to_path_buf());
            let dest_db_path = self
                .dest_storage_db
                .as_ref()
                .map(|db| db.db_path().to_path_buf());

            // Spawn background writer thread for incremental caching
            let writer_handle = thread::spawn(move || {
                // Track how many hashes we've written
                let mut source_count = 0usize;
                let mut dest_count = 0usize;

                // Open DB connections in this thread
                let source_db = if let Some(ref path) = source_db_path {
                    StorageDb::open_at(path).ok()
                } else {
                    None
                };
                let dest_db = if let Some(ref path) = dest_db_path {
                    StorageDb::open_at(path).ok()
                } else {
                    None
                };

                let mut source_batch = ChecksumBatch::new();
                let mut dest_batch = ChecksumBatch::new();
                let mut last_flush = Instant::now();
                let flush_interval = Duration::from_millis(500);
                let batch_size = 100;

                loop {
                    // Try to receive from both channels with timeout
                    let timeout = Duration::from_millis(50);

                    // Check source channel
                    match source_hash_rx.recv_timeout(timeout) {
                        Ok(hash) => {
                            if let Some(ref db) = source_db {
                                let entry = CacheEntry::with_xxh3(
                                    hash.path,
                                    hash.size as i64,
                                    hash.mtime as i64,
                                    hash.hash,
                                );
                                source_batch.add(entry);
                                source_count += 1;

                                // Flush if batch is full
                                if source_batch.pending_count() >= batch_size {
                                    let _ = db.flush_batch(&mut source_batch);
                                }
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            // Channel closed, flush remaining and exit
                            if let Some(ref db) = source_db
                                && source_batch.pending_count() > 0
                            {
                                let _ = db.flush_batch(&mut source_batch);
                            }
                            if let Some(ref db) = dest_db
                                && dest_batch.pending_count() > 0
                            {
                                let _ = db.flush_batch(&mut dest_batch);
                            }
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }

                    // Check dest channel (non-blocking)
                    while let Ok(hash) = dest_hash_rx.try_recv() {
                        if let Some(ref db) = dest_db {
                            let entry = CacheEntry::with_xxh3(
                                hash.path,
                                hash.size as i64,
                                hash.mtime as i64,
                                hash.hash,
                            );
                            dest_batch.add(entry);
                            dest_count += 1;

                            // Flush if batch is full
                            if dest_batch.pending_count() >= batch_size {
                                let _ = db.flush_batch(&mut dest_batch);
                            }
                        }
                    }

                    // Time-based flush: flush if interval elapsed and there's data
                    if last_flush.elapsed() >= flush_interval {
                        if let Some(ref db) = source_db
                            && source_batch.pending_count() > 0
                        {
                            let _ = db.flush_batch(&mut source_batch);
                        }
                        if let Some(ref db) = dest_db
                            && dest_batch.pending_count() > 0
                        {
                            let _ = db.flush_batch(&mut dest_batch);
                        }
                        last_flush = Instant::now();
                    }
                }

                (source_count, dest_count)
            });

            // Create atomic progress tracker for real-time display
            let diff_progress = Arc::new(AtomicProgress::new(0, 0));

            // Start progress display thread for real-time updates during hashing
            let diff_display = ProgressDisplay::builder(Arc::clone(&diff_progress))
                .with_prefix("Hashing")
                .start();

            // Spawn a thread to forward diff progress to sync progress for FFI polling
            let diff_progress_forwarder = if let Some(ref sp) = sync_progress {
                let sp_clone = Arc::clone(sp);
                let diff_progress_clone = Arc::clone(&diff_progress);
                let files_to_compare = pairs_count;
                sp_clone.set_files_to_compare(files_to_compare);
                Some(thread::spawn(move || {
                    let mut last_files = 0usize;
                    loop {
                        let current_files = diff_progress_clone.files_done();
                        let current_file = diff_progress_clone.current_file();

                        // Update sync progress with diff progress
                        if current_files != last_files {
                            sp_clone.add_compared(current_files - last_files);
                            last_files = current_files;
                        }

                        // Update current file/status for FFI
                        if !current_file.is_empty() {
                            sp_clone.set_current_file(&current_file);
                        }

                        // Check if diff is done (files_done >= files_total or files_total is set and we're done)
                        let total = diff_progress_clone.files_total();
                        if total > 0 && current_files >= total {
                            break;
                        }

                        thread::sleep(Duration::from_millis(50));
                    }
                }))
            } else {
                None
            };

            let (result, remaining_source_hashes, remaining_dest_hashes) =
                diff_with_atomic_progress(
                    &source_files,
                    &dest_files,
                    &diff_options,
                    diff_progress,
                    Some(source_hash_tx),
                    Some(dest_hash_tx),
                );

            // Stop progress display
            diff_display.stop();

            // Wait for progress forwarder to finish
            if let Some(handle) = diff_progress_forwarder {
                let _ = handle.join();
            }
            eprintln!(
                "Comparing files... done ({:.1}s)",
                diff_start.elapsed().as_secs_f64()
            );

            // Wait for writer thread to finish flushing
            // Channels are dropped here, signaling writer to exit
            let (cached_source, cached_dest) = writer_handle.join().unwrap_or((0, 0));

            // Handle any remaining hashes that couldn't be sent
            if (!remaining_source_hashes.is_empty() || !remaining_dest_hashes.is_empty())
                && let (Some(source_db), Some(dest_db)) =
                    (&self.source_storage_db, &self.dest_storage_db)
            {
                let mut source_batch = ChecksumBatch::new();
                for h in &remaining_source_hashes {
                    let entry = CacheEntry::with_xxh3(
                        h.path.clone(),
                        h.size as i64,
                        h.mtime as i64,
                        h.hash.clone(),
                    );
                    source_batch.add(entry);
                }
                let _ = source_db.flush_batch(&mut source_batch);

                let mut dest_batch = ChecksumBatch::new();
                for h in &remaining_dest_hashes {
                    let entry = CacheEntry::with_xxh3(
                        h.path.clone(),
                        h.size as i64,
                        h.mtime as i64,
                        h.hash.clone(),
                    );
                    dest_batch.add(entry);
                }
                let _ = dest_db.flush_batch(&mut dest_batch);
            }

            let total_cached = cached_source
                + cached_dest
                + remaining_source_hashes.len()
                + remaining_dest_hashes.len();
            if total_cached > 0 {
                eprintln!("Cached {} checksums incrementally", total_cached);
            }

            result
        } else {
            // Metadata comparison is fast, but update status for FFI progress
            if let Some(ref sp) = sync_progress {
                sp.set_files_to_compare(source_files.len());
                sp.set_status(&format!(
                    "Comparing {} source files with {} dest files...",
                    source_files.len(),
                    dest_files.len()
                ));
            }
            eprint!(
                "Comparing {} source files with {} dest files...",
                source_files.len(),
                dest_files.len()
            );
            let _ = std::io::stderr().flush();
            let result = diff_with_options(&source_files, &dest_files, &diff_options);
            eprintln!(" done ({:.1}s)", diff_start.elapsed().as_secs_f64());
            result
        };

        let diff_summary = DiffSummary {
            added: diff_result.added_count,
            modified: diff_result.modified_count,
            removed: diff_result.removed_count,
            identical: diff_result.identical_count,
        };

        // Update status for building transfer list (can be slow with many files)
        if let Some(ref sp) = sync_progress {
            sp.set_status("Building transfer list...");
        }

        // Build HashMap of diff entries for O(1) lookup instead of O(n) linear search
        // This is critical for performance with large file counts (476K+ files)
        let diff_entry_map: std::collections::HashMap<&Path, &DiffStatus> = diff_result
            .entries
            .iter()
            .map(|e| (e.path.as_path(), &e.status))
            .collect();

        // Build list of files to transfer (added + modified) - now O(n) instead of O(n*m)
        let files_to_transfer: Vec<&FileEntry> = source_files
            .iter()
            .filter(|f| {
                diff_entry_map
                    .get(f.path.as_path())
                    .map(|status| status.needs_transfer())
                    .unwrap_or(false)
            })
            .collect();

        // Files to delete (only if option is set)
        let files_to_delete: Vec<PathBuf> = if self.options.delete_orphans {
            diff_result
                .entries_by_status(DiffStatus::Removed)
                .map(|e| e.path.clone())
                .collect()
        } else {
            Vec::new()
        };

        // Load completed files if resuming (do this BEFORE showing "Copying" message)
        // With cache-only approach, files with matching hashes in dest are already
        // excluded by the diff (they show as "identical"). No separate resume state needed.
        let files_to_copy: Vec<&FileEntry> = files_to_transfer.to_vec();

        progress.total_files = files_to_transfer.len();
        progress.total_bytes = diff_result.bytes_to_transfer;
        progress.files_to_delete = files_to_delete.len();
        progress.files_done = 0;
        progress.bytes_done = 0;
        // Calculate bytes for identical files (for skipped reporting) - now O(n) instead of O(n*m)
        let identical_bytes: u64 = source_files
            .iter()
            .filter(|f| {
                diff_entry_map
                    .get(f.path.as_path())
                    .map(|status| **status == DiffStatus::Identical)
                    .unwrap_or(false)
            })
            .map(|f| f.size)
            .sum();

        progress.phase = SyncPhase::Transferring;
        on_progress(&progress);

        // Update atomic progress with transfer totals and skipped info
        if let Some(ref sp) = sync_progress {
            sp.set_phase(SyncPhaseAtomic::Transferring);
            sp.set_transfer_totals(files_to_transfer.len(), diff_result.bytes_to_transfer);
            sp.set_skipped(diff_result.identical_count, identical_bytes);
            sp.clear_status();
        }

        // Check for cancellation
        if let Some(ref sp) = sync_progress
            && sp.is_cancelled()
        {
            return Err(SyncError::IoError(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Sync cancelled",
            )));
        }

        // Copy options
        let copy_options = CopyOptions {
            verify: self.options.verify,
            hash_algorithm: self.options.hash_algorithm,
        };

        // DRY RUN MODE: Show what would be done without actually doing it
        if self.options.dry_run {
            return Ok(super::phases::phase_dry_run(
                &files_to_copy,
                &files_to_delete,
                diff_result.bytes_to_transfer,
                diff_result.identical_count,
                source_stats,
                dest_files.len(),
                dest_bytes_before,
                diff_summary,
                start_time,
            ));
        }

        // Phase 5: Parallel file transfer
        let transfer_ctx = super::phases::TransferContext {
            dest: &self.dest,
            copy_options,
            chunked: self.options.chunked,
            chunk_threshold: self.options.chunk_threshold,
            verify: self.options.verify,
            parallel_copies: self.options.parallel_copies,
            sync_progress: sync_progress.clone(),
        };

        let transfer = super::phases::phase_transfer(&transfer_ctx, &files_to_copy);

        progress.files_done += transfer.files_transferred;
        progress.bytes_done += transfer.bytes_transferred;
        progress.errors = transfer.errors;

        // Phase 6: Cache flush (when verify mode is enabled)
        if self.options.verify
            && !transfer.collected_hashes.is_empty()
            && let (
                Some(control_db),
                Some(source_db),
                Some(dest_db),
                Some(source_id),
                Some(dest_id),
            ) = (
                &self.control_db,
                &self.source_storage_db,
                &self.dest_storage_db,
                self.source_storage_id,
                self.dest_storage_id,
            )
        {
            super::phases::phase_cache_flush(
                control_db,
                source_db,
                dest_db,
                source_id,
                dest_id,
                &transfer.collected_hashes,
            );
        }

        // Phase 7: Delete orphan files
        let mut files_deleted = 0;
        if self.options.delete_orphans && !files_to_delete.is_empty() {
            progress.phase = SyncPhase::Deleting;
            on_progress(&progress);

            let (deleted, delete_errors) =
                super::phases::phase_delete(&self.dest, &files_to_delete, &sync_progress);
            files_deleted = deleted;
            progress.errors += delete_errors;
            progress.files_deleted = files_deleted;

            // Clean up empty directories
            self.cleanup_empty_dirs(&self.dest)?;
        }

        // Phase 8: Finalization
        progress.phase = SyncPhase::Complete;
        progress.current_file = None;
        on_progress(&progress);
        if let Some(ref sp) = sync_progress {
            sp.set_phase(SyncPhaseAtomic::Complete);
        }

        let dirs_permissions_synced = if self.options.preserve_permissions {
            match sync_dir_permissions(&self.source, &self.dest) {
                Ok(result) => result.dirs_synced,
                Err(e) => {
                    tracing::warn!("Failed to sync directory permissions: {}", e);
                    0
                }
            }
        } else {
            0
        };

        let dest_stats = FolderStats {
            files: dest_files.len() + diff_summary.added - files_deleted,
            bytes: dest_bytes_before + transfer.bytes_transferred,
        };

        Ok(SyncResult {
            files_transferred: transfer.files_transferred,
            bytes_transferred: transfer.bytes_transferred,
            files_deleted,
            files_unchanged: diff_result.identical_count,
            errors: progress.errors,
            duration: start_time.elapsed(),
            dirs_permissions_synced,
            diff_summary,
            was_resumed: false,
            source_stats,
            dest_stats,
        })
    }

    /// Clean up empty directories after deleting files
    fn cleanup_empty_dirs(&self, dir: &Path) -> Result<(), SyncError> {
        if !dir.is_dir() {
            return Ok(());
        }

        // Recursively process subdirectories first
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                // Skip the .zero state directory
                if path.file_name().map(|n| n == ".zero").unwrap_or(false) {
                    continue;
                }
                self.cleanup_empty_dirs(&path)?;
            }
        }

        // Try to remove this directory if empty (will fail if not empty, which is fine)
        // Don't try to remove the root dest directory
        if dir != self.dest {
            let _ = fs::remove_dir(dir);
        }

        Ok(())
    }
}

/// Convert hex string to bytes
pub(crate) fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, std::num::ParseIntError> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect()
}

/// Pre-allocate a file to the given size
///
/// This can improve write performance by:
/// - Avoiding fragmentation
/// - Ensuring disk space is available before writing
/// - Allowing the filesystem to optimize block allocation
pub(crate) fn preallocate_file(path: &Path, size: u64) -> std::io::Result<()> {
    // Create parent directories if needed
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }

    let file = fs::File::create(path)?;

    // Use platform-specific preallocation if available
    // macOS and most Unix: use ftruncate via set_len
    // This pre-extends the file to the target size
    file.set_len(size)?;

    Ok(())
}

#[cfg(test)]
#[path = "job_test.rs"]
mod tests;
