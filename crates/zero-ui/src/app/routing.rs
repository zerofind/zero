use std::path::Path;

use gpui::*;

use crate::models::ActiveView;

use super::ZeroApp;

impl ZeroApp {
    /// Unified file-opening router: directory → navigate, data table → DataTable view,
    /// editable → Editor view, otherwise → platform open.
    pub fn open_path(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if path.is_dir() {
            self.navigate_to(path.to_path_buf(), window, cx);
        } else if crate::views::data_table::is_data_table(path) {
            self.data_table = None;
            self.push_view(ActiveView::DataTable(path.to_path_buf()), window, cx);
        } else if crate::views::editor::is_editable(path) {
            self.editor = None;
            self.push_view(ActiveView::Editor(path.to_path_buf()), window, cx);
        } else {
            #[cfg(target_os = "macos")]
            crate::platform::open::open_path(path);
        }
    }

    /// Launch dedup view with a pre-set scan path.
    pub fn launch_dedup(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.dedup = None;
        let dedup = self.ensure_dedup(window, cx);
        dedup.update(cx, |view, cx| {
            view.set_scan_path(path.to_path_buf(), cx);
        });
        self.push_view(ActiveView::Dedup, window, cx);
    }

    /// Map an `action://` URL to the corresponding ActiveView.
    pub(crate) fn action_view(action: &str) -> Option<ActiveView> {
        match action {
            "action://settings" => Some(ActiveView::Settings),
            "action://cleanup" => Some(ActiveView::Cleanup),
            "action://dedup" => Some(ActiveView::Dedup),
            "action://todo" => Some(ActiveView::Todo),
            "action://secure_erase" => Some(ActiveView::SecureErase),
            "action://automations" => Some(ActiveView::Automations),
            _ => None,
        }
    }

    pub fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!("action: open command palette");
        self.command_palette_open = true;
        let palette = self.ensure_command_palette(window, cx);
        palette.update(cx, |view, cx| view.reset(window, cx));
        cx.notify();
    }

    pub fn paste_files(&mut self, cx: &mut Context<Self>) {
        let Some(clipboard) = self.file_clipboard.clone() else {
            return;
        };
        if let Some(browser) = &self.file_browser {
            browser.update(cx, |view, cx| {
                view.paste_files(&clipboard, cx);
            });
        }
        if clipboard.operation == crate::models::ClipboardOperation::Cut {
            self.file_clipboard = None;
        }
    }
}
