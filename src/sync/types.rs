//! Types for sync job orchestration

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use std::sync::RwLock;

use crate::hasher::HashAlgorithm;

/// Options for a sync job
#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// Use checksums instead of mtime+size for diff comparison
    pub use_checksum: bool,
    /// Verify checksums after each file copy
    pub verify: bool,
    /// Delete files in destination that don't exist in source
    pub delete_orphans: bool,
    /// Hash algorithm for checksums
    pub hash_algorithm: HashAlgorithm,
    /// Maximum depth to scan
    pub max_depth: Option<usize>,
    /// Number of parallel copy threads (0 = auto, based on CPU count)
    pub parallel_copies: usize,
    /// Preserve directory permissions (file permissions are always preserved)
    pub preserve_permissions: bool,
    /// Patterns to exclude from sync (e.g., "*.tmp", "node_modules")
    pub exclude_patterns: Vec<String>,
    /// Dry run mode: show what would be done without actually copying/deleting
    pub dry_run: bool,
    /// Use chunked transfer for large files (enables resume)
    pub chunked: bool,
    /// Minimum file size to use chunked transfer (default: 100MB)
    pub chunk_threshold: u64,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            use_checksum: false,
            verify: false,
            delete_orphans: false,
            hash_algorithm: HashAlgorithm::default(),
            max_depth: None,
            parallel_copies: 0,
            preserve_permissions: false,
            exclude_patterns: Vec::new(),
            dry_run: false,
            chunked: false,
            chunk_threshold: 100 * 1024 * 1024, // 100MB
        }
    }
}

/// Progress of a sync job
#[derive(Debug, Clone)]
pub struct SyncProgress {
    /// Current phase of the sync
    pub phase: SyncPhase,
    /// Total files to transfer
    pub total_files: usize,
    /// Files transferred so far
    pub files_done: usize,
    /// Total bytes to transfer
    pub total_bytes: u64,
    /// Bytes transferred so far
    pub bytes_done: u64,
    /// Current file being processed (if any)
    pub current_file: Option<PathBuf>,
    /// Number of files to delete (if delete_orphans is true)
    pub files_to_delete: usize,
    /// Number of files deleted so far
    pub files_deleted: usize,
    /// Number of errors encountered
    pub errors: usize,
}

impl SyncProgress {
    /// Overall percentage complete (0.0 to 100.0)
    pub fn percent(&self) -> f64 {
        if self.total_bytes == 0 {
            if self.total_files == 0 {
                100.0
            } else {
                (self.files_done as f64 / self.total_files as f64) * 100.0
            }
        } else {
            (self.bytes_done as f64 / self.total_bytes as f64) * 100.0
        }
    }
}

/// Current phase of sync operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPhase {
    /// Scanning directories (source and dest in parallel)
    Scanning,
    /// Computing diff
    Diffing,
    /// Transferring files
    Transferring,
    /// Deleting orphan files
    Deleting,
    /// Completed
    Complete,
}

impl std::fmt::Display for SyncPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncPhase::Scanning => write!(f, "Scanning"),
            SyncPhase::Diffing => write!(f, "Computing diff"),
            SyncPhase::Transferring => write!(f, "Transferring"),
            SyncPhase::Deleting => write!(f, "Deleting orphans"),
            SyncPhase::Complete => write!(f, "Complete"),
        }
    }
}

/// Result of a completed sync job
#[must_use]
#[derive(Debug)]
pub struct SyncResult {
    /// Number of files transferred
    pub files_transferred: usize,
    /// Bytes transferred
    pub bytes_transferred: u64,
    /// Number of files deleted
    pub files_deleted: usize,
    /// Number of files that were already up to date
    pub files_unchanged: usize,
    /// Number of errors
    pub errors: usize,
    /// Total duration
    pub duration: std::time::Duration,
    /// Number of directories whose permissions were synced (if preserve_permissions enabled)
    pub dirs_permissions_synced: usize,
    /// Diff summary
    pub diff_summary: DiffSummary,
    /// Whether this was a resumed job
    pub was_resumed: bool,
    /// Source folder stats
    pub source_stats: FolderStats,
    /// Destination folder stats (after sync)
    pub dest_stats: FolderStats,
}

/// Stats about a folder
#[derive(Debug, Clone, Default)]
pub struct FolderStats {
    /// Number of files
    pub files: usize,
    /// Total size in bytes
    pub bytes: u64,
}

impl FolderStats {
    /// Create from a list of file entries
    pub fn from_entries(entries: &[crate::scanner::FileEntry]) -> Self {
        Self {
            files: entries.len(),
            bytes: entries.iter().map(|e| e.size).sum(),
        }
    }
}

/// Summary of the diff
#[derive(Debug, Clone, Default)]
pub struct DiffSummary {
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
    pub identical: usize,
}

