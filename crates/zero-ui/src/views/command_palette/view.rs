use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _, h_flex,
    input::{Input, InputState},
    v_flex,
};

use crate::services::apps::AppEntry;
use crate::services::{AppService, SearchService};
use crate::theme::{
    self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, ICON_XS, MODAL_PALETTE_WIDTH, RADIUS, RADIUS_LG,
};

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
    category: &'static str,
    shortcut: Option<&'static str>,
}

const DEFAULT_ACTIONS: &[PaletteAction] = &[
    PaletteAction {
        name: "Settings",
        icon: || IconName::Settings,
        path: "action://settings",
        category: "View",
        shortcut: Some("\u{2318},"),
    },
    PaletteAction {
        name: "New Folder",
        icon: || IconName::Folder,
        path: "action://new_folder",
        category: "Files",
        shortcut: Some("\u{21e7}\u{2318}N"),
    },
    PaletteAction {
        name: "Toggle Sidebar",
        icon: || IconName::PanelLeft,
        path: "action://toggle_sidebar",
        category: "View",
        shortcut: Some("\u{2318}B"),
    },
    PaletteAction {
        name: "Toggle Split View",
        icon: || IconName::LayoutDashboard,
        path: "action://toggle_split_view",
        category: "View",
        shortcut: Some("\u{2318}T"),
    },
    PaletteAction {
        name: "Go Back",
        icon: || IconName::ArrowLeft,
        path: "action://go_back",
        category: "Navigation",
        shortcut: Some("\u{2318}["),
    },
    PaletteAction {
        name: "Go Forward",
        icon: || IconName::ArrowRight,
        path: "action://go_forward",
        category: "Navigation",
        shortcut: Some("\u{2318}]"),
    },
    PaletteAction {
        name: "Go Up",
        icon: || IconName::ArrowUp,
        path: "action://go_up",
        category: "Navigation",
        shortcut: Some("\u{2318}\u{2191}"),
    },
    PaletteAction {
        name: "Storage Cleanup",
        icon: || IconName::Delete,
        path: "action://cleanup",
        category: "View",
        shortcut: None,
    },
    PaletteAction {
        name: "Find Duplicates",
        icon: || IconName::File,
        path: "action://dedup",
        category: "View",
        shortcut: None,
    },
    PaletteAction {
        name: "Tasks",
        icon: || IconName::Check,
        path: "action://todo",
        category: "View",
        shortcut: None,
    },
    PaletteAction {
        name: "Secure Erase",
        icon: || IconName::Delete,
        path: "action://secure_erase",
        category: "View",
        shortcut: None,
    },
    PaletteAction {
        name: "Automations",
        icon: || IconName::Settings,
        path: "action://automations",
        category: "View",
        shortcut: None,
    },
    PaletteAction {
        name: "Search Images",
        icon: || IconName::Eye,
        path: "type://images",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "Search Videos",
        icon: || IconName::Eye,
        path: "type://videos",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "Search Audio",
        icon: || IconName::Search,
        path: "type://audio",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "Search Documents",
        icon: || IconName::File,
        path: "type://documents",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "Search Code",
        icon: || IconName::Search,
        path: "type://code",
        category: "Search",
        shortcut: None,
    },
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
    scroll_handle: ScrollHandle,
}

impl PaletteView {
    pub fn new(
        search: Entity<SearchService>,
        apps: Entity<AppService>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search files..."));

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
            scroll_handle: ScrollHandle::new(),
        }
    }

    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.clear();
        self.results.clear();
        self.selected_idx = 0;
        self.input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.input.focus_handle(cx).focus(window);
        cx.notify();
    }

    /// Set the query text programmatically (e.g. from type search action).
    pub fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let query = query.to_string();
        self.input.update(cx, |state, cx| {
            state.set_value(&query, window, cx);
        });
        self.perform_search(&query, cx);
        self.input.focus_handle(cx).focus(window);
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

    /// Compute the scroll child index accounting for section headers.
    fn scroll_child_index(&self) -> usize {
        if !self.query.is_empty() {
            // With results: items are file results, then optionally "Applications" header + apps
            if self.selected_idx < self.results.len() {
                self.selected_idx
            } else {
                // Past file results: +1 for the "Applications" section header
                self.selected_idx + 1
            }
        } else {
            // Default view: bookmarks section header + bookmarks, then actions header + actions
            let bookmark_count = self.bookmarks.len();
            let headers_before = if bookmark_count > 0 {
                if self.selected_idx < bookmark_count {
                    1 // "Bookmarks" header
                } else {
                    2 // "Bookmarks" + "Actions" headers
                }
            } else {
                1 // "Actions" header only
            };
            self.selected_idx + headers_before
        }
    }

    fn select_prev(&mut self, cx: &mut Context<Self>) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
            self.scroll_handle.scroll_to_item(self.scroll_child_index());
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
            self.scroll_handle.scroll_to_item(self.scroll_child_index());
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

    /// Action label for a result item.
    fn action_label_for_result(_is_dir: bool) -> &'static str {
        "Open \u{21b5}"
    }

    fn action_label_for_action(action: &PaletteAction) -> &'static str {
        if action.path.starts_with("type://") {
            "Search >"
        } else if action.path.starts_with("app://") {
            "Launch \u{21b5}"
        } else {
            "Run \u{21b5}"
        }
    }
}

impl Render for PaletteView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            // Search input with icon prefix
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Input::new(&self.input).appearance(false).prefix(
                            Icon::new(IconName::Search)
                                .with_size(ICON_XS)
                                .text_color(cx.theme().muted_foreground),
                        ),
                    ),
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
                        let is_dir = result.node.node_type == zero::index::NodeType::Directory;
                        let path = result.node.path.clone();
                        let label = Self::action_label_for_result(is_dir);

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
                                .action_label(label)
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
                                .category("Bookmark")
                                .action_label("Open \u{21b5}")
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
                        let is_type_search = action.path.starts_with("type://");
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
                            .hover(|s| s.bg(crate::theme::surface_hover(cx)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_idx = item_idx;
                                this.confirm_selection(cx);
                            }))
                            .child(
                                Icon::new((action.icon)())
                                    .with_size(ICON_XS)
                                    .text_color(muted),
                            )
                            .child(div().flex_1().text_size(FONT_SIZE_BODY).child(action.name))
                            // Right side
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
                                                .child(if is_type_search {
                                                    "Search >"
                                                } else {
                                                    label
                                                }),
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
