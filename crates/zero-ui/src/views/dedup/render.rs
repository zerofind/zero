use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    table::Table,
    v_flex,
};

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION};
use crate::ui::{ConfirmDialog, EmptyState, format_size};

use super::{DedupFilter, DedupView};

impl Render for DedupView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        let selected_count = self.selected_count(cx);
        let selected_bytes = self.selected_bytes(cx);
        let has_selection = selected_count > 0;
        let group_count = self.group_count(cx);
        let total_wasted = self.total_wasted(cx);

        let confirm_dialog = if self.confirm_delete {
            let confirm_entity = cx.entity().clone();
            let cancel_entity = cx.entity().clone();

            Some(
                ConfirmDialog::new(
                    "Delete Duplicate Files",
                    format!(
                        "Move {} duplicate files ({}) to Trash?",
                        selected_count,
                        format_size(selected_bytes),
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

        div()
            .relative()
            .size_full()
            .child(
                v_flex()
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
                                    .when(has_selection && !self.deleting, |el| {
                                        el.child(
                                            Button::new("delete-btn")
                                                .label(SharedString::from(format!(
                                                    "Delete {} ({} files)",
                                                    format_size(selected_bytes),
                                                    selected_count,
                                                )))
                                                .compact()
                                                .small()
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.confirm_delete = true;
                                                    cx.notify();
                                                })),
                                        )
                                    })
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
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.start_scan(window, cx);
                                            })),
                                    ),
                            ),
                    )
                    // Filter pills + bulk actions
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
                            .when(self.scan_complete && group_count > 0, |el| {
                                el.child(
                                    h_flex()
                                        .gap_2()
                                        .items_center()
                                        .child(
                                            div()
                                                .text_size(FONT_SIZE_CAPTION)
                                                .text_color(muted)
                                                .child(SharedString::from(format!(
                                                    "{} groups, {} wasted",
                                                    group_count,
                                                    format_size(total_wasted),
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
                                        .when(has_selection, |el| {
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
                            }),
                    )
                    // Content
                    .when(!self.scan_complete && !self.scanning, |el| {
                        el.child(
                            EmptyState::new(IconName::File, "Find duplicate files")
                                .subtitle("Scan a folder to identify identical files"),
                        )
                    })
                    .when(self.scanning && self.table.is_none(), |el| {
                        let status = self
                            .scan_status
                            .clone()
                            .unwrap_or_else(|| "Scanning for duplicates...".to_string());
                        el.child(
                            EmptyState::new(IconName::Search, SharedString::from(status))
                                .subtitle("Finding identical copies across your files"),
                        )
                    })
                    .when_some(self.table.as_ref().cloned(), |el, table_state| {
                        el.child(
                            div()
                                .flex_1()
                                .size_full()
                                .overflow_hidden()
                                .child(Table::new(&table_state).bordered(false).stripe(true)),
                        )
                    }),
            )
            .when_some(confirm_dialog, |el, dialog| el.child(dialog))
    }
}
