use std::path::PathBuf;
use std::time::Duration;

use gpui::*;
use gpui_component::input::InputState;

use crate::services::apps::AppEntry;
use crate::services::search::SearchEvent;
use crate::services::{AppService, SearchService};
use crate::theme::FONT_SIZE_CAPTION;

use super::items::{DEFAULT_ACTIONS, PaletteAction, StorageEntry};

// -- Events emitted to the parent ZeroApp ------------------------------------

pub enum PaletteEvent {
    Dismiss,
    OpenResult(PathBuf),
    ShowInBrowser(String),
    ShowTypeInBrowser {
        type_filter: String,
        query: String,
        label: String,
    },
}

impl EventEmitter<PaletteEvent> for PaletteView {}

// -- View --------------------------------------------------------------------

#[derive(Clone, PartialEq)]
pub(super) enum PaletteMode {
    Root,
    DrilledIn { type_filter: String, label: String },
}

pub struct PaletteView {
    pub(super) search: Entity<SearchService>,
    pub(super) apps: Entity<AppService>,
    pub(super) input: Entity<InputState>,
    pub(super) results: Vec<search::SearchResult>,
    pub(super) app_results: Vec<AppEntry>,
    pub(super) bookmarks: Vec<PathBuf>,
    pub(super) storages: Vec<StorageEntry>,
    pub(super) selected_idx: usize,
    pub(super) query: String,
    pub(super) mode: PaletteMode,
    pub(super) focus_handle: FocusHandle,
    _input_sub: Subscription,
    _search_sub: Subscription,
    pub(super) scroll_handle: ScrollHandle,
}

