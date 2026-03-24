mod general;
pub mod helpers;
mod search;

use std::sync::Arc;
use std::time::Duration;

use gpui::*;
use gpui_component::{
    IndexPath, Sizable as _, h_flex,
    input::InputState,
    scroll::ScrollableElement as _,
    select::{SelectEvent, SelectState},
    tab::TabBar,
    v_flex,
};

use scanner::CrawlProgress;

use gpui_component::theme::ThemeMode;

use crate::services::{McpService, SearchEvent, SearchService};
use crate::session::Settings;
use crate::theme::{self, FONT_SIZE_BODY};

// -- Events emitted to the parent ZeroApp ------------------------------------

pub enum SettingsEvent {
    IndexRebuildStarted(Arc<CrawlProgress>),
    IndexRebuildFinished,
}

impl EventEmitter<SettingsEvent> for SettingsView {}

// -- Settings tabs -----------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum SettingsTab {
    General,
    Search,
}

// -- View state --------------------------------------------------------------

const MODE_LABELS: &[&str] = &["Light", "Dark", "System"];

pub struct SettingsView {
    pub(super) active_tab: SettingsTab,
    pub(super) settings: Settings,
    pub(super) search: Entity<SearchService>,
    pub(super) mcp: Entity<McpService>,
    pub(super) mode_select: Entity<SelectState<Vec<String>>>,
    pub(super) light_theme_select: Entity<SelectState<Vec<String>>>,
    pub(super) dark_theme_select: Entity<SelectState<Vec<String>>>,

    // Search tab
    pub(super) adding_root: bool,
    pub(super) root_input: Entity<InputState>,
    pub(super) root_error: Option<String>,
    pub(super) reindexing_root: Option<usize>,
    pub(super) reindexing_all: bool,

    // Danger zone confirmations
    pub(super) confirm_clear_index: bool,
    pub(super) confirm_reset: bool,

    // Async state
    pub(super) rebuilding: bool,
    pub(super) rebuild_files: u64,

    #[allow(dead_code)]
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(
        search: Entity<SearchService>,
        mcp: Entity<McpService>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let root_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("~/Documents, ~/Photos, ..."));

        let settings = Settings::load();

        // Mode selector (Light / Dark / System)
        let mode_items: Vec<String> = MODE_LABELS
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        let mode_idx = match settings.theme_mode.as_str() {
            "light" => 0,
            "dark" => 1,
            _ => 2, // system
        };
        let mode_select =
            cx.new(|cx| SelectState::new(mode_items, Some(IndexPath::new(mode_idx)), window, cx));

        let mode_sub = cx.subscribe(
            &mode_select,
            |this, _, event: &SelectEvent<Vec<String>>, cx| {
                if let SelectEvent::Confirm(Some(label)) = event {
                    let mode = match label.as_str() {
                        "Light" => "light",
                        "Dark" => "dark",
                        _ => "system",
                    };
                    this.set_mode(mode, cx);
                }
            },
        );

        // Light theme selector
        let light_items = theme::theme_names_for_mode(ThemeMode::Light, cx);
        let light_idx = light_items
            .iter()
            .position(|n| n == &settings.light_theme)
            .unwrap_or(0);
        let light_theme_select =
            cx.new(|cx| SelectState::new(light_items, Some(IndexPath::new(light_idx)), window, cx));

        let light_sub = cx.subscribe(
            &light_theme_select,
            |this, _, event: &SelectEvent<Vec<String>>, cx| {
                if let SelectEvent::Confirm(Some(name)) = event {
                    this.set_light_theme(name, cx);
                }
            },
        );

        // Dark theme selector
        let dark_items = theme::theme_names_for_mode(ThemeMode::Dark, cx);
        let dark_idx = dark_items
            .iter()
            .position(|n| n == &settings.dark_theme)
            .unwrap_or(0);
        let dark_theme_select =
            cx.new(|cx| SelectState::new(dark_items, Some(IndexPath::new(dark_idx)), window, cx));

        let dark_sub = cx.subscribe(
            &dark_theme_select,
            |this, _, event: &SelectEvent<Vec<String>>, cx| {
                if let SelectEvent::Confirm(Some(name)) = event {
                    this.set_dark_theme(name, cx);
                }
            },
        );

        // Re-render when index state changes (loaded, finished, etc.)
        let search_sub = cx.subscribe(&search, |this, _, event: &SearchEvent, cx| match event {
            SearchEvent::IndexingFinished | SearchEvent::IndexLoaded => {
                this.reindexing_all = false;
                this.reindexing_root = None;
                cx.notify();
            }
            _ => cx.notify(),
        });

