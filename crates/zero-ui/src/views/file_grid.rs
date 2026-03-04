use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, InteractiveElementExt as _};

use crate::theme::{
    self, FONT_SIZE_CAPTION, GRID_CELL_HEIGHT, GRID_CELL_WIDTH, GRID_ICON_SIZE, RADIUS,
};
use crate::ui::FileIcon;

use super::file_browser::state::BrowserEntry;

/// Events emitted by the grid view to the parent.
pub enum FileGridEvent {
    NavigateToDir(PathBuf),
}

/// Grid view of files — adaptive columns of 80x80 cells.
pub struct FileGridView {
    entries: Vec<BrowserEntry>,
    selected: Vec<usize>,
    pub focus_handle: FocusHandle,
}

impl EventEmitter<FileGridEvent> for FileGridView {}

impl FileGridView {
    pub fn new(entries: Vec<BrowserEntry>, cx: &mut Context<Self>) -> Self {
        Self {
            entries,
            selected: Vec::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Navigate to a new directory in-place (synchronous, matching ensure_file_grid pattern).
    pub fn navigate(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let entries = crate::views::file_browser::state::load_directory(path);
        self.update_entries(entries, cx);
    }

    #[allow(dead_code)]
    pub fn update_entries(&mut self, entries: Vec<BrowserEntry>, cx: &mut Context<Self>) {
        self.entries = entries;
        self.selected.clear();
        cx.notify();
    }

    fn open_entry(&mut self, idx: usize, cx: &mut Context<Self>) {
        let Some(entry) = self.entries.get(idx) else {
            return;
        };
        if entry.is_dir {
            cx.emit(FileGridEvent::NavigateToDir(entry.path.clone()));
        } else {
            #[cfg(target_os = "macos")]
            crate::platform::open::open_path(&entry.path);
        }
    }
}

impl Render for FileGridView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("file-grid")
            .track_focus(&self.focus_handle)
            .size_full()
            .p_3()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_2()
            .overflow_y_scroll()
            .bg(theme::content_bg(cx))
            .children(self.entries.iter().enumerate().map(|(i, entry)| {
                let is_selected = self.selected.contains(&i);
                let name = SharedString::from(entry.name.clone());

                div()
                    .id(SharedString::from(format!("grid-{i}")))
                    .w(GRID_CELL_WIDTH)
                    .h(GRID_CELL_HEIGHT)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .rounded(RADIUS)
                    .cursor_pointer()
                    .when(is_selected, |el| el.bg(theme::surface_active(cx)))
                    .hover(|s| s.bg(theme::surface_hover(cx)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = vec![i];
                        cx.notify();
                    }))
                    .on_double_click(cx.listener(move |this, _, _, cx| {
                        this.open_entry(i, cx);
                    }))
                    .child(
                        div()
                            .w(GRID_ICON_SIZE)
                            .h(GRID_ICON_SIZE)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(FileIcon::new(entry.extension.as_deref(), entry.is_dir)),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(cx.theme().foreground)
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_center()
                            .child(name),
                    )
            }))
    }
}