/// Error type for sync jobs
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Scan error: {0}")]
    ScanError(#[from] crate::scanner::ScanError),

    #[error("Copy error for {path}: {message}")]
    CopyFailed { path: String, message: String },

    #[error("Delete error for {path}: {message}")]
    DeleteFailed { path: String, message: String },

    #[error("Source directory does not exist: {0}")]
    SourceNotFound(PathBuf),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

// =============================================================================
// Atomic Progress for Sync Operations
// =============================================================================

/// Phase constants for atomic storage
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPhaseAtomic {
    Scanning = 0,
    Diffing = 1,
    Transferring = 2,
    Deleting = 3,
    Complete = 4,
}

impl From<u32> for SyncPhaseAtomic {
    fn from(v: u32) -> Self {
        match v {
            0 => SyncPhaseAtomic::Scanning,
            1 => SyncPhaseAtomic::Diffing,
            2 => SyncPhaseAtomic::Transferring,
            3 => SyncPhaseAtomic::Deleting,
            4 => SyncPhaseAtomic::Complete,
            _ => SyncPhaseAtomic::Scanning,
        }
    }
}

impl From<SyncPhase> for SyncPhaseAtomic {
    fn from(phase: SyncPhase) -> Self {
        match phase {
            SyncPhase::Scanning => SyncPhaseAtomic::Scanning,
            SyncPhase::Diffing => SyncPhaseAtomic::Diffing,
            SyncPhase::Transferring => SyncPhaseAtomic::Transferring,
            SyncPhase::Deleting => SyncPhaseAtomic::Deleting,
            SyncPhase::Complete => SyncPhaseAtomic::Complete,
        }
    }
}

/// Thread-safe atomic progress tracker for sync operations
///
/// This can be shared between the sync job and the FFI layer,
/// allowing real-time progress polling without callbacks.
#[derive(Debug)]
pub struct SyncAtomicProgress {
    // Phase tracking
    phase: AtomicU32,

    // Scanning phase counters
    files_scanned: AtomicUsize,

    // Diffing phase info
    files_to_compare: AtomicUsize,
    files_compared: AtomicUsize,

    // Transfer phase counters
    files_total: AtomicUsize,
    files_done: AtomicUsize,
    bytes_total: AtomicU64,
    bytes_done: AtomicU64,

    // Skipped files (already identical)
    files_skipped: AtomicUsize,
    bytes_skipped: AtomicU64,

    // Delete phase counters
    files_to_delete: AtomicUsize,
    files_deleted: AtomicUsize,

    // Error tracking
    errors: AtomicUsize,

    // Current file being processed
    current_file: RwLock<Option<String>>,

    // Status message (for diffing phase, etc.)
    status_message: RwLock<Option<String>>,

    // Timing
    started_at: Instant,

    // Cancellation flag
    cancelled: std::sync::atomic::AtomicBool,
}

impl SyncAtomicProgress {
    /// Create a new sync progress tracker
    pub fn new() -> Self {
        Self {
            phase: AtomicU32::new(SyncPhaseAtomic::Scanning as u32),
            files_scanned: AtomicUsize::new(0),
            files_to_compare: AtomicUsize::new(0),
            files_compared: AtomicUsize::new(0),
            files_total: AtomicUsize::new(0),
            files_done: AtomicUsize::new(0),
            bytes_total: AtomicU64::new(0),
            bytes_done: AtomicU64::new(0),
            files_skipped: AtomicUsize::new(0),
            bytes_skipped: AtomicU64::new(0),
            files_to_delete: AtomicUsize::new(0),
            files_deleted: AtomicUsize::new(0),
            errors: AtomicUsize::new(0),
            current_file: RwLock::new(None),
            status_message: RwLock::new(None),
            started_at: Instant::now(),
            cancelled: std::sync::atomic::AtomicBool::new(false),
        }
    }

    // ==================== Phase Management ====================

    /// Set the current phase
    pub fn set_phase(&self, phase: SyncPhaseAtomic) {
        self.phase.store(phase as u32, Ordering::Release);
    }

    /// Get the current phase
    pub fn phase(&self) -> SyncPhaseAtomic {
        SyncPhaseAtomic::from(self.phase.load(Ordering::Acquire))
    }

    // ==================== Scanning Phase ====================

    /// Increment files scanned counter
    #[inline]
    pub fn add_scanned(&self, count: usize) {
        self.files_scanned.fetch_add(count, Ordering::Relaxed);
    }

    /// Set files scanned (for batch updates)
    pub fn set_files_scanned(&self, count: usize) {
        self.files_scanned.store(count, Ordering::Relaxed);
    }

    /// Get files scanned
    #[inline]
    pub fn files_scanned(&self) -> usize {
        self.files_scanned.load(Ordering::Relaxed)
    }

    // ==================== Diffing Phase ====================

    /// Set the number of files to compare
    pub fn set_files_to_compare(&self, count: usize) {
        self.files_to_compare.store(count, Ordering::Relaxed);
    }

    /// Increment files compared
    #[inline]
    pub fn add_compared(&self, count: usize) {
        self.files_compared.fetch_add(count, Ordering::Relaxed);
    }

    /// Get files to compare
    pub fn files_to_compare(&self) -> usize {
        self.files_to_compare.load(Ordering::Relaxed)
    }

    /// Get files compared
    pub fn files_compared(&self) -> usize {
        self.files_compared.load(Ordering::Relaxed)
    }

    // ==================== Transfer Phase ====================

    /// Set transfer totals (called after diff is complete)
    pub fn set_transfer_totals(&self, files: usize, bytes: u64) {
        self.files_total.store(files, Ordering::Relaxed);
        self.bytes_total.store(bytes, Ordering::Relaxed);
    }

    /// Add bytes transferred
    #[inline]
    pub fn add_bytes(&self, bytes: u64) {
        self.bytes_done.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Mark a file as transferred
    #[inline]
    pub fn file_done(&self) {
        self.files_done.fetch_add(1, Ordering::Relaxed);
    }

    /// Get files total
    pub fn files_total(&self) -> usize {
        self.files_total.load(Ordering::Relaxed)
    }

    /// Get files done
    pub fn files_done(&self) -> usize {
        self.files_done.load(Ordering::Relaxed)
    }

    /// Get bytes total
    pub fn bytes_total(&self) -> u64 {
        self.bytes_total.load(Ordering::Relaxed)
    }

    /// Get bytes done
    pub fn bytes_done(&self) -> u64 {
        self.bytes_done.load(Ordering::Relaxed)
    }

    // ==================== Skipped Files ====================

    /// Set skipped files (already identical)
    pub fn set_skipped(&self, files: usize, bytes: u64) {
        self.files_skipped.store(files, Ordering::Relaxed);
        self.bytes_skipped.store(bytes, Ordering::Relaxed);
    }

    /// Get files skipped
    pub fn files_skipped(&self) -> usize {
        self.files_skipped.load(Ordering::Relaxed)
    }

    /// Get bytes skipped
    pub fn bytes_skipped(&self) -> u64 {
        self.bytes_skipped.load(Ordering::Relaxed)
    }

    // ==================== Delete Phase ====================

    /// Set files to delete
    pub fn set_files_to_delete(&self, count: usize) {
        self.files_to_delete.store(count, Ordering::Relaxed);
    }

    /// Mark a file as deleted
    #[inline]
    pub fn file_deleted(&self) {
        self.files_deleted.fetch_add(1, Ordering::Relaxed);
    }

    /// Get files to delete
    pub fn files_to_delete(&self) -> usize {
        self.files_to_delete.load(Ordering::Relaxed)
    }

    /// Get files deleted
    pub fn files_deleted(&self) -> usize {
        self.files_deleted.load(Ordering::Relaxed)
    }

    // ==================== Error Tracking ====================

    /// Add an error
    #[inline]
    pub fn add_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Get error count
    pub fn errors(&self) -> usize {
        self.errors.load(Ordering::Relaxed)
    }

    // ==================== Current File / Status ====================

    /// Set current file being processed
    pub fn set_current_file(&self, path: &str) {
        if let Ok(mut current) = self.current_file.write() {
            *current = Some(path.to_string());
        }
    }

    /// Clear current file
    pub fn clear_current_file(&self) {
        if let Ok(mut current) = self.current_file.write() {
            *current = None;
        }
    }

    /// Get current file
    pub fn current_file(&self) -> Option<String> {
        self.current_file.read().ok()?.clone()
    }

    /// Set status message (for phases without file-by-file progress)
    pub fn set_status(&self, message: &str) {
        if let Ok(mut status) = self.status_message.write() {
            *status = Some(message.to_string());
        }
    }

    /// Clear status message
    pub fn clear_status(&self) {
        if let Ok(mut status) = self.status_message.write() {
            *status = None;
        }
    }

    /// Get status message
    pub fn status_message(&self) -> Option<String> {
        self.status_message.read().ok()?.clone()
    }

    // ==================== Cancellation ====================

    /// Request cancellation
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Check if cancellation was requested
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    // ==================== Timing ====================

    /// Get elapsed time in seconds
    pub fn elapsed_secs(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }

    // ==================== Percentage ====================

    /// Get overall percentage (depends on phase)
    pub fn percent(&self) -> f64 {
        match self.phase() {
            SyncPhaseAtomic::Scanning => {
                // Indeterminate during scanning
                0.0
            }
            SyncPhaseAtomic::Diffing => {
                let total = self.files_to_compare();
                if total == 0 {
                    0.0
                } else {
                    (self.files_compared() as f64 / total as f64) * 100.0
                }
            }
            SyncPhaseAtomic::Transferring => {
                let total = self.bytes_total();
                if total == 0 {
                    let files_total = self.files_total();
                    if files_total == 0 {
                        100.0
                    } else {
                        (self.files_done() as f64 / files_total as f64) * 100.0
                    }
                } else {
                    (self.bytes_done() as f64 / total as f64) * 100.0
                }
            }
            SyncPhaseAtomic::Deleting => {
                let total = self.files_to_delete();
                if total == 0 {
                    100.0
                } else {
                    (self.files_deleted() as f64 / total as f64) * 100.0
                }
            }
            SyncPhaseAtomic::Complete => 100.0,
        }
    }
}

impl Default for SyncAtomicProgress {
    fn default() -> Self {
        Self::new()
    }
}

// Make SyncAtomicProgress safe to share across threads
unsafe impl Send for SyncAtomicProgress {}
unsafe impl Sync for SyncAtomicProgress {}
