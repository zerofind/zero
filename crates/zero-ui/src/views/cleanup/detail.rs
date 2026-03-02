use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex, v_flex,
};

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS};
use crate::ui::{ConfirmDialog, format_size};

use super::view::CleanupView;

/// Render the per-file detail view for a drilled-in category.
pub(super) fn render_detail_view(
    view: &mut CleanupView,
    window: &mut Window,
    cx: &mut Context<CleanupView>,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;

    let cat_idx = view.detail_category.unwrap_or(0);
    let cat = view.categories.get(cat_idx);
    let cat_name = cat.map(|c| c.name.clone()).unwrap_or_default();
    let file_count = cat.map(|c| c.files.len()).unwrap_or(0);
    let total_bytes: u64 = cat.map(|c| c.total_bytes).unwrap_or(0);
    let all_selected = file_count > 0 && view.detail_selected.len() == file_count;
    let has_selection = !view.detail_selected.is_empty();
    let selected_bytes = view.detail_selected_bytes();

    let confirm_dialog = if view.confirm_detail_clean {
        let count = view.detail_selected.len();
        let confirm_entity = cx.entity().clone();
        let cancel_entity = cx.entity().clone();

        Some(
            ConfirmDialog::new(
                "Clean Selected Files",
                format!(
                    "Move {} files ({}) to Trash? This cannot be undone.",
                    count,
                    format_size(selected_bytes),
                ),
                move |_window, cx| {
                    confirm_entity.update(cx, |this, cx| {
                        this.confirm_detail_clean = false;
                        this.clean_detail_selected(cx);
                    });
                },
                move |_window, cx| {
                    cancel_entity.update(cx, |this, cx| {
                        this.confirm_detail_clean = false;
                        cx.notify();
                    });
                },
            )
            .confirm_label("Clean")
            .destructive()
            .render_element(window, cx),
        )
    } else {
        None
    };

    let focused_idx = view.detail_focused;

    let file_rows: Vec<_> = cat
        .map(|c| {
            c.files
                .iter()
                .enumerate()
                .map(|(i, file)| {
                    let checked = view.detail_selected.contains(&i);
                    let is_focused = focused_idx == Some(i);
                    let parent_str = file
                        .path
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                    h_flex()
                        .id(SharedString::from(format!("detail-file-{i}")))
                        .group("detail-row")
                        .w_full()
                        .px_3()
                        .py_1p5()
                        .gap_3()
                        .items_center()
                        .rounded(RADIUS)
                        .cursor_pointer()
                        .when(is_focused, |el| el.bg(theme::surface_hover(cx)))
                        .when(!is_focused, |el| {
                            el.hover(|s| s.bg(theme::surface_hover(cx)))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.detail_focused = Some(i);
                            this.toggle_detail_file(i, cx);
                        }))
                        .child(
                            Checkbox::new(SharedString::from(format!("detail-chk-{i}")))
                                .checked(checked),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .text_size(FONT_SIZE_BODY)
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(SharedString::from(file.name.clone())),
                                )
                                .child(
                                    div()
                                        .text_size(FONT_SIZE_CAPTION)
                                        .text_color(muted)
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(SharedString::from(parent_str)),
                                ),
                        )
                        .child(
                            div()
                                .invisible()
                                .group_hover("detail-row", |s| s.visible())
                                .when(is_focused, |el| el.visible())
                                .child(
                                    Button::new(SharedString::from(format!("reveal-{i}")))
                                        .icon(IconName::ExternalLink)
                                        .ghost()
                                        .compact()
                                        .xsmall()
                                        .tooltip("Reveal in Finder")
                                        .on_click(cx.listener(move |this, _, _, _cx| {
                                            this.detail_reveal_item(i);
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .text_size(FONT_SIZE_BODY)
                                .min_w(px(80.0))
                                .text_right()
                                .child(SharedString::from(format_size(file.size))),
                        )
                })
                .collect()
        })
        .unwrap_or_default();

    div()
        .relative()
        .size_full()
        .child(
            v_flex()
                .id("detail-view")
                .track_focus(&view.focus_handle)
                .size_full()
                .bg(theme::content_bg(cx))
                .p_4()
                .gap_3()
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                    match ev.keystroke.key.as_str() {
                        "up" => this.detail_focus_prev(cx),
                        "down" => this.detail_focus_next(cx),
                        " " => this.detail_toggle_focused(cx),
                        "enter" => this.detail_open_focused(cx),
                        _ => {}
                    }
                }))
                // Header with back button
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    Button::new("back-btn")
                                        .ghost()
                                        .compact()
                                        .small()
                                        .icon(IconName::ArrowLeft)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.back_from_detail(cx);
                                        })),
                                )
                                .child(
                                    div()
                                        .text_size(FONT_SIZE_BODY)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(SharedString::from(cat_name)),
                                )
                                .child(div().text_size(FONT_SIZE_CAPTION).text_color(muted).child(
                                    SharedString::from(format!(
                                        "{} files  {}",
                                        file_count,
                                        format_size(total_bytes),
                                    )),
                                )),
                        )
                        .child(
                            h_flex()
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
                                                this.confirm_detail_clean = true;
                                                cx.notify();
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
                        ),
                )
                // File list
                .child(
                    v_flex()
                        .id("detail-file-list")
                        .flex_1()
                        .overflow_y_scroll()
                        .gap_0p5()
                        .children(file_rows),
                ),
        )
        .when_some(confirm_dialog, |el, dialog| el.child(dialog))
}
