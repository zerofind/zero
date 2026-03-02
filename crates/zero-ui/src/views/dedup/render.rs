use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex, v_flex,
};

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS};
use crate::ui::{ConfirmDialog, EmptyState, StatusPill, format_size};

use super::{DedupFilter, DedupSort, DedupView};

impl Render for DedupView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        let confirm_dialog = if self.confirm_delete {
            let file_count = self.selected_for_deletion.len();
            let bytes = self.selected_bytes();
            let confirm_entity = cx.entity().clone();
            let cancel_entity = cx.entity().clone();

            Some(
                ConfirmDialog::new(
                    "Delete Duplicate Files",
                    format!(
                        "Move {} duplicate files ({}) to Trash?",
                        file_count,
                        format_size(bytes),
                    ),
                    move |_window, cx| {
                        confirm_entity.update(cx, |this, cx| {
                            this.confirm_delete = false;
                            this.delete_selected(cx);
                        });
                    },
                    move |_window, cx| {
                        cancel_entity.update(cx, |this, cx| {
                            this.confirm_delete = false;
                            cx.notify();
                        });
                    },
                )
                .confirm_label("Delete")
                .destructive()
                .render_element(window, cx),
            )
        } else {
            None
        };

        let active_filter = self.active_filter;
        let active_sort = self.sort_order;

        div()
            .relative()
            .size_full()
            .child(v_flex()
            .size_full()
            .bg(theme::content_bg(cx))
            .p_4()
            .gap_3()
            // Header
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(FONT_SIZE_BODY)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Duplicate Files"),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .when(self.deleting, |el| {
                                el.child(
                                    Button::new("delete-btn")
                                        .label("Deleting...")
                                        .compact()
                                        .small()
                                        .disabled(true),
                                )
                            })
                            .when(
                                !self.selected_for_deletion.is_empty() && !self.deleting,
                                |el| {
                                    el.child(
                                        Button::new("delete-btn")
                                            .label(SharedString::from(format!(
                                                "Delete {} ({} files)",
                                                format_size(self.selected_bytes()),
                                                self.selected_for_deletion.len(),
                                            )))
                                            .compact()
                                            .small()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.confirm_delete = true;
                                                cx.notify();
                                            })),
                                    )
                                },
                            )
                            .child(
                                Button::new("dedup-scan")
                                    .label(if self.scanning {
                                        "Scanning..."
                                    } else {
                                        "Find Duplicates"
                                    })
                                    .compact()
                                    .small()
                                    .disabled(self.scanning || self.deleting)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.start_scan(cx);
                                    })),
                            ),
                    ),
            )
            // Filter pills + sort buttons
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_1()
                            .children(DedupFilter::ALL_FILTERS.iter().map(|&filter| {
                                let is_active = filter == active_filter;
                                let mut btn = Button::new(SharedString::from(format!(
                                    "filter-{}",
                                    filter.label()
                                )))
                                .label(filter.label())
                                .compact()
                                .xsmall();

                                if is_active {
                                    btn = btn.primary();
                                } else {
                                    btn = btn.ghost();
                                }

                                btn.on_click(cx.listener(move |this, _, _, cx| {
                                    this.active_filter = filter;
                                    cx.notify();
                                }))
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                div()
                                    .text_size(FONT_SIZE_CAPTION)
                                    .text_color(muted)
                                    .child("Sort:"),
                            )
                            .children(DedupSort::ALL.iter().map(|&sort| {
                                let is_active = sort == active_sort;
                                let mut btn = Button::new(SharedString::from(format!(
                                    "sort-{}",
                                    sort.label()
                                )))
                                .label(sort.label())
                                .compact()
                                .xsmall();

                                if is_active {
                                    btn = btn.primary();
                                } else {
                                    btn = btn.ghost();
                                }

                                btn.on_click(cx.listener(move |this, _, _, cx| {
                                    this.set_sort(sort, cx);
                                }))
                            })),
                    ),
            )
            // Bulk selection buttons
            .when(self.scan_complete && !self.groups.is_empty(), |el| {
                el.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .child(SharedString::from(format!(
                                    "{} duplicate groups, {} wasted",
                                    self.groups.len(),
                                    format_size(self.total_wasted()),
                                ))),
                        )
                        .child(
                            Button::new("select-all-dupes")
                                .label("Select All Duplicates")
                                .compact()
                                .xsmall()
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.select_all_duplicates(cx);
                                })),
                        )
                        .when(!self.selected_for_deletion.is_empty(), |el| {
                            el.child(
                                Button::new("clear-selection")
                                    .label("Clear")
                                    .compact()
                                    .xsmall()
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.clear_selection(cx);
                                    })),
                            )
                        }),
                )
            })
            // Content
            .when(!self.scan_complete && !self.scanning, |el| {
                el.child(
                    EmptyState::new(IconName::File, "Find duplicate files")
                        .subtitle("Scan a folder to identify identical files"),
                )
            })
            .when(self.scanning, |el| {
                let status = self
                    .scan_status
                    .clone()
                    .unwrap_or_else(|| "Scanning for duplicates...".to_string());
                el.child(
                    EmptyState::new(IconName::Search, SharedString::from(status))
                        .subtitle("Finding identical copies across your files"),
                )
            })
            .when(self.scan_complete && !self.scanning, |el| {
                el.child(
                    v_flex()
                        .id("dedup-list")
                        .flex_1()
                        .overflow_y_scroll()
                        .gap_1()
                        .children(self.groups.iter().enumerate().map(|(i, group)| {
                            let name = group
                                .files
                                .first()
                                .and_then(|p| {
                                    p.file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                })
                                .unwrap_or_else(|| "Unknown".into());

                            let copies = group.files.len();
                            let expanded = group.expanded;

                            v_flex()
                                .w_full()
                                .child(
                                    h_flex()
                                        .id(SharedString::from(format!("group-{i}")))
                                        .w_full()
                                        .px_3()
                                        .py_2()
                                        .gap_2()
                                        .items_center()
                                        .rounded(RADIUS)
                                        .cursor_pointer()
                                        .hover(|s| s.bg(theme::surface_hover(cx)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if i < this.groups.len() {
                                                this.groups[i].expanded =
                                                    !this.groups[i].expanded;
                                                cx.notify();
                                            }
                                        }))
                                        .child(
                                            Icon::new(if expanded {
                                                IconName::ChevronDown
                                            } else {
                                                IconName::ChevronRight
                                            })
                                            .with_size(px(12.0))
                                            .text_color(muted),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .text_size(FONT_SIZE_BODY)
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .child(SharedString::from(name)),
                                        )
                                        .child(StatusPill::new(
                                            SharedString::from(format!("{copies} copies")),
                                            theme::brand_color(),
                                        ))
                                        .child(
                                            div()
                                                .text_size(FONT_SIZE_CAPTION)
                                                .text_color(muted)
                                                .min_w(px(70.0))
                                                .text_right()
                                                .child(SharedString::from(format_size(
                                                    group.size,
                                                ))),
                                        ),
                                )
                                .when(expanded, |el| {
                                    el.child(
                                        v_flex().pl(px(28.0)).gap_0p5().children(
                                            group.files.iter().enumerate().map(|(fi, p)| {
                                                let is_keeper = fi == 0;
                                                let p_clone = p.clone();
                                                let checked =
                                                    self.selected_for_deletion.contains(p);
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .py_0p5()
                                                    .when(!is_keeper, |el| {
                                                        el.child(
                                                            Checkbox::new(
                                                                SharedString::from(format!(
                                                                    "dup-{i}-{fi}"
                                                                )),
                                                            )
                                                            .checked(checked)
                                                            .on_click(cx.listener(
                                                                move |this, _, _, cx| {
                                                                    this.toggle_file_selection(
                                                                        p_clone.clone(),
                                                                        cx,
                                                                    );
                                                                },
                                                            )),
                                                        )
                                                    })
                                                    .when(is_keeper, |el| {
                                                        el.child(
                                                            div()
                                                                .text_size(FONT_SIZE_CAPTION)
                                                                .text_color(
                                                                    theme::success_color(),
                                                                )
                                                                .child("keep"),
                                                        )
                                                    })
                                                    .child({
                                                        let reveal_path = p.clone();
                                                        div()
                                                            .id(SharedString::from(format!("path-{i}-{fi}")))
                                                            .flex_1()
                                                            .min_w_0()
                                                            .text_size(FONT_SIZE_CAPTION)
                                                            .text_color(muted)
                                                            .text_ellipsis()
                                                            .whitespace_nowrap()
                                                            .cursor_pointer()
                                                            .hover(|s| s.underline())
                                                            .on_click(move |_, _, _| {
                                                                #[cfg(target_os = "macos")]
                                                                crate::platform::open::reveal_in_finder(&reveal_path);
                                                            })
                                                            .child(SharedString::from(
                                                                p.to_string_lossy().to_string(),
                                                            ))
                                                    })
                                            }),
                                        ),
                                    )
                                })
                        })),
                )
            }))
            .when_some(confirm_dialog, |el, dialog| el.child(dialog))
    }
}
