use gpui::prelude::FluentBuilder as _;
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
        selected_count: usize,
        selected_size: u64,
        load_time: String,
        path: String,
    },
    /// Search results summary.
    SearchResults {
        total: usize,
        query: String,
        total_size: u64,
    },
    /// Cleanup selection summary.
    Cleanup {
        total_items: usize,
        total_size: u64,
        selected_count: usize,
        selected_size: u64,
        /// Label for the item unit (e.g. "items" or "groups").
        label: &'static str,
        /// Result of last clean: (ok, failed). Shown when present.
        last_result: Option<(usize, usize)>,
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
                                .child(SharedString::from(format!("results for \"{query}\""))),
                        ),
                )
                .child(
                    div()
                        .text_size(FONT_SIZE_CAPTION)
                        .text_color(muted)
                        .child(SharedString::from(format_size(total_size).clone())),
                ),

            StatusBarMode::Cleanup {
                total_items,
                total_size,
                selected_count,
                selected_size,
                label,
                last_result,
            } => {
                let left = if selected_count > 0 {
                    h_flex()
                        .gap(px(4.0))
                        .items_center()
                        .text_size(FONT_SIZE_CAPTION)
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child(SharedString::from(format!(
                                    "{selected_count} of {total_items}"
                                ))),
                        )
                        .child(div().text_color(muted).child("selected,"))
                        .child(
                            div()
                                .text_color(muted)
                                .child(SharedString::from(format_size(selected_size).clone())),
                        )
                } else {
                    h_flex()
                        .gap(px(4.0))
                        .items_center()
                        .text_size(FONT_SIZE_CAPTION)
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child(SharedString::from(total_items.to_string())),
                        )
                        .child(div().text_color(muted).child(SharedString::from(format!(
                            "{label}, {}",
                            format_size(total_size),
                        ))))
                };

                let right = last_result.map(|(ok, failed)| {
                    let warn = gpui::red();
                    if failed > 0 {
                        h_flex()
                            .gap(px(4.0))
                            .items_center()
                            .text_size(FONT_SIZE_CAPTION)
                            .child(
                                div()
                                    .text_color(muted)
                                    .child(SharedString::from(format!("Cleaned {ok},"))),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(warn)
                                    .child(SharedString::from(format!("{failed} failed"))),
                            )
                    } else {
                        h_flex()
                            .gap(px(4.0))
                            .items_center()
                            .text_size(FONT_SIZE_CAPTION)
                            .child(
                                div()
                                    .text_color(muted)
                                    .child(SharedString::from(format!("Cleaned {ok} items"))),
                            )
                    }
                });

                h_flex()
                    .w_full()
                    .px_3()
                    .py_1()
                    .bg(theme::status_bar_bg(cx))
                    .items_center()
                    .justify_between()
                    .child(left)
                    .when_some(right, gpui::ParentElement::child)
            }

            StatusBarMode::Directory {
                file_count,
                folder_count,
                total_size,
                selected_count,
                selected_size,
                load_time,
                path,
            } => {
                let total_items = file_count + folder_count;

                let left = if selected_count > 1 {
                    // "3 of 11 selected, 1.2 MB"
                    h_flex()
                        .gap(px(4.0))
                        .items_center()
                        .text_size(FONT_SIZE_CAPTION)
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child(SharedString::from(format!(
                                    "{selected_count} of {total_items}"
                                ))),
                        )
                        .child(div().text_color(muted).child("selected,"))
                        .child(
                            div()
                                .text_color(muted)
                                .child(SharedString::from(format_size(selected_size).clone())),
                        )
                } else {
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
                        )
                };

                h_flex()
                    .w_full()
                    .px_3()
                    .py_1()
                    .bg(theme::status_bar_bg(cx))
                    .items_center()
                    .justify_between()
                    .child(left)
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .child(SharedString::from(path)),
                    )
            }
        }
    }
}
