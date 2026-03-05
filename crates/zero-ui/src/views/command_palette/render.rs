use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable as _, h_flex, input::Input, v_flex};

use crate::theme::{
    self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, ICON_XS, MODAL_PALETTE_WIDTH, RADIUS, RADIUS_LG,
};

use super::items::{DEFAULT_ACTIONS, PaletteItem};
use super::view::{PaletteEvent, PaletteMode, PaletteView};

impl Render for PaletteView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_results = !self.results.is_empty();
        let show_results = self.is_showing_results();
        let is_drilled = matches!(self.mode, PaletteMode::DrilledIn { .. });
        let drill_label = match &self.mode {
            PaletteMode::DrilledIn { label, .. } => Some(label.clone()),
            PaletteMode::Root => None,
        };

        v_flex()
            .track_focus(&self.focus_handle)
            .key_context("PaletteView")
            .on_action(cx.listener(|this, _: &crate::actions::GoBack, _, cx| {
                this.dismiss(cx);
            }))
            .on_key_down(cx.listener(
                |this, ev: &KeyDownEvent, window, cx| match &ev.keystroke.key {
                    key if key == "escape" => {
                        if matches!(this.mode, PaletteMode::DrilledIn { .. }) {
                            this.exit_drill(window, cx);
                        } else {
                            this.dismiss(cx);
                        }
                    }
                    key if key == "enter" && ev.keystroke.modifiers.platform => {
                        if !this.query.is_empty() {
                            cx.emit(PaletteEvent::ShowInBrowser(this.query.clone()));
                        }
                    }
                    key if key == "enter" => this.confirm_selection(window, cx),
                    key if key == "up" => this.select_prev(cx),
                    key if key == "down" => this.select_next(cx),
                    key if key == "backspace" => {
                        if this.query.is_empty()
                            && matches!(this.mode, PaletteMode::DrilledIn { .. })
                        {
                            this.exit_drill(window, cx);
                        }
                    }
                    _ => {}
                },
            ))
            .w(MODAL_PALETTE_WIDTH)
            .max_h(px(460.0))
            .mx_auto()
            .mt(px(100.0))
            .rounded(RADIUS_LG)
            .bg(theme::content_bg(cx))
            .border_1()
            .border_color(cx.theme().border)
            .shadow_lg()
            .overflow_hidden()
            // Search input with icon prefix
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Input::new(&self.input)
                            .appearance(false)
                            .prefix(if is_drilled {
                                h_flex()
                                    .gap_1()
                                    .items_center()
                                    .child(
                                        Icon::new(IconName::ChevronLeft)
                                            .with_size(ICON_XS)
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(
                                        div()
                                            .text_size(FONT_SIZE_CAPTION)
                                            .text_color(cx.theme().muted_foreground)
                                            .child(drill_label.unwrap_or_default()),
                                    )
                                    .into_any_element()
                            } else {
                                Icon::new(IconName::Search)
                                    .with_size(ICON_XS)
                                    .text_color(cx.theme().muted_foreground)
                                    .into_any_element()
                            }),
                    ),
            )
            // DrilledIn mode: files + apps
            .when(show_results && is_drilled, |el| {
                self.render_drilled_results(el, has_results, cx)
            })
            // Root mode with query: multi-section filtered results
            .when(show_results && !is_drilled, |el| {
                self.render_root_results(el, cx)
            })
            // Default sections when no query and in root mode
            .when(!show_results, |el| self.render_default_sections(el, cx))
    }
}

