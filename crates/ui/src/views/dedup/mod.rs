mod actions;
pub(super) mod delegate;
mod render;

use std::path::PathBuf;
use std::sync::Arc;

use gpui::*;
use gpui_component::table::TableState;

use dedup::DedupProgress;

use crate::services::SearchService;

use delegate::DedupDelegate;

// -- Events emitted to the parent ZeroApp ------------------------------------

pub enum DedupEvent {
    ScanStarted(Arc<DedupProgress>),
    ScanFinished,
}

impl EventEmitter<DedupEvent> for DedupView {}

// -- Types -------------------------------------------------------------------

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

// -- View state --------------------------------------------------------------

pub struct DedupView {
    pub(super) table: Option<Entity<TableState<DedupDelegate>>>,
    pub(super) scanning: bool,
    pub(super) deleting: bool,
    pub(super) scan_complete: bool,
    pub(super) scan_path: Option<PathBuf>,
    pub(super) scan_status: Option<String>,
    pub(super) confirm_delete: bool,
    pub(super) active_filter: DedupFilter,
    pub(super) search: Entity<SearchService>,
    #[allow(dead_code)]
    focus_handle: FocusHandle,
}

impl DedupView {
    pub fn new(search: Entity<SearchService>, cx: &mut Context<Self>) -> Self {
        Self {
            table: None,
            scanning: false,
            deleting: false,
            scan_complete: false,
            scan_path: dirs::home_dir(),
            scan_status: None,
            confirm_delete: false,
            active_filter: DedupFilter::All,
            search,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Set the root path for the next dedup scan (e.g. when scoped to a folder).
    pub fn set_scan_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.scan_path = Some(path);
        cx.notify();
    }
}
