use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    h_flex,
    input::{Input, InputState},
    v_flex, ActiveTheme, Icon, IconName, Sizable as _,
};

use crate::services::{AppService, SearchService};
use crate::services::apps::AppEntry;
use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, MODAL_PALETTE_WIDTH, RADIUS, RADIUS_LG};

use super::items::PaletteItem;

// -- Events emitted to the parent ZeroApp ------------------------------------

pub enum PaletteEvent {
    Dismiss,
    OpenResult(PathBuf),
    ShowInBrowser(String),
}

impl EventEmitter<PaletteEvent> for PaletteView {}

// -- View --------------------------------------------------------------------

/// Action entry for the palette's default section.
struct PaletteAction {
    name: &'static str,
    icon: fn() -> IconName,
    path: &'static str,
}

const DEFAULT_ACTIONS: &[PaletteAction] = &[
    PaletteAction { name: "Settings", icon: || IconName::Settings, path: "action://settings" },
    PaletteAction { name: "Storage Cleanup", icon: || IconName::Delete, path: "action://cleanup" },
    PaletteAction { name: "Find Duplicates", icon: || IconName::File, path: "action://dedup" },
    PaletteAction { name: "Tasks", icon: || IconName::Check, path: "action://todo" },
    PaletteAction { name: "Secure Erase", icon: || IconName::Delete, path: "action://secure_erase" },
    PaletteAction { name: "Automations", icon: || IconName::Settings, path: "action://automations" },
    PaletteAction { name: "Search Images", icon: || IconName::Eye, path: "type://images" },
    PaletteAction { name: "Search Videos", icon: || IconName::Eye, path: "type://videos" },
    PaletteAction { name: "Search Audio", icon: || IconName::Search, path: "type://audio" },
    PaletteAction { name: "Search Documents", icon: || IconName::File, path: "type://documents" },
    PaletteAction { name: "Search Code", icon: || IconName::Search, path: "type://code" },
];

pub struct PaletteView {
    search: Entity<SearchService>,
    apps: Entity<AppService>,
    input: Entity<InputState>,
    results: Vec<zero::prelude::SearchResult>,
    app_results: Vec<AppEntry>,
    bookmarks: Vec<PathBuf>,
    selected_idx: usize,
    query: String,
    focus_handle: FocusHandle,
}