impl PaletteView {
    fn render_drilled_results(&self, el: Div, _has_results: bool, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;

        // "Show All {Label}" action at index 0
        let type_label = match &self.mode {
            PaletteMode::DrilledIn { label, .. } => label.clone(),
            _ => String::new(),
        };
        let show_all_selected = self.selected_idx == 0;
        let show_all_label = SharedString::from(format!("Show All {type_label}"));
        let show_all_item = h_flex()
            .id("show-all-type")
            .w_full()
            .px_3()
            .py_1p5()
            .gap_2p5()
            .items_center()
            .rounded(RADIUS)
            .cursor_pointer()
            .when(show_all_selected, |el| {
                el.bg(crate::theme::surface_active(cx))
            })
            .on_click(cx.listener(|this, _, window, cx| {
                this.selected_idx = 0;
                this.confirm_selection(window, cx);
            }))
            .child(
                Icon::new(IconName::Search)
                    .with_size(ICON_XS)
                    .text_color(muted),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(FONT_SIZE_BODY)
                    .child(show_all_label),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .child("Search"),
                    )
                    .when(show_all_selected, |el| {
                        el.child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .child("Run \u{21b5}"),
                        )
                    }),
            );

        // File results at index 1+
        let file_results: Vec<_> = self
            .results
            .iter()
            .enumerate()
            .take(20)
            .map(|(i, result)| {
                let item_idx = i + 1; // shifted by 1 for "Show All"
                let ext = std::path::Path::new(&result.node.name)
                    .extension()
                    .map(|e| e.to_string_lossy().to_string());
                let is_dir = result.node.node_type == zero::index::NodeType::Directory;
                let path = result.node.path.clone();
                let label = Self::action_label_for_result(is_dir);
                div()
                    .id(SharedString::from(format!("click-result-{i}")))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_idx = item_idx;
                        this.confirm_selection(window, cx);
                    }))
                    .child(
                        PaletteItem::new(
                            SharedString::from(format!("result-{i}")),
                            SharedString::from(result.node.name.clone()),
                            SharedString::from(path),
                            ext,
                            is_dir,
                        )
                        .action_label(label)
                        .selected(item_idx == self.selected_idx),
                    )
            })
            .collect();
        let results_len = self.results.len();
        let app_rows: Vec<_> = self
            .app_results
            .iter()
            .enumerate()
            .map(|(ai, app)| {
                let item_idx = 1 + results_len + ai; // shifted by 1
                let selected = self.selected_idx == item_idx;
                let path_str = app.path.to_string_lossy().to_string();
                div()
                    .id(SharedString::from(format!("click-app-{ai}")))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_idx = item_idx;
                        this.confirm_selection(window, cx);
                    }))
                    .child(
                        PaletteItem::new(
                            SharedString::from(format!("app-{ai}")),
                            SharedString::from(app.name.clone()),
                            SharedString::from(path_str),
                            Some("app".to_string()),
                            false,
                        )
                        .category("Application")
                        .action_label("Launch \u{21b5}")
                        .selected(selected),
                    )
            })
            .collect();
        el.child(
            v_flex()
                .id("palette-results")
                .flex_1()
                .overflow_y_scroll()
                .track_scroll(&self.scroll_handle)
                .py_1()
                .child(show_all_item)
                .children(file_results)
                .when(!app_rows.is_empty(), |el| {
                    el.child(Self::section_header("Applications", muted))
                        .children(app_rows)
                }),
        )
    }

    fn render_root_results(&self, el: Div, cx: &mut Context<Self>) -> Div {
        let total = self.root_result_total();
        if total == 0 {
            return el.child(
                div()
                    .px_3()
                    .py_4()
                    .text_size(FONT_SIZE_CAPTION)
                    .text_color(cx.theme().muted_foreground)
                    .child("No results found"),
            );
        }
        let muted = cx.theme().muted_foreground;
        let (p_count, _, _, _, _, _, _) = self.root_result_sections();
        let mut idx_offset: usize = 0;

        // -- Path detection item --
        let path_rows: Vec<_> = if let Some(detected) = Self::detect_path(&self.query) {
            let item_idx = 0;
            let selected = self.selected_idx == item_idx;
            let name = detected.to_string_lossy().to_string();
            let subtitle = if detected.is_file() {
                "Open file"
            } else {
                "Go to path"
            };
            vec![
                div()
                    .id("click-path-detect")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_idx = item_idx;
                        this.confirm_selection(window, cx);
                    }))
                    .child(
                        PaletteItem::new(
                            "path-detect",
                            SharedString::from(name),
                            SharedString::from(subtitle),
                            None,
                            true,
                        )
                        .action_label("Open \u{21b5}")
                        .selected(selected),
                    ),
            ]
        } else {
            vec![]
        };
        idx_offset += p_count;

        // -- Applications --
        let app_rows: Vec<_> = self
            .app_results
            .iter()
            .enumerate()
            .map(|(ai, app)| {
                let item_idx = idx_offset + ai;
                let selected = self.selected_idx == item_idx;
                let path_str = app.path.to_string_lossy().to_string();
                div()
                    .id(SharedString::from(format!("click-rapp-{ai}")))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_idx = item_idx;
                        this.confirm_selection(window, cx);
                    }))
                    .child(
                        PaletteItem::new(
                            SharedString::from(format!("rapp-{ai}")),
                            SharedString::from(app.name.clone()),
                            SharedString::from(path_str),
                            Some("app".to_string()),
                            false,
                        )
                        .category("Application")
                        .action_label("Launch \u{21b5}")
                        .selected(selected),
                    )
            })
            .collect();
        idx_offset += self.app_results.len();

        // -- Filtered bookmarks --
        let filt_bm = self.filtered_bookmarks();
        let bm_rows: Vec<_> = filt_bm
            .iter()
            .enumerate()
            .map(|(fi, &(_, path))| {
                let item_idx = idx_offset + fi;
                let selected = self.selected_idx == item_idx;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                div()
                    .id(SharedString::from(format!("click-fbm-{fi}")))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_idx = item_idx;
                        this.confirm_selection(window, cx);
                    }))
                    .child(
                        PaletteItem::new(
                            SharedString::from(format!("fbm-{fi}")),
                            SharedString::from(name),
                            SharedString::from(""),
                            None,
                            true,
                        )
                        .category("Bookmark")
                        .action_label("Open \u{21b5}")
                        .selected(selected),
                    )
            })
            .collect();
        idx_offset += filt_bm.len();

        // -- Filtered storages --
        let filt_st = self.filtered_storages();
        let st_rows: Vec<_> = filt_st
            .iter()
            .enumerate()
            .map(|(fi, &(_, storage))| {
                let item_idx = idx_offset + fi;
                let selected = self.selected_idx == item_idx;
                let subtitle = if storage.is_external {
                    String::new()
                } else {
                    storage.mount_point.to_string_lossy().to_string()
                };
                div()
                    .id(SharedString::from(format!("click-fst-{fi}")))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_idx = item_idx;
                        this.confirm_selection(window, cx);
                    }))
                    .child(
                        PaletteItem::new(
                            SharedString::from(format!("fst-{fi}")),
                            SharedString::from(storage.name.clone()),
                            SharedString::from(subtitle),
                            None,
                            true,
                        )
                        .category(if storage.is_external {
                            "External"
                        } else {
                            "Storage"
                        })
                        .action_label("Open \u{21b5}")
                        .selected(selected),
                    )
            })
            .collect();
        idx_offset += filt_st.len();

        // -- "Search Files" dynamic item + filtered actions --
        let search_files_idx = idx_offset;
        let search_files_selected = self.selected_idx == search_files_idx;
        let query_clone = self.query.clone();
        idx_offset += 1;

        let filt_act = self.filtered_actions();
        let action_rows: Vec<_> = filt_act
            .iter()
            .enumerate()
            .map(|(fi, &(_, action))| {
                let item_idx = idx_offset + fi;
                let selected = self.selected_idx == item_idx;
                let is_type_search = action.path.starts_with("type://") || action.path == "apps://";
                let label = Self::action_label_for_action(action);
                h_flex()
                    .id(SharedString::from(format!("faction-{fi}")))
                    .w_full()
                    .px_3()
                    .py_1p5()
                    .gap_2p5()
                    .items_center()
                    .rounded(RADIUS)
                    .cursor_pointer()
                    .when(selected, |el| el.bg(crate::theme::surface_active(cx)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_idx = item_idx;
                        this.confirm_selection(window, cx);
                    }))
                    .child(
                        Icon::new((action.icon)())
                            .with_size(ICON_XS)
                            .text_color(muted),
                    )
                    .child(div().flex_1().text_size(FONT_SIZE_BODY).child(action.name))
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_size(FONT_SIZE_CAPTION)
                                    .text_color(muted)
                                    .child(action.category),
                            )
                            .when(selected, |el| {
                                el.child(
                                    div()
                                        .text_size(FONT_SIZE_CAPTION)
                                        .text_color(muted)
                                        .child(if is_type_search { "Search >" } else { label }),
                                )
                            }),
                    )
            })
            .collect();
        idx_offset += filt_act.len();

        // -- File results (limit 8, folders-first) --
        let file_rows: Vec<_> = self
            .results
            .iter()
            .enumerate()
            .take(8)
            .map(|(fi, result)| {
                let item_idx = idx_offset + fi;
                let selected = self.selected_idx == item_idx;
                let ext = std::path::Path::new(&result.node.name)
                    .extension()
                    .map(|e| e.to_string_lossy().to_string());
                let is_dir = result.node.node_type == zero::index::NodeType::Directory;
                let path = result.node.path.clone();
                let label = Self::action_label_for_result(is_dir);
                div()
                    .id(SharedString::from(format!("click-fres-{fi}")))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_idx = item_idx;
                        this.confirm_selection(window, cx);
                    }))
                    .child(
                        PaletteItem::new(
                            SharedString::from(format!("fres-{fi}")),
                            SharedString::from(result.node.name.clone()),
                            SharedString::from(path),
                            ext,
                            is_dir,
                        )
                        .action_label(label)
                        .selected(selected),
                    )
            })
            .collect();

        el.child(
            v_flex()
                .id("palette-results")
                .flex_1()
                .overflow_y_scroll()
                .track_scroll(&self.scroll_handle)
                .py_1()
                .children(path_rows)
                .when(!app_rows.is_empty(), |el| {
                    el.child(Self::section_header("Applications", muted))
                        .children(app_rows)
                })
                .when(!bm_rows.is_empty(), |el| {
                    el.child(Self::section_header("Bookmarks", muted))
                        .children(bm_rows)
                })
                .when(!st_rows.is_empty(), |el| {
                    el.child(Self::section_header("Storages", muted))
                        .children(st_rows)
                })
                .child(Self::section_header("Actions", muted))
                .child(
                    h_flex()
                        .id("search-files-item")
                        .w_full()
                        .px_3()
                        .py_1p5()
                        .gap_2p5()
                        .items_center()
                        .rounded(RADIUS)
                        .cursor_pointer()
                        .when(search_files_selected, |el| {
                            el.bg(crate::theme::surface_active(cx))
                        })
                        .on_click(cx.listener(move |_this, _, _window, cx| {
                            cx.emit(PaletteEvent::ShowInBrowser(query_clone.clone()));
                        }))
                        .child(
                            Icon::new(IconName::Search)
                                .with_size(ICON_XS)
                                .text_color(muted),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_size(FONT_SIZE_BODY)
                                .child("Search Files"),
                        )
                        .child(
                            h_flex()
                                .flex_shrink_0()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .text_size(FONT_SIZE_CAPTION)
                                        .text_color(muted)
                                        .child("Search"),
                                )
                                .when(search_files_selected, |el| {
                                    el.child(
                                        div()
                                            .text_size(FONT_SIZE_CAPTION)
                                            .text_color(muted)
                                            .child("Run \u{21b5}"),
                                    )
                                }),
                        ),
                )
                .children(action_rows)
                .when(!file_rows.is_empty(), |el| {
                    el.child(Self::section_header("Files", muted))
                        .children(file_rows)
                }),
        )
    }

    fn render_default_sections(&self, el: Div, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let bookmark_count = self.bookmarks.len();
        let storage_count = self.storages.len();

        let bookmark_rows: Vec<_> = self
            .bookmarks
            .iter()
            .enumerate()
            .map(|(bi, path)| {
                let item_idx = bi;
                let selected = self.selected_idx == item_idx;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());

                div()
                    .id(SharedString::from(format!("click-bm-{bi}")))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_idx = item_idx;
                        this.confirm_selection(window, cx);
                    }))
                    .child(
                        PaletteItem::new(
                            SharedString::from(format!("bm-{bi}")),
                            SharedString::from(name),
                            SharedString::from(""),
                            None,
                            true,
                        )
                        .category("Bookmark")
                        .action_label("Open \u{21b5}")
                        .selected(selected),
                    )
            })
            .collect();

        let storage_rows: Vec<_> = self
            .storages
            .iter()
            .enumerate()
            .map(|(si, storage)| {
                let item_idx = bookmark_count + si;
                let selected = self.selected_idx == item_idx;
                let subtitle = if storage.is_external {
                    String::new()
                } else {
                    storage.mount_point.to_string_lossy().to_string()
                };

                div()
                    .id(SharedString::from(format!("click-st-{si}")))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_idx = item_idx;
                        this.confirm_selection(window, cx);
                    }))
                    .child(
                        PaletteItem::new(
                            SharedString::from(format!("st-{si}")),
                            SharedString::from(storage.name.clone()),
                            SharedString::from(subtitle),
                            None,
                            true,
                        )
                        .category(if storage.is_external {
                            "External"
                        } else {
                            "Storage"
                        })
                        .action_label("Open \u{21b5}")
                        .selected(selected),
                    )
            })
            .collect();

        let action_rows: Vec<_> = DEFAULT_ACTIONS
            .iter()
            .enumerate()
            .map(|(ai, action)| {
                let item_idx = bookmark_count + storage_count + ai;
                let selected = self.selected_idx == item_idx;
                let is_type_search = action.path.starts_with("type://") || action.path == "apps://";
                let label = Self::action_label_for_action(action);

                h_flex()
                    .id(SharedString::from(format!("action-{ai}")))
                    .w_full()
                    .px_3()
                    .py_1p5()
                    .gap_2p5()
                    .items_center()
                    .rounded(RADIUS)
                    .cursor_pointer()
                    .when(selected, |el| el.bg(crate::theme::surface_active(cx)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_idx = item_idx;
                        this.confirm_selection(window, cx);
                    }))
                    .child(
                        Icon::new((action.icon)())
                            .with_size(ICON_XS)
                            .text_color(muted),
                    )
                    .child(div().flex_1().text_size(FONT_SIZE_BODY).child(action.name))
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_size(FONT_SIZE_CAPTION)
                                    .text_color(muted)
                                    .child(action.category),
                            )
                            .when_some(action.shortcut, |el, sc| {
                                el.child(
                                    div()
                                        .text_size(FONT_SIZE_CAPTION)
                                        .text_color(muted)
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded(RADIUS)
                                        .bg(cx.theme().muted)
                                        .child(sc),
                                )
                            })
                            .when(selected, |el| {
                                el.child(
                                    div()
                                        .text_size(FONT_SIZE_CAPTION)
                                        .text_color(muted)
                                        .child(if is_type_search { "Search >" } else { label }),
                                )
                            }),
                    )
            })
            .collect();

        el.child(
            v_flex()
                .id("palette-defaults")
                .flex_1()
                .overflow_y_scroll()
                .track_scroll(&self.scroll_handle)
                .py_1()
                .when(!bookmark_rows.is_empty(), |el| {
                    el.child(Self::section_header("Bookmarks", muted))
                        .children(bookmark_rows)
                })
                .when(!storage_rows.is_empty(), |el| {
                    el.child(Self::section_header("Storages", muted))
                        .children(storage_rows)
                })
                .child(Self::section_header("Actions", muted))
                .children(action_rows),
        )
    }
}
