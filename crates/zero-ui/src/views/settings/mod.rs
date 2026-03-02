mod general;
pub mod helpers;
mod search;

use std::sync::Arc;
use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::InputState,
    v_flex, Sizable as _,
};
use gpui_component::theme::{Theme, ThemeMode};

use zero::scanner::CrawlProgress;

use crate::services::SearchService;
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

pub struct SettingsView {
    pub(super) active_tab: SettingsTab,
    pub(super) settings: Settings,
    pub(super) search: Entity<SearchService>,

    // Search tab
    pub(super) adding_root: bool,
    pub(super) root_input: Entity<InputState>,
    pub(super) root_error: Option<String>,
    pub(super) indexed_file_count: u64,
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
}

impl SettingsView {
    pub fn new(
        search: Entity<SearchService>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let root_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("~/Documents, ~/Photos, ...")
        });

        let file_count = search.read(cx).file_count();

        Self {
            active_tab: SettingsTab::General,
            settings: Settings::load(),
            search,
            adding_root: false,
            root_input,
            root_error: None,
            indexed_file_count: file_count,
            reindexing_root: None,
            reindexing_all: false,
            confirm_clear_index: false,
            confirm_reset: false,
            rebuilding: false,
            rebuild_files: 0,
            focus_handle: cx.focus_handle(),
        }
    }

    // -- Actions -------------------------------------------------------------

    pub(super) fn set_theme(&mut self, mode: &str, cx: &mut Context<Self>) {
        let new_mode = if mode == "system" {
            Self::detect_system_theme()
        } else if mode == "dark" {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };

        Theme::change(new_mode, None, cx);

        self.settings.theme_mode = mode.to_string();
        self.settings.save();
        cx.notify();
    }

    fn detect_system_theme() -> ThemeMode {
        // On macOS, check system dark mode preference
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("defaults")
                .args(["read", "-g", "AppleInterfaceStyle"])
                .output();
            if let Ok(out) = output {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if stdout.trim().eq_ignore_ascii_case("dark") {
                    return ThemeMode::Dark;
                }
            }
            ThemeMode::Light
        }
        #[cfg(not(target_os = "macos"))]
        {
            ThemeMode::Dark
        }
    }

    pub(super) fn rebuild_index(&mut self, cx: &mut Context<Self>) {
        if self.rebuilding {
            return;
        }
        self.rebuilding = true;
        self.rebuild_files = 0;
        cx.notify();

        let roots = self.settings.search_roots.clone();
        let progress = self.search.update(cx, |svc, cx| svc.rebuild(roots, cx));

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
        if self.reindexing_root.is_some() || self.reindexing_all || idx >= self.settings.search_roots.len() {
            return;
        }
        self.reindexing_root = Some(idx);
        cx.notify();

        let path = self.settings.search_roots[idx].clone();
        let path_str = path.to_string_lossy().to_string();

        self.search.update(cx, |svc, cx| {
            svc.remove_root(&path_str, cx);
            svc.add_root(&path_str, cx);
        });

        self.reindexing_root = None;
        self.indexed_file_count = self.search.read(cx).file_count();
        cx.notify();
    }

    pub(super) fn reindex_all(&mut self, cx: &mut Context<Self>) {
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
        self.indexed_file_count = self.search.read(cx).file_count();
        cx.notify();
    }

    pub(super) fn request_clear_index(&mut self, cx: &mut Context<Self>) {
        self.confirm_clear_index = true;
        cx.notify();
    }

    pub(super) fn confirm_clear_index(&mut self, cx: &mut Context<Self>) {
        self.confirm_clear_index = false;
        self.search.update(cx, |svc, cx| svc.clear(cx));
        cx.notify();
    }

    pub(super) fn request_reset_settings(&mut self, cx: &mut Context<Self>) {
        self.confirm_reset = true;
        cx.notify();
    }

    pub(super) fn confirm_reset_settings(&mut self, cx: &mut Context<Self>) {
        self.confirm_reset = false;
        self.settings = Settings::default();
        self.settings.save();

        let mode = if self.settings.theme_mode == "system" {
            Self::detect_system_theme()
        } else if self.settings.theme_mode == "dark" {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        Theme::change(mode, None, cx);

        cx.notify();
    }

    fn render_tab_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_1()
            .child(
                Button::new("tab-general")
                    .label("General")
                    .compact()
                    .small()
                    .when(self.active_tab == SettingsTab::General, |el| el.primary())
                    .when(self.active_tab != SettingsTab::General, |el| el.ghost())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.active_tab = SettingsTab::General;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("tab-search")
                    .label("Search")
                    .compact()
                    .small()
                    .when(self.active_tab == SettingsTab::Search, |el| el.primary())
                    .when(self.active_tab != SettingsTab::Search, |el| el.ghost())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.active_tab = SettingsTab::Search;
                        cx.notify();
                    })),
            )
    }
}

// -- Render ------------------------------------------------------------------

impl Render for SettingsView {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active_tab = self.active_tab;
        let tab_buttons = self.render_tab_buttons(cx).into_any_element();
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
                    .child(tab_buttons),
            )
            // Tab content
            .child(content)
    }
}