impl PaletteView {
    pub fn new(
        search: Entity<SearchService>,
        apps: Entity<AppService>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));

        let input_sub = cx.subscribe(
            &input,
            |this: &mut Self, input, _ev: &gpui_component::input::InputEvent, cx| {
                let text = input.read(cx).value().to_string();
                this.perform_search(&text, cx);
                cx.notify();
            },
        );

        let search_sub = cx.subscribe(
            &search,
            |this: &mut Self, _, ev: &SearchEvent, cx| match ev {
                SearchEvent::RootLoaded { .. }
                | SearchEvent::IndexLoaded
                | SearchEvent::IndexingFinished => {
                    if !this.query.is_empty() {
                        let q = this.query.clone();
                        this.perform_search(&q, cx);
                    }
                    cx.notify();
                }
                _ => {}
            },
        );

        let settings = crate::session::Settings::load();
        let bookmarks = settings.active_ws().pinned_bookmarks.clone();

        // Poll every 500ms while loading/indexing so the banner file count updates live.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let still_loading = this
                    .update(cx, |view, cx| {
                        let loading = view.is_loading(cx);
                        if loading {
                            cx.notify();
                        }
                        loading
                    })
                    .unwrap_or(false);
                if !still_loading {
                    break;
                }
            }
        })
        .detach();

        Self {
            search,
            apps,
            input,
            results: Vec::new(),
            app_results: Vec::new(),
            bookmarks,
            storages: Vec::new(),
            selected_idx: 0,
            query: String::new(),
            mode: PaletteMode::Root,
            focus_handle: cx.focus_handle(),
            _input_sub: input_sub,
            _search_sub: search_sub,
            scroll_handle: ScrollHandle::new(),
        }
    }

    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.clear();
        self.results.clear();
        self.selected_idx = 0;
        self.mode = PaletteMode::Root;
        // Read cached storages from the search service (no I/O)
        self.storages = self
            .search
            .read(cx)
            .storages()
            .iter()
            .map(|s| StorageEntry {
                name: s.name.clone(),
                mount_point: s.mount_point.clone(),
                is_external: s.is_external,
            })
            .collect();
        self.input.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.set_placeholder("Search...", window, cx);
        });
        self.input.focus_handle(cx).focus(window);
        cx.notify();
    }

    pub(super) fn drill_into(
        &mut self,
        type_filter: &str,
        label: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(type_filter, label, "palette: drill into");
        self.mode = PaletteMode::DrilledIn {
            type_filter: type_filter.to_string(),
            label: label.to_string(),
        };
        self.query.clear();
        self.selected_idx = 0;
        let placeholder = format!("Search {}...", label.to_lowercase());
        self.input.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.set_placeholder(&placeholder, window, cx);
        });
        if type_filter == "apps" {
            self.results.clear();
            self.app_results = self.apps.read(cx).list(50);
        } else {
            self.app_results.clear();
            self.results = self.search.read(cx).search_by_type(type_filter, 50);
        }
        cx.notify();
    }

    pub(super) fn exit_drill(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!("palette: exit drill");
        self.mode = PaletteMode::Root;
        self.query.clear();
        self.results.clear();
        self.app_results.clear();
        self.selected_idx = 0;
        self.input.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.set_placeholder("Search...", window, cx);
        });
        cx.notify();
    }

    pub(super) fn perform_search(&mut self, query: &str, cx: &mut Context<Self>) {
        tracing::debug!(query, "palette: search");
        self.query = query.to_string();
        self.selected_idx = 0;

        match &self.mode {
            PaletteMode::Root => {
                if query.is_empty() {
                    self.results.clear();
                    self.app_results.clear();
                    return;
                }
                self.results = self.search.read(cx).search(query, 50);
                // Sort folders first
                self.results.sort_by(|a, b| {
                    let a_dir = a.node.node_type == search::NodeType::Directory;
                    let b_dir = b.node.node_type == search::NodeType::Directory;
                    b_dir.cmp(&a_dir)
                });
                self.app_results = self.apps.read(cx).search(query, 5);
            }
            PaletteMode::DrilledIn { type_filter, .. } => {
                let tf = type_filter.clone();
                if tf == "apps" {
                    self.results.clear();
                    if query.is_empty() {
                        self.app_results = self.apps.read(cx).list(50);
                    } else {
                        self.app_results = self.apps.read(cx).search(query, 50);
                    }
                } else {
                    self.app_results.clear();
                    if query.is_empty() {
                        self.results = self.search.read(cx).search_by_type(&tf, 50);
                    } else {
                        self.results = self.search.read(cx).search_with_type(query, &tf, 50);
                    }
                }
            }
        }
    }

    pub(super) fn matches_query(text: &str, query: &str) -> bool {
        text.to_lowercase().contains(&query.to_lowercase())
    }

    pub(super) fn filtered_bookmarks(&self) -> Vec<(usize, &PathBuf)> {
        if self.query.is_empty() {
            return Vec::new();
        }
        self.bookmarks
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                Self::matches_query(&name, &self.query)
            })
            .collect()
    }

    pub(super) fn filtered_storages(&self) -> Vec<(usize, &StorageEntry)> {
        if self.query.is_empty() {
            return Vec::new();
        }
        self.storages
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                Self::matches_query(&s.name, &self.query)
                    || Self::matches_query(&s.mount_point.to_string_lossy(), &self.query)
            })
            .collect()
    }

    pub(super) fn filtered_actions(&self) -> Vec<(usize, &PaletteAction)> {
        if self.query.is_empty() {
            return Vec::new();
        }
        DEFAULT_ACTIONS
            .iter()
            .enumerate()
            .filter(|(_, a)| Self::matches_query(a.name, &self.query))
            .collect()
    }

    pub(super) fn detect_path(query: &str) -> Option<PathBuf> {
        let trimmed = query.trim();
        if trimmed.starts_with('/') {
            let p = PathBuf::from(trimmed);
            if p.exists() || p.parent().is_some_and(|pp| pp.exists()) {
                return Some(p);
            }
        } else if let Some(rest) = trimmed.strip_prefix("~/")
            && let Some(home) = dirs::home_dir()
        {
            let p = home.join(rest);
            if p.exists() || p.parent().is_some_and(|pp| pp.exists()) {
                return Some(p);
            }
        }
        None
    }

    /// Compute section lengths for the filtered results view in Root mode.
    /// Returns (path_detect, apps, bookmarks, storages, search_files_item, actions, files).
    pub(super) fn root_result_sections(&self) -> (usize, usize, usize, usize, usize, usize, usize) {
        let path_count = if Self::detect_path(&self.query).is_some() {
            1
        } else {
            0
        };
        let apps = self.app_results.len();
        let bookmarks = self.filtered_bookmarks().len();
        let storages = self.filtered_storages().len();
        let actions = self.filtered_actions().len();
        // "Search Files" dynamic item always shown when query is non-empty
        let search_files = if self.query.is_empty() { 0 } else { 1 };
        let files = self.results.len().min(8);
        (
            path_count,
            apps,
            bookmarks,
            storages,
            search_files,
            actions,
            files,
        )
    }

    pub(super) fn root_result_total(&self) -> usize {
        let (p, a, b, s, sf, act, f) = self.root_result_sections();
        p + a + b + s + sf + act + f
    }

    pub(super) fn is_showing_results(&self) -> bool {
        !self.query.is_empty() || matches!(self.mode, PaletteMode::DrilledIn { .. })
    }

    /// True when the search service is still loading indexes from disk.
    pub(super) fn is_loading(&self, cx: &Context<Self>) -> bool {
        let svc = self.search.read(cx);
        svc.is_loading() || svc.is_indexing()
    }

    /// Compute the scroll child index accounting for section headers.
    pub(super) fn scroll_child_index(&self) -> usize {
        if self.is_showing_results() {
            if matches!(self.mode, PaletteMode::DrilledIn { .. }) {
                // Drilled-in: "Show All" item + file results, then optionally apps header + apps
                if self.selected_idx < 1 + self.results.len() {
                    self.selected_idx
                } else {
                    self.selected_idx + 1
                }
            } else {
                // Root with query: count section headers before selected_idx
                let (p, a, b, s, sf, act, _f) = self.root_result_sections();
                let idx = self.selected_idx;
                let mut headers = 0;
                let mut offset = 0;
                // Path detection (no header)
                offset += p;
                // Applications section
                if a > 0 && idx >= offset {
                    headers += 1;
                }
                offset += a;
                // Bookmarks section
                if b > 0 && idx >= offset {
                    headers += 1;
                }
                offset += b;
                // Storages section
                if s > 0 && idx >= offset {
                    headers += 1;
                }
                offset += s;
                // Actions section (includes search_files item + filtered actions)
                if (sf + act) > 0 && idx >= offset {
                    headers += 1;
                }
                offset += sf + act;
                // Files section
                if idx >= offset {
                    headers += 1;
                }
                idx + headers
            }
        } else {
            // Default view: bookmarks header + bookmarks, storages header + storages, actions header + actions
            let bookmark_count = self.bookmarks.len();
            let storage_count = self.storages.len();
            let mut headers = 0;
            if bookmark_count > 0 {
                headers += 1; // "Bookmarks" header
            }
            if storage_count > 0 && self.selected_idx >= bookmark_count {
                headers += 1; // "Storages" header
            }
            if self.selected_idx >= bookmark_count + storage_count {
                headers += 1; // "Actions" header
            }
            if headers == 0 {
                headers = 1; // At least "Actions" header
            }
            self.selected_idx + headers
        }
    }

    pub(super) fn select_prev(&mut self, cx: &mut Context<Self>) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
            self.scroll_handle.scroll_to_item(self.scroll_child_index());
            cx.notify();
        }
    }

    pub(super) fn select_next(&mut self, cx: &mut Context<Self>) {
        let max = if self.is_showing_results() {
            if matches!(self.mode, PaletteMode::Root) {
                self.root_result_total()
            } else {
                // +1 for the "Show All" item at index 0
                1 + self.results.len() + self.app_results.len()
            }
        } else {
            self.default_item_count()
        };
        if self.selected_idx + 1 < max {
            self.selected_idx += 1;
            self.scroll_handle.scroll_to_item(self.scroll_child_index());
            cx.notify();
        }
    }

    pub(super) fn default_item_count(&self) -> usize {
        self.bookmarks.len() + self.storages.len() + Self::visible_default_actions().len()
    }

    /// Default actions filtered to those visible in the empty-state palette.
    pub(super) fn visible_default_actions() -> Vec<&'static PaletteAction> {
        DEFAULT_ACTIONS
            .iter()
            .filter(|a| a.default_visible)
            .collect()
    }

    pub(super) fn confirm_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!(
            selected_idx = self.selected_idx,
            "palette: confirm selection"
        );
        if self.is_showing_results() {
            if let PaletteMode::DrilledIn {
                ref type_filter,
                ref label,
            } = self.mode
            {
                // Index 0 = "Show All {Label}" action
                if self.selected_idx == 0 {
                    cx.emit(PaletteEvent::ShowTypeInBrowser {
                        type_filter: type_filter.clone(),
                        query: self.query.clone(),
                        label: label.clone(),
                    });
                    return;
                }
                // File results at 1..1+results.len()
                let file_idx = self.selected_idx - 1;
                if let Some(result) = self.results.get(file_idx) {
                    let path = PathBuf::from(&result.node.path);
                    cx.emit(PaletteEvent::OpenResult(path));
                    return;
                }
                // App results after files
                let app_idx = file_idx.saturating_sub(self.results.len());
                if let Some(app) = self.app_results.get(app_idx) {
                    let path_str = format!("app://{}", app.path.display());
                    cx.emit(PaletteEvent::OpenResult(PathBuf::from(path_str)));
                }
                return;
            }

            // Root mode with query: walk sections
            let (p, a, b, s, sf, act, f) = self.root_result_sections();
            let idx = self.selected_idx;
            let mut offset = 0;

            // Path detection
            if idx < offset + p {
                if let Some(path) = Self::detect_path(&self.query) {
                    cx.emit(PaletteEvent::OpenResult(path));
                }
                return;
            }
            offset += p;

            // Applications
            if idx < offset + a {
                if let Some(app) = self.app_results.get(idx - offset) {
                    let path_str = format!("app://{}", app.path.display());
                    cx.emit(PaletteEvent::OpenResult(PathBuf::from(path_str)));
                }
                return;
            }
            offset += a;

            // Bookmarks
            if idx < offset + b {
                let filtered = self.filtered_bookmarks();
                if let Some(&(_, path)) = filtered.get(idx - offset) {
                    cx.emit(PaletteEvent::OpenResult(path.clone()));
                }
                return;
            }
            offset += b;

            // Storages
            if idx < offset + s {
                let filtered = self.filtered_storages();
                if let Some(&(_, storage)) = filtered.get(idx - offset) {
                    cx.emit(PaletteEvent::OpenResult(storage.mount_point.clone()));
                }
                return;
            }
            offset += s;

            // Search Files dynamic item
            if idx < offset + sf {
                cx.emit(PaletteEvent::ShowInBrowser(self.query.clone()));
                return;
            }
            offset += sf;

            // Filtered actions
            if idx < offset + act {
                let filtered = self.filtered_actions();
                if let Some(&(_, action)) = filtered.get(idx - offset) {
                    if let Some(type_name) = action.path.strip_prefix("type://") {
                        self.drill_into(type_name, action.name, window, cx);
                        return;
                    }
                    if action.path == "apps://" {
                        self.drill_into("apps", action.name, window, cx);
                        return;
                    }
                    cx.emit(PaletteEvent::OpenResult(PathBuf::from(action.path)));
                }
                return;
            }
            offset += act;

            // File results
            if idx < offset + f
                && let Some(result) = self.results.get(idx - offset)
            {
                let path = PathBuf::from(&result.node.path);
                cx.emit(PaletteEvent::OpenResult(path));
            }
            return;
        }

        let bookmark_count = self.bookmarks.len();
        let storage_count = self.storages.len();
        if self.selected_idx < bookmark_count {
            if let Some(path) = self.bookmarks.get(self.selected_idx) {
                cx.emit(PaletteEvent::OpenResult(path.clone()));
            }
        } else if self.selected_idx < bookmark_count + storage_count {
            let si = self.selected_idx - bookmark_count;
            if let Some(storage) = self.storages.get(si) {
                cx.emit(PaletteEvent::OpenResult(storage.mount_point.clone()));
            }
        } else {
            let action_idx = self.selected_idx - bookmark_count - storage_count;
            let visible = Self::visible_default_actions();
            if let Some(action) = visible.get(action_idx) {
                // Handle type:// drill-in internally
                if let Some(type_name) = action.path.strip_prefix("type://") {
                    self.drill_into(type_name, action.name, window, cx);
                    return;
                }
                // Handle apps:// drill-in
                if action.path == "apps://" {
                    self.drill_into("apps", action.name, window, cx);
                    return;
                }
                cx.emit(PaletteEvent::OpenResult(PathBuf::from(action.path)));
            }
        }
    }

    pub(super) fn dismiss(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("palette: dismiss");
        cx.emit(PaletteEvent::Dismiss);
    }

    /// Action label for a result item.
    pub(super) fn action_label_for_result(_is_dir: bool) -> &'static str {
        "Run"
    }

    pub(super) fn section_header(title: &'static str, muted: Hsla) -> Div {
        div()
            .px_3()
            .py_1()
            .text_size(FONT_SIZE_CAPTION)
            .text_color(muted)
            .font_weight(FontWeight::SEMIBOLD)
            .child(title)
    }
}