impl PaletteView {
    pub fn new(
        search: Entity<SearchService>,
        apps: Entity<AppService>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search files...")
        });

        let input_sub = cx.subscribe(
            &input,
            |this: &mut Self, input, _ev: &gpui_component::input::InputEvent, cx| {
                let text = input.read(cx).value().to_string();
                this.perform_search(&text, cx);
                cx.notify();
            },
        );
        let _ = input_sub;

        let bookmarks = crate::session::Settings::load().sidebar_bookmarks;

        Self {
            search,
            apps,
            input,
            results: Vec::new(),
            app_results: Vec::new(),
            bookmarks,
            selected_idx: 0,
            query: String::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.clear();
        self.results.clear();
        self.selected_idx = 0;
        self.input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.focus_handle.focus(window);
        cx.notify();
    }

    /// Set the query text programmatically (e.g. from type search action).
    pub fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let query = query.to_string();
        self.input.update(cx, |state, cx| {
            state.set_value(&query, window, cx);
        });
        self.perform_search(&query, cx);
        self.focus_handle.focus(window);
        cx.notify();
    }

    fn perform_search(&mut self, query: &str, cx: &mut Context<Self>) {
        self.query = query.to_string();
        self.selected_idx = 0;

        if query.is_empty() {
            self.results.clear();
            self.app_results.clear();
            return;
        }

        // Handle @type prefix: e.g. "@images sunset" searches images for "sunset"
        if let Some(rest) = query.strip_prefix('@') {
            let (type_str, text_query) = match rest.split_once(' ') {
                Some((t, q)) => (t, q.trim()),
                None => (rest, ""),
            };

            // Type-only search (no text query)
            if text_query.is_empty() {
                self.results = self.search.read(cx).search_by_type(type_str, 50);
            } else {
                // Search by type, then filter by text query
                let type_results = self.search.read(cx).search_by_type(type_str, 200);
                let query_lower = text_query.to_lowercase();
                self.results = type_results
                    .into_iter()
                    .filter(|r| r.node.name.to_lowercase().contains(&query_lower))
                    .take(50)
                    .collect();
            }
            self.app_results.clear();
            return;
        }

        self.results = self.search.read(cx).search(query, 50);
        self.app_results = self.apps.read(cx).search(query, 5);
    }

    fn select_prev(&mut self, cx: &mut Context<Self>) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
            cx.notify();
        }
    }

    fn select_next(&mut self, cx: &mut Context<Self>) {
        let max = if self.query.is_empty() {
            self.default_item_count()
        } else {
            self.results.len() + self.app_results.len()
        };
        if self.selected_idx + 1 < max {
            self.selected_idx += 1;
            cx.notify();
        }
    }

    fn default_item_count(&self) -> usize {
        self.bookmarks.len() + DEFAULT_ACTIONS.len()
    }

    fn confirm_selection(&mut self, cx: &mut Context<Self>) {
        if !self.query.is_empty() {
            // File results first, then app results
            if let Some(result) = self.results.get(self.selected_idx) {
                let path = PathBuf::from(&result.node.path);
                cx.emit(PaletteEvent::OpenResult(path));
                return;
            }
            let app_idx = self.selected_idx.saturating_sub(self.results.len());
            if let Some(app) = self.app_results.get(app_idx) {
                let path_str = format!("app://{}", app.path.display());
                cx.emit(PaletteEvent::OpenResult(PathBuf::from(path_str)));
            }
            return;
        }

        let bookmark_count = self.bookmarks.len();
        if self.selected_idx < bookmark_count {
            if let Some(path) = self.bookmarks.get(self.selected_idx) {
                cx.emit(PaletteEvent::OpenResult(path.clone()));
            }
        } else {
            let action_idx = self.selected_idx - bookmark_count;
            if let Some(action) = DEFAULT_ACTIONS.get(action_idx) {
                cx.emit(PaletteEvent::OpenResult(PathBuf::from(action.path)));
            }
        }
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(PaletteEvent::Dismiss);
    }
}

impl Render for PaletteView {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let has_results = !self.results.is_empty();
        let has_query = !self.query.is_empty();

