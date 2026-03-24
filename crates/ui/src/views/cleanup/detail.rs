use std::path::Path;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    Disableable as _, IconName, Sizable as _, button::Button, checkbox::Checkbox, h_flex,
    table::Table, v_flex,
};

use crate::theme;
use crate::ui::{EmptyState, StatusBar, StatusBarMode, format_size};

use super::view::CleanupView;

/// Shorten a path by replacing the home directory with ~
pub(super) fn shorten_path(path: &Path) -> String {
    if let Some(rest) = dirs::home_dir().and_then(|h| path.strip_prefix(h).ok()) {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

/// Render the detail view for a drilled-in group.
pub(super) fn render_detail_view(
    view: &mut CleanupView,
    _window: &mut Window,
    cx: &mut Context<CleanupView>,
) -> impl IntoElement {
    let group_idx = view.detail_group.unwrap_or(0);
    let group = view.groups.get(group_idx);
    let total_bytes = group.map_or(0, |g| g.total_bytes);
    let detail_loaded = group.is_some_and(|g| g.detail_loaded);
    let item_count = view.detail_item_count(cx);
    let selected_count = view.detail_selected_count(cx);
    let all_selected = item_count > 0 && selected_count == item_count;
    let has_selection = selected_count > 0;
    let selected_bytes = view.detail_selected_bytes(cx);

    // Show loading state if detail not yet loaded, otherwise the virtualized table
    let content: AnyElement = if !detail_loaded {
        EmptyState::new(IconName::Search, "Loading...")
            .subtitle("Fetching file details")
            .into_any_element()
    } else if let Some(ref table_state) = view.detail_table {
        div()
            .flex_1()
            .size_full()
            .overflow_hidden()
            .child(Table::new(table_state).bordered(false).stripe(true))
            .into_any_element()
    } else {
        div().into_any_element()
    };

    div().relative().size_full().child(
        v_flex()
            .id("detail-view")
            .track_focus(&view.focus_handle)
            .size_full()
            .bg(theme::content_bg(cx))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                match ev.keystroke.key.as_str() {
                    "up" => this.detail_focus_prev(cx),
                    "down" => this.detail_focus_next(cx),
                    " " => this.detail_toggle_focused(cx),
                    "enter" => this.detail_open_focused(cx),
                    _ => {}
                }
            }))
            // Toolbar: select all + clean button
            .when(detail_loaded, |el| {
                el.child(
                    h_flex()
                        .items_center()
                        .justify_end()
                        .px_4()
                        .py_1()
                        .gap_2()
                        .child(
                            Checkbox::new("select-all")
                                .checked(all_selected)
                                .label("All")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_detail_select_all(cx);
                                })),
                        )
                        .when(has_selection && !view.cleaning, |el| {
                            el.child(
                                Button::new("clean-detail-btn")
                                    .label(SharedString::from(format!(
                                        "Clean {}",
                                        format_size(selected_bytes),
                                    )))
                                    .compact()
                                    .small()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.clean_detail_selected(cx);
                                    })),
                            )
                        })
                        .when(view.cleaning, |el| {
                            el.child(
                                Button::new("clean-detail-btn")
                                    .label("Cleaning...")
                                    .compact()
                                    .small()
                                    .disabled(true),
                            )
                        }),
                )
            })
            // Item list or loading state
            .child(content)
            // Footer status bar
            .when(detail_loaded && item_count > 0, |el| {
                el.child(StatusBar::new(StatusBarMode::Cleanup {
                    total_items: item_count,
                    total_size: total_bytes,
                    selected_count,
                    selected_size: selected_bytes,
                    label: "items",
                    last_result: view.last_clean_result,
                }))
            }),
    )
}
