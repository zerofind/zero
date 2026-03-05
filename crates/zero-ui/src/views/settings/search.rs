use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::Input,
    switch::Switch,
    v_flex,
};

use crate::theme::{FONT_SIZE_BODY, FONT_SIZE_CAPTION, ICON_XS, RADIUS_LG};
use crate::ui::format::format_number;

use super::SettingsView;
use super::helpers::{card_border, group_label, setting_row};

impl SettingsView {
    pub(super) fn render_search(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let border = card_border(cx);
        let muted = cx.theme().muted_foreground;
        let fg = cx.theme().foreground;

        // Always read live count from the search service
        let total_files = self.search.read(cx).file_count();

        v_flex()
            .gap_6()
            // Search options
            .child(
                v_flex().gap_3().child(group_label("Options", fg)).child(
                    v_flex()
                        .rounded(RADIUS_LG)
                        .border_1()
                        .border_color(border)
                        .overflow_hidden()
                        .child(setting_row(
                            "Include Trash in search",
                            "Show files in the Trash when searching.",
                            Switch::new("search-include-trash")
                                .checked(self.settings.search_include_trash)
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.settings.search_include_trash = *checked;
                                    this.settings.save();
                                    cx.notify();
                                }))
                                .into_any_element(),
                            muted,
                            fg,
                            border,
                            true,
                        ))
                        .child(setting_row(
                            "Include offline storages",
                            "Include files from disconnected drives in search results.",
                            Switch::new("search-include-offline")
                                .checked(self.settings.search_include_offline)
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.settings.search_include_offline = *checked;
                                    this.settings.save();
                                    cx.notify();
                                }))
                                .into_any_element(),
                            muted,
                            fg,
                            border,
                            false,
                        )),
                ),
            )
            // Search roots
            .child(
                v_flex()
                    .gap_3()
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(group_label("Indexed Locations", fg))
                            .child(
                                h_flex()
                                    .gap_3()
                                    .items_center()
                                    .child(
                                        div().text_size(FONT_SIZE_CAPTION).text_color(muted).child(
                                            SharedString::from(format!(
                                                "{} files",
                                                format_number(total_files),
                                            )),
                                        ),
                                    )
                                    .child(
                                        Button::new("reindex-all")
                                            .label(if self.reindexing_all {
                                                "Reindexing..."
                                            } else {
                                                "Re-index All"
                                            })
                                            .compact()
                                            .small()
                                            .ghost()
                                            .disabled(
                                                self.reindexing_all
                                                    || self.settings.search_roots.is_empty(),
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.reindex_all(cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("add-root")
                                            .label("Add Folder")
                                            .compact()
                                            .small()
                                            .icon(IconName::Plus)
                                            .ghost()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.adding_root = true;
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    )
                    // Add folder input
                    .when(self.adding_root, |el| {
                        el.child(
                            v_flex()
                                .gap_1()
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .items_center()
                                        .on_key_down(cx.listener(
                                            |this, ev: &KeyDownEvent, window, cx| {
                                                if ev.keystroke.key == "enter" {
                                                    this.confirm_root_input(window, cx);
                                                } else if ev.keystroke.key == "escape" {
                                                    this.adding_root = false;
                                                    this.root_error = None;
                                                    cx.notify();
                                                }
                                            },
                                        ))
                                        .child(div().flex_1().child(Input::new(&self.root_input)))
                                        .child(
                                            Button::new("confirm-root")
                                                .label("Add")
                                                .compact()
                                                .primary()
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.confirm_root_input(window, cx);
                                                })),
                                        )
                                        .child(
                                            Button::new("cancel-root")
                                                .label("Cancel")
                                                .compact()
                                                .ghost()
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.adding_root = false;
                                                    this.root_error = None;
                                                    cx.notify();
                                                })),
                                        ),
                                )
                                .when_some(self.root_error.clone(), |el, err| {
                                    el.child(
                                        div()
                                            .text_size(FONT_SIZE_CAPTION)
                                            .text_color(cx.theme().danger)
                                            .child(SharedString::from(err)),
                                    )
                                }),
                        )
                    })
                    // Root list or empty state
                    .child(
                        v_flex()
                            .rounded(RADIUS_LG)
                            .border_1()
                            .border_color(border)
                            .overflow_hidden()
                            .when(self.settings.search_roots.is_empty(), |el| {
                                el.child(
                                    div()
                                        .px_5()
                                        .py_4()
                                        .text_size(FONT_SIZE_CAPTION)
                                        .text_color(muted)
                                        .child(
                                            "No search roots configured. \
                                             Add folders to enable instant search.",
                                        ),
                                )
                            })
                            .children(self.settings.search_roots.iter().enumerate().map(
                                |(i, root)| {
                                    let is_last = i == self.settings.search_roots.len() - 1;
                                    let is_reindexing = self.reindexing_root == Some(i);

                                    // Overlap detection
                                    let overlap =
                                        self.settings.search_roots.iter().enumerate().find_map(
                                            |(j, other)| {
                                                if j == i {
                                                    return None;
                                                }
                                                let root_str = root.to_string_lossy();
                                                let other_str = other.to_string_lossy();
                                                let root_with_sep =
                                                    format!("{}/", root_str.trim_end_matches('/'));
                                                let other_with_sep =
                                                    format!("{}/", other_str.trim_end_matches('/'));
                                                if root_with_sep.starts_with(&*other_with_sep) {
                                                    Some(other.to_string_lossy().to_string())
                                                } else {
                                                    None
                                                }
                                            },
                                        );

                                    v_flex()
                                        .px_5()
                                        .py_3()
                                        .gap_1()
                                        .when(!is_last, |el| el.border_b_1().border_color(border))
                                        .child(
                                            h_flex()
                                                .gap_3()
                                                .items_center()
                                                .child(
                                                    Icon::new(IconName::Folder)
                                                        .with_size(ICON_XS)
                                                        .text_color(muted),
                                                )
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .text_size(FONT_SIZE_BODY)
                                                        .text_ellipsis()
                                                        .whitespace_nowrap()
                                                        .child(SharedString::from(
                                                            root.to_string_lossy().to_string(),
                                                        )),
                                                )
                                                .child(
                                                    Button::new(SharedString::from(format!(
                                                        "reindex-root-{i}"
                                                    )))
                                                    .label(if is_reindexing {
                                                        "Reindexing..."
                                                    } else {
                                                        "Re-index"
                                                    })
                                                    .ghost()
                                                    .compact()
                                                    .xsmall()
                                                    .disabled(is_reindexing)
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.reindex_root(i, cx);
                                                    })),
                                                )
                                                .child(
                                                    Button::new(SharedString::from(format!(
                                                        "rm-root-{i}"
                                                    )))
                                                    .icon(IconName::Close)
                                                    .ghost()
                                                    .compact()
                                                    .xsmall()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.remove_root(i, cx);
                                                    })),
                                                ),
                                        )
                                        .when_some(overlap, |el, parent| {
                                            el.child(
                                                div()
                                                    .text_size(FONT_SIZE_CAPTION)
                                                    .text_color(hsla(0.08, 0.9, 0.55, 1.0))
                                                    .child(SharedString::from(format!(
                                                        "Overlaps with {}",
                                                        parent,
                                                    ))),
                                            )
                                        })
                                },
                            )),
                    ),
            )
    }

    pub(super) fn confirm_root_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!("settings: confirm root input");
        let raw = self.root_input.read(cx).value().to_string();
        let raw = raw.trim();
        if raw.is_empty() {
            self.adding_root = false;
            cx.notify();
            return;
        }

        let expanded = if let Some(suffix) = raw.strip_prefix('~') {
            if let Some(home) = dirs::home_dir() {
                let suffix = suffix.strip_prefix('/').unwrap_or(suffix);
                home.join(suffix)
            } else {
                PathBuf::from(raw)
            }
        } else {
            PathBuf::from(raw)
        };

        if expanded.is_dir() {
            self.add_root(expanded, cx);
            self.root_error = None;
            self.adding_root = false;
            self.root_input.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
        } else {
            self.root_error = Some(format!("Path does not exist: {}", expanded.display()));
        }
        cx.notify();
    }

    pub(super) fn add_root(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        tracing::debug!(path = %path.display(), "settings: add root");
        if self.settings.search_roots.contains(&path) {
            return;
        }

        let path_str = path.to_string_lossy().to_string();

        self.settings.search_roots.push(path.clone());
        self.settings.save();

        self.search
            .update(cx, |svc, cx| svc.add_root(&path_str, cx));
        cx.notify();
    }

    pub(super) fn remove_root(&mut self, idx: usize, cx: &mut Context<Self>) {
        tracing::debug!(idx, "settings: remove root");
        if idx >= self.settings.search_roots.len() {
            return;
        }

        let path = self.settings.search_roots.remove(idx);
        self.settings.save();

        let path_str = path.to_string_lossy().to_string();
        self.search
            .update(cx, |svc, cx| svc.remove_root(&path_str, cx));
        cx.notify();
    }
}
