use std::path::PathBuf;

use gpui::*;

use crate::models::{ActiveView, SplitPane, ViewMode};
use crate::workspace::{SplitPaneSnapshot, WorkspaceSnapshot};

use super::ZeroApp;

impl ZeroApp {
    /// Snapshot the current workspace state into `workspace_snapshots`.
    fn snapshot_current_workspace(&mut self) {
        let snap = WorkspaceSnapshot {
            nav_stack: self.nav_stack.clone(),
            nav_idx: self.nav_idx,
            current_path: self.current_path.clone(),
            active_view: self.active_view.clone(),
            view_mode: self.view_mode,
            split_pane: self.split_pane.as_ref().map(SplitPaneSnapshot::from),
        };
        self.workspace_snapshots[self.active_workspace_idx] = Some(snap);
    }

    /// Switch to the workspace at `idx`.
    pub fn switch_workspace(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        let mut settings = crate::session::Settings::load();
        if idx >= settings.workspaces.len() || idx == self.active_workspace_idx {
            return;
        }

        // 1. Snapshot current workspace
        self.snapshot_current_workspace();

        // 2. Persist current workspace's last_path
        settings.workspaces[self.active_workspace_idx].last_path = Some(self.current_path.clone());
        settings.workspaces[self.active_workspace_idx].view_mode = match self.view_mode {
            ViewMode::List => "list".to_string(),
            ViewMode::Grid => "grid".to_string(),
        };
        settings.active_workspace = idx;
        settings.save();

        // 3. Update active index
        self.active_workspace_idx = idx;

        // 4. Restore target workspace
        if let Some(snap) = self.workspace_snapshots[idx].take() {
            self.nav_stack = snap.nav_stack;
            self.nav_idx = snap.nav_idx;
            self.current_path = snap.current_path;
            self.active_view = snap.active_view;
            self.view_mode = snap.view_mode;
            self.split_pane = snap.split_pane.as_ref().map(SplitPane::from);
        } else {
            // Fresh start from last_path or home
            let ws = &settings.workspaces[idx];
            let start = ws
                .last_path
                .clone()
                .or_else(|| ws.pinned_bookmarks.first().cloned())
                .or_else(dirs::home_dir)
                .unwrap_or_else(|| PathBuf::from("/"));
            self.view_mode = if ws.view_mode == "grid" {
                ViewMode::Grid
            } else {
                ViewMode::List
            };
            self.current_path = start.clone();
            self.active_view = ActiveView::FileBrowser(start.clone());
            self.nav_stack = vec![ActiveView::FileBrowser(start)];
            self.nav_idx = 0;
            self.split_pane = None;
        }

        // 5. Invalidate cached views so they rebuild
        self.file_browser = None;
        self.file_grid = None;
        self.split_browser = None;

        // 6. Update sidebar bookmarks
        let ws = &settings.workspaces[idx];
        let names: Vec<String> = settings.workspaces.iter().map(|w| w.name.clone()).collect();
        if let Some(sidebar) = &self.sidebar {
            sidebar.update(cx, |s, cx| {
                s.set_workspace(
                    names,
                    idx,
                    ws.pinned_bookmarks.clone(),
                    ws.regular_bookmarks.clone(),
                    cx,
                );
            });
        }

        // 7. Refresh git state for the new bookmarks
        self.refresh_sidebar_git(cx);

        // 8. Apply the current view
        self.apply_current_view(window, cx);
    }

    /// Create a new workspace and switch to it.
    pub fn create_workspace(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let mut settings = crate::session::Settings::load();
        settings
            .workspaces
            .push(crate::workspace::Workspace::new_empty(&name));
        settings.save();

        // Grow snapshot storage
        self.workspace_snapshots.push(None);

        let new_idx = settings.workspaces.len() - 1;
        self.switch_workspace(new_idx, window, cx);
    }

    /// Rename a workspace.
    pub fn rename_workspace(&mut self, idx: usize, name: String, cx: &mut Context<Self>) {
        let mut settings = crate::session::Settings::load();
        if idx < settings.workspaces.len() {
            settings.workspaces[idx].name = name;
            settings.save();

            let names: Vec<String> = settings.workspaces.iter().map(|w| w.name.clone()).collect();
            if let Some(sidebar) = &self.sidebar {
                sidebar.update(cx, |s, cx| {
                    s.set_workspace_names(names, cx);
                });
            }
        }
    }

    /// Delete a workspace (disabled if only 1 remains).
    pub fn delete_workspace(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        let mut settings = crate::session::Settings::load();
        if settings.workspaces.len() <= 1 || idx >= settings.workspaces.len() {
            return;
        }

        settings.workspaces.remove(idx);
        self.workspace_snapshots.remove(idx);

        // Adjust active index
        let new_active = if self.active_workspace_idx >= settings.workspaces.len() {
            settings.workspaces.len() - 1
        } else if self.active_workspace_idx > idx {
            self.active_workspace_idx - 1
        } else {
            self.active_workspace_idx
        };
        settings.active_workspace = new_active;
        settings.save();

        // If we deleted the active workspace, switch to the new active
        if idx == self.active_workspace_idx {
            self.active_workspace_idx = new_active;
            // Force re-init from the new workspace
            let ws = &settings.workspaces[new_active];
            let start = ws
                .last_path
                .clone()
                .or_else(|| ws.pinned_bookmarks.first().cloned())
                .or_else(dirs::home_dir)
                .unwrap_or_else(|| PathBuf::from("/"));
            self.view_mode = if ws.view_mode == "grid" {
                ViewMode::Grid
            } else {
                ViewMode::List
            };
            self.current_path = start.clone();
            self.active_view = ActiveView::FileBrowser(start.clone());
            self.nav_stack = vec![ActiveView::FileBrowser(start)];
            self.nav_idx = 0;
            self.split_pane = None;
            self.file_browser = None;
            self.file_grid = None;
            self.split_browser = None;
            self.apply_current_view(window, cx);
        } else {
            self.active_workspace_idx = new_active;
        }

        // Update sidebar
        let names: Vec<String> = settings.workspaces.iter().map(|w| w.name.clone()).collect();
        let ws = &settings.workspaces[self.active_workspace_idx];
        if let Some(sidebar) = &self.sidebar {
            sidebar.update(cx, |s, cx| {
                s.set_workspace(
                    names,
                    self.active_workspace_idx,
                    ws.pinned_bookmarks.clone(),
                    ws.regular_bookmarks.clone(),
                    cx,
                );
            });
        }
        self.refresh_sidebar_git(cx);
    }
}