        Self {
            active_tab: SettingsTab::General,
            settings,
            search,
            mcp,
            mode_select,
            light_theme_select,
            dark_theme_select,
            adding_root: false,
            root_input,
            root_error: None,
            reindexing_root: None,
            reindexing_all: false,
            confirm_clear_index: false,
            confirm_reset: false,
            rebuilding: false,
            rebuild_files: 0,
            focus_handle: cx.focus_handle(),
            _subs: vec![search_sub, mode_sub, light_sub, dark_sub],
        }
    }

    // -- Actions -------------------------------------------------------------

    pub(super) fn set_mode(&mut self, mode: &str, cx: &mut Context<Self>) {
        tracing::debug!(mode, "settings: set mode");
        self.settings.theme_mode = mode.to_string();
        self.apply_and_save(cx);
    }

    pub(super) fn set_light_theme(&mut self, name: &str, cx: &mut Context<Self>) {
        tracing::debug!(name, "settings: set light theme");
        self.settings.light_theme = name.to_string();
        self.apply_and_save(cx);
    }

    pub(super) fn set_dark_theme(&mut self, name: &str, cx: &mut Context<Self>) {
        tracing::debug!(name, "settings: set dark theme");
        self.settings.dark_theme = name.to_string();
        self.apply_and_save(cx);
    }

    fn apply_and_save(&mut self, cx: &mut Context<Self>) {
        theme::apply_dual_themes(
            &self.settings.light_theme,
            &self.settings.dark_theme,
            &self.settings.theme_mode,
            cx,
        );
        self.settings.save();
        cx.notify();
    }

    pub(super) fn rebuild_index(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("settings: rebuild index");
        if self.rebuilding {
            return;
        }
        self.rebuilding = true;
        self.rebuild_files = 0;
        cx.notify();

        let roots = self.settings.search_roots.clone();
        let progress = self.search.update(cx, |svc, cx| svc.rebuild(&roots, cx));

        cx.emit(SettingsEvent::IndexRebuildStarted(progress.clone()));

        // Start 150ms poll loop for live file count
        let poll_progress = progress;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;

                let still_running = this
                    .update(cx, |view, cx| {
                        if !view.rebuilding {
                            return false;
                        }
                        view.rebuild_files = poll_progress.files() as u64;

                        // Check if service is done indexing
                        if !view.search.read(cx).is_indexing() {
                            view.rebuilding = false;
                            cx.emit(SettingsEvent::IndexRebuildFinished);
                            cx.notify();
                            return false;
                        }

                        cx.notify();
                        true
                    })
                    .unwrap_or(false);

                if !still_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn reindex_root(&mut self, idx: usize, cx: &mut Context<Self>) {
        tracing::debug!(idx, "settings: reindex root");
        if self.reindexing_root.is_some()
            || self.reindexing_all
            || idx >= self.settings.search_roots.len()
        {
            return;
        }
        self.reindexing_root = Some(idx);
        cx.notify();

        let Some(path) = self.settings.search_roots.get(idx).cloned() else {
            return;
        };
        let path_str = path.to_string_lossy().to_string();

        self.search.update(cx, |svc, cx| {
            svc.remove_root(&path_str, cx);
            svc.add_root(&path_str, cx);
        });

        self.reindexing_root = None;
        cx.notify();
    }

    pub(super) fn reindex_all(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("settings: reindex all");
        if self.reindexing_all || self.settings.search_roots.is_empty() {
            return;
        }
        self.reindexing_all = true;
        cx.notify();

        for root in &self.settings.search_roots {
            let path_str = root.to_string_lossy().to_string();
            self.search.update(cx, |svc, cx| {
                svc.remove_root(&path_str, cx);
                svc.add_root(&path_str, cx);
            });
        }

        self.reindexing_all = false;
        cx.notify();
    }

    pub(super) fn request_clear_index(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("settings: request clear index");
        self.confirm_clear_index = true;
        cx.notify();
    }

    pub(super) fn confirm_clear_index(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("settings: confirm clear index");
        self.confirm_clear_index = false;
        self.search
            .update(cx, super::super::services::search::SearchService::clear);
        cx.notify();
    }

    pub(super) fn request_reset_settings(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("settings: request reset");
        self.confirm_reset = true;
        cx.notify();
    }

    pub(super) fn confirm_reset_settings(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("settings: confirm reset");
        self.confirm_reset = false;
        self.settings = Settings::default();
        self.settings.save();

        theme::apply_dual_themes(
            &self.settings.light_theme,
            &self.settings.dark_theme,
            &self.settings.theme_mode,
            cx,
        );

        cx.notify();
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = match self.active_tab {
            SettingsTab::General => 0,
            SettingsTab::Search => 1,
        };

        TabBar::new("settings-tabs")
            .segmented()
            .small()
            .selected_index(selected)
            .child("General")
            .child("Search")
            .on_click(cx.listener(|this, idx: &usize, _, cx| {
                this.active_tab = match idx {
                    0 => SettingsTab::General,
                    _ => SettingsTab::Search,
                };
                cx.notify();
            }))
    }
}

// -- Render ------------------------------------------------------------------

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_tab = self.active_tab;
        let tabs = self.render_tabs(cx).into_any_element();
        let content = match active_tab {
            SettingsTab::General => self.render_general(window, cx).into_any_element(),
            SettingsTab::Search => self.render_search(cx).into_any_element(),
        };

        v_flex()
            .size_full()
            .bg(theme::content_bg(cx))
            .p_4()
            .gap_4()
            // Title + tabs
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(FONT_SIZE_BODY)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Settings"),
                    )
                    .child(tabs),
            )
            // Scrollable tab content
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(content),
            )
    }
}
