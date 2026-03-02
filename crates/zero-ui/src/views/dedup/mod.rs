mod actions;
mod render;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::*;

use zero::dedup::DedupProgress;

// -- Events emitted to the parent ZeroApp ------------------------------------

pub enum DedupEvent {
    ScanStarted(Arc<DedupProgress>),
    ScanFinished,
}

impl EventEmitter<DedupEvent> for DedupView {}

// -- Types -------------------------------------------------------------------

/// Sort order for duplicate groups.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) enum DedupSort {
    /// By wasted space (most waste first).
    #[default]
    Savings,
    /// By individual file size (largest first).
    Size,
    /// By number of copies (most copies first).
    Count,
    /// Alphabetical by filename.
    Name,
}

impl DedupSort {
    pub fn label(self) -> &'static str {
        match self {
            Self::Savings => "Savings",
            Self::Size => "Size",
            Self::Count => "Count",
            Self::Name => "Name",
        }
    }

    pub const ALL: &[DedupSort] = &[Self::Savings, Self::Size, Self::Count, Self::Name];
}

/// File type filter for dedup scans.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum DedupFilter {
    All,
    Images,
    Videos,
    Audio,
    Documents,
    Code,
    Archives,
}

impl DedupFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Images => "Images",
            Self::Videos => "Videos",
            Self::Audio => "Audio",
            Self::Documents => "Documents",
            Self::Code => "Code",
            Self::Archives => "Archives",
        }
    }

    pub const ALL_FILTERS: &[DedupFilter] = &[
        Self::All,
        Self::Images,
        Self::Videos,
        Self::Audio,
        Self::Documents,
        Self::Code,
        Self::Archives,
    ];
}

pub(super) struct DuplicateGroup {
    #[allow(dead_code)]
    pub hash: String,
    pub size: u64,
    pub files: Vec<PathBuf>,
    pub expanded: bool,
}

// -- View state --------------------------------------------------------------

pub struct DedupView {
    pub(super) groups: Vec<DuplicateGroup>,
    pub(super) selected_for_deletion: HashSet<PathBuf>,
    pub(super) scanning: bool,
    pub(super) deleting: bool,
    pub(super) scan_complete: bool,
    pub(super) scan_path: Option<PathBuf>,
    pub(super) scan_status: Option<String>,
    pub(super) confirm_delete: bool,
    pub(super) active_filter: DedupFilter,
    pub(super) sort_order: DedupSort,
    #[allow(dead_code)]
    focus_handle: FocusHandle,
}

impl DedupView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            groups: Vec::new(),
            selected_for_deletion: HashSet::new(),
            scanning: false,
            deleting: false,
            scan_complete: false,
            scan_path: dirs::home_dir(),
            scan_status: None,
            confirm_delete: false,
            active_filter: DedupFilter::All,
            sort_order: DedupSort::default(),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Set the root path for the next dedup scan (e.g. when scoped to a folder).
    pub fn set_scan_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.scan_path = Some(path);
        cx.notify();
    }
}
