use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::models::{ActiveView, SplitPane, ViewMode};

/// Persisted workspace — bookmarks and last-known path.
#[derive(Serialize, Deserialize, Clone)]
pub struct Workspace {
    pub name: String,
    pub pinned_bookmarks: Vec<PathBuf>,
    pub regular_bookmarks: Vec<PathBuf>,
    #[serde(default)]
    pub last_path: Option<PathBuf>,
    #[serde(default = "default_view_mode")]
    pub view_mode: String,
}

fn default_view_mode() -> String {
    "list".to_string()
}

impl Workspace {
    pub fn new_default(pinned: Vec<PathBuf>, regular: Vec<PathBuf>) -> Self {
        Self {
            name: "Default".to_string(),
            pinned_bookmarks: pinned,
            regular_bookmarks: regular,
            last_path: None,
            view_mode: default_view_mode(),
        }
    }

    pub fn new_with_defaults(name: impl Into<String>, pinned: Vec<PathBuf>) -> Self {
        Self {
            name: name.into(),
            pinned_bookmarks: pinned,
            regular_bookmarks: Vec::new(),
            last_path: None,
            view_mode: default_view_mode(),
        }
    }
}

/// Runtime-only snapshot of a workspace's navigation state.
/// Held in-memory for instant restore on switch — NOT persisted.
#[derive(Clone)]
pub struct WorkspaceSnapshot {
    pub nav_stack: Vec<ActiveView>,
    pub nav_idx: usize,
    pub current_path: PathBuf,
    pub active_view: ActiveView,
    pub view_mode: ViewMode,
    pub split_pane: Option<SplitPaneSnapshot>,
}

/// Snapshot of the split pane state for workspace restore.
#[derive(Clone)]
pub struct SplitPaneSnapshot {
    pub current_path: PathBuf,
    pub history: Vec<PathBuf>,
    pub history_idx: usize,
}

impl From<&SplitPane> for SplitPaneSnapshot {
    fn from(pane: &SplitPane) -> Self {
        Self {
            current_path: pane.current_path.clone(),
            history: pane.history.clone(),
            history_idx: pane.history_idx,
        }
    }
}

impl From<&SplitPaneSnapshot> for SplitPane {
    fn from(snap: &SplitPaneSnapshot) -> Self {
        Self {
            current_path: snap.current_path.clone(),
            history: snap.history.clone(),
            history_idx: snap.history_idx,
        }
    }
}
