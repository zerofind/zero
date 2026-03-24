use std::path::PathBuf;

use gpui::*;

use crate::models::ActiveView;

use super::ZeroApp;

impl ZeroApp {
    // -- Navigation core -------------------------------------------------

    /// Push a new view onto the navigation stack. All navigation flows through here.
    pub fn push_view(&mut self, view: ActiveView, window: &mut Window, cx: &mut Context<Self>) {
        // Don't push duplicate consecutive entries
        // SAFETY: nav_idx is always valid — initialized to 0 and only modified by push/back/forward
        #[allow(clippy::indexing_slicing)]
        if self.nav_stack[self.nav_idx] == view {
            return;
        }
        // Trim forward history
        if self.nav_idx + 1 < self.nav_stack.len() {
            self.nav_stack.truncate(self.nav_idx + 1);
        }
        self.nav_stack.push(view);
        self.nav_idx = self.nav_stack.len() - 1;
        self.apply_current_view(window, cx);
    }

    /// Sync UI state from the current stack entry.
    pub(crate) fn apply_current_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // nav_idx is always in bounds — maintained by push_view, go_back, go_forward
        #[allow(clippy::indexing_slicing)]
        let view = self.nav_stack[self.nav_idx].clone();
        self.active_view = view.clone();

        match &view {
            ActiveView::FileBrowser(path) => {
                self.current_path.clone_from(path);
                // Persist last_path to active workspace
                let mut settings = crate::session::Settings::load();
                settings.active_ws_mut().last_path = Some(path.clone());
                settings.save();

                if let Some(browser) = &self.file_browser {
                    let p = path.clone();
                    browser.update(cx, |v, cx| v.navigate(p, cx));
                }
                if let Some(grid) = &self.file_grid {
                    let p = path.clone();
                    grid.update(cx, |v, cx| v.navigate(&p, cx));
                }
            }
            ActiveView::Cleanup => {
                self.ensure_cleanup(window, cx);
                if let Some(cleanup) = &self.cleanup {
                    cleanup.update(cx, |v, cx| v.set_detail_group(None, cx));
                }
            }
            ActiveView::CleanupDetail(idx) => {
                let idx = *idx;
                self.ensure_cleanup(window, cx);
                if let Some(cleanup) = &self.cleanup {
                    cleanup.update(cx, |v, cx| v.set_detail_group(Some(idx), cx));
                }
            }
            ActiveView::Editor(path) => {
                if self
                    .editor
                    .as_ref()
                    .is_some_and(|e| e.read(cx).path() != path)
                {
                    self.editor = None;
                }
            }
            ActiveView::DataTable(path) => {
                if self
                    .data_table
                    .as_ref()
                    .is_some_and(|dt| dt.read(cx).path() != path)
                {
                    self.data_table = None;
                }
            }
            _ => {}
        }

        self.focus_content(window, cx);
        cx.notify();
    }

    pub fn focus_content(&self, window: &mut Window, cx: &App) {
        match &self.active_view {
            ActiveView::FileBrowser(_) => match self.view_mode {
                crate::models::ViewMode::List => {
                    if let Some(view) = &self.file_browser {
                        let table_handle = view.read(cx).table_state.focus_handle(cx);
                        table_handle.focus(window);
                        return;
                    }
                }
                crate::models::ViewMode::Grid => {
                    if let Some(view) = &self.file_grid {
                        view.read(cx).focus_handle.focus(window);
                        return;
                    }
                }
            },
            ActiveView::Editor(_) => {
                if let Some(view) = &self.editor {
                    view.read(cx).focus_handle.focus(window);
                    return;
                }
            }
            ActiveView::DataTable(_) => {
                if let Some(view) = &self.data_table {
                    view.read(cx).focus_handle.focus(window);
                    return;
                }
            }
            _ => {}
        }
        self.focus_handle.focus(window);
    }

    pub fn navigate_to(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!(path = %path.display(), "navigate_to");
        self.push_view(ActiveView::FileBrowser(path), window, cx);
    }

    pub fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.nav_idx == 0 {
            return;
        }
        self.nav_idx -= 1;
        tracing::debug!(idx = self.nav_idx, "go_back");
        self.apply_current_view(window, cx);
    }

    pub fn go_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.nav_idx + 1 >= self.nav_stack.len() {
            return;
        }
        self.nav_idx += 1;
        tracing::debug!(idx = self.nav_idx, "go_forward");
        self.apply_current_view(window, cx);
    }

    pub fn can_go_back(&self) -> bool {
        self.nav_idx > 0
    }
    pub fn can_go_forward(&self) -> bool {
        self.nav_idx + 1 < self.nav_stack.len()
    }
}
