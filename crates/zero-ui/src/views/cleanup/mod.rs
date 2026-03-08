pub(super) mod delegate;
mod detail;
mod view;

pub use view::CleanupView;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use gpui::EventEmitter;
use zero::profiles::CleanupGroup;

/// Lock-free progress counters for cleanup deletion.
pub struct CleanupProgress {
    pub files_done: AtomicU64,
    pub files_total: AtomicU64,
    pub failed: AtomicU64,
    pub cancelled: AtomicBool,
}

impl CleanupProgress {
    pub fn new(files_total: u64, _bytes_total: u64) -> Arc<Self> {
        Arc::new(Self {
            files_done: AtomicU64::new(0),
            files_total: AtomicU64::new(files_total),
            failed: AtomicU64::new(0),
            cancelled: AtomicBool::new(false),
        })
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

pub enum CleanupViewEvent {
    ScanStarted,
    ScanFinished,
    CleanStarted {
        message: String,
        progress: Arc<CleanupProgress>,
    },
    CleanFinished {
        ok: usize,
        failed: usize,
        cancelled: bool,
    },
    DrillInto(usize),
}

impl EventEmitter<CleanupViewEvent> for CleanupView {}

/// A cleanup group with summary info (for group overview)
pub(super) struct GroupResult {
    pub group: CleanupGroup,
    pub total_bytes: u64,
    pub item_count: usize,
    pub detail_loaded: bool,
    pub dev_dirs: Vec<DevDirEntry>,
    pub categories: Vec<CategoryResult>,
    /// Cached summary from initial scan to avoid re-querying on drill-in.
    pub cached_summary: Option<zero::cleanup::GroupSummary>,
}

/// Directory-level entry for Developer group (computed from index)
pub(super) struct DevDirEntry {
    pub path: PathBuf,
    pub total_bytes: u64,
    #[allow(dead_code)]
    pub file_count: usize,
    pub category_name: String,
}

/// Category scan result for display.
pub(super) struct CategoryResult {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub group: String,
    #[allow(dead_code)]
    pub file_count: usize,
    #[allow(dead_code)]
    pub total_bytes: u64,
    pub files: Vec<CategoryFileItem>,
}

/// A single file within a category result.
pub(super) struct CategoryFileItem {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
}
