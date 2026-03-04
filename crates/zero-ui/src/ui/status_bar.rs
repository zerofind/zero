use gpui::*;
use gpui_component::{ActiveTheme, h_flex};

use crate::theme::{self, FONT_SIZE_CAPTION, brand_color};
use crate::views::file_browser::state::format_size;

/// What the status bar should display.
pub enum StatusBarMode {
    /// Normal directory listing stats.
    Directory {
        file_count: usize,
        folder_count: usize,
        total_size: u64,
        load_time: String,
        path: String,
    },
    /// Search results summary.
    SearchResults {
        total: usize,
        query: String,
        total_size: u64,
    },
}

/// Stateless summary bar — receives data, renders once.
#[derive(IntoElement)]
pub struct StatusBar {
    mode: StatusBarMode,
}

impl StatusBar {
    pub fn new(mode: StatusBarMode) -> Self {
        Self { mode }
    }
}

impl RenderOnce for StatusBar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;

        match self.mode {
            StatusBarMode::SearchResults {
                total,
                query,
                total_size,
            } => h_flex()
                .w_full()
                .px_3()
                .py_1()
                .bg(theme::status_bar_bg(cx))
                .items_center()
                .justify_between()
                .child(
                    h_flex()
                        .gap(px(4.0))
                        .items_center()
                        .text_size(FONT_SIZE_CAPTION)
                        .child(div().text_color(brand_color(cx)).child("\u{1F50D}"))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child(SharedString::from(total.to_string())),
                        )
                        .child(
                            div()
                                .text_color(muted)
                                .child(SharedString::from(format!("results for \"{}\"", query))),
                        ),
                )
                .child(
                    div()
                        .text_size(FONT_SIZE_CAPTION)
                        .text_color(muted)
                        .child(SharedString::from(format_size(total_size).to_string())),
                ),

            StatusBarMode::Directory {
                file_count,
                folder_count,
                total_size,
                load_time,
                path,
            } => h_flex()
                .w_full()
                .px_3()
                .py_1()
                .bg(theme::status_bar_bg(cx))
                .items_center()
                .justify_between()
                .child(
                    h_flex()
                        .gap(px(4.0))
                        .items_center()
                        .text_size(FONT_SIZE_CAPTION)
                        .child(div().text_color(brand_color(cx)).child("\u{26A1}"))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child(SharedString::from(file_count.to_string())),
                        )
                        .child(div().text_color(muted).child(SharedString::from(format!(
                            "files ({}),",
                            format_size(total_size),
                        ))))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child(SharedString::from(folder_count.to_string())),
                        )
                        .child(div().text_color(muted).child("folders in"))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child(SharedString::from(load_time)),
                        ),
                )
                .child(
                    div()
                        .text_size(FONT_SIZE_CAPTION)
                        .text_color(muted)
                        .child(SharedString::from(path)),
                ),
        }
    }
}