        v_flex()
            .track_focus(&self.focus_handle)
            .key_context("PaletteView")
            .on_action(cx.listener(|this, _: &crate::actions::GoBack, _, cx| {
                this.dismiss(cx);
            }))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                match &ev.keystroke.key {
                    key if key == "escape" => this.dismiss(cx),
                    key if key == "enter" && ev.keystroke.modifiers.platform => {
                        // Cmd+Enter: show all results in file browser
                        if !this.query.is_empty() {
                            cx.emit(PaletteEvent::ShowInBrowser(this.query.clone()));
                        }
                    }
                    key if key == "enter" => this.confirm_selection(cx),
                    key if key == "up" => this.select_prev(cx),
                    key if key == "down" => this.select_next(cx),
                    _ => {}
                }
            }))
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
            // Search input
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(Input::new(&self.input)),
            )
            // Results list (files + apps)
            .when(has_query, |el| {
                let has_any = has_results || !self.app_results.is_empty();

                if !has_any {
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
                let file_results: Vec<_> = self
                    .results
                    .iter()
                    .enumerate()
                    .take(20)
                    .map(|(i, result)| {
                        let ext = std::path::Path::new(&result.node.name)
                            .extension()
                            .map(|e| e.to_string_lossy().to_string());
                        let is_dir =
                            result.node.node_type == zero::index::NodeType::Directory;
                        let path = result.node.path.clone();

                        div()
                            .id(SharedString::from(format!("click-result-{i}")))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_idx = i;
                                this.confirm_selection(cx);
                            }))
                            .child(
                                PaletteItem::new(
                                    SharedString::from(format!("result-{i}")),
                                    SharedString::from(result.node.name.clone()),
                                    SharedString::from(path),
                                    ext,
                                    is_dir,
                                )
                                .selected(i == self.selected_idx),
                            )
                    })
                    .collect();

                let results_len = self.results.len();
                let app_rows: Vec<_> = self
                    .app_results
                    .iter()
                    .enumerate()
                    .map(|(ai, app)| {
                        let item_idx = results_len + ai;
                        let selected = self.selected_idx == item_idx;
                        let path_str = app.path.to_string_lossy().to_string();

                        div()
                            .id(SharedString::from(format!("click-app-{ai}")))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_idx = item_idx;
                                this.confirm_selection(cx);
                            }))
                            .child(
                                PaletteItem::new(
                                    SharedString::from(format!("app-{ai}")),
                                    SharedString::from(app.name.clone()),
                                    SharedString::from(path_str),
                                    Some("app".to_string()),
                                    false,
                                )
                                .selected(selected),
                            )
                    })
                    .collect();

                el.child(
                    v_flex()
                        .id("palette-results")
                        .flex_1()
                        .overflow_y_scroll()
                        .py_1()
                        .children(file_results)
                        .when(!app_rows.is_empty(), |el| {
                            el.child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .text_size(FONT_SIZE_CAPTION)
                                    .text_color(muted)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Applications"),
                            )
                            .children(app_rows)
                        }),
                )
            })
            // Default sections when no query
            .when(!has_query, |el| {
                let muted = cx.theme().muted_foreground;
                let bookmark_count = self.bookmarks.len();

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
                        let path_str = path.to_string_lossy().to_string();

                        div()
                            .id(SharedString::from(format!("click-bm-{bi}")))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_idx = item_idx;
                                this.confirm_selection(cx);
                            }))
                            .child(
                                PaletteItem::new(
                                    SharedString::from(format!("bm-{bi}")),
                                    SharedString::from(name),
                                    SharedString::from(path_str),
                                    None,
                                    true,
                                )
                                .selected(selected),
                            )
                    })
                    .collect();

                let action_rows: Vec<_> = DEFAULT_ACTIONS
                    .iter()
                    .enumerate()
                    .map(|(ai, action)| {
                        let item_idx = bookmark_count + ai;
                        let selected = self.selected_idx == item_idx;

                        h_flex()
                            .id(SharedString::from(format!("action-{ai}")))
                            .w_full()
                            .px_3()
                            .py_1p5()
                            .gap_2p5()
                            .items_center()
                            .rounded(RADIUS)
                            .cursor_pointer()
                            .when(selected, |el| {
                                el.bg(crate::theme::surface_active(cx))
                            })
                            .hover(|s| s.bg(crate::theme::surface_hover(cx)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_idx = item_idx;
                                this.confirm_selection(cx);
                            }))
                            .child(
                                Icon::new((action.icon)())
                                    .xsmall()
                                    .text_color(muted),
                            )
                            .child(
                                div()
                                    .text_size(FONT_SIZE_BODY)
                                    .child(action.name),
                            )
                    })
                    .collect();

                el.child(
                    v_flex()
                        .id("palette-defaults")
                        .flex_1()
                        .overflow_y_scroll()
                        .py_1()
                        // Bookmarks
                        .when(!bookmark_rows.is_empty(), |el| {
                            el.child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .text_size(FONT_SIZE_CAPTION)
                                    .text_color(muted)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Bookmarks"),
                            )
                            .children(bookmark_rows)
                        })
                        // Actions
                        .child(
                            div()
                                .px_3()
                                .py_1()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Actions"),
                        )
                        .children(action_rows),
                )
            })
    }
}
