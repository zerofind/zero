pub mod navigation;
pub mod progress;
pub mod split;
pub mod titlebar;

use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{h_flex, v_flex, ActiveTheme};

use crate::actions::{
    GoBack, GoForward, GoUp, OpenCommandPalette, PasteFiles, ToggleSidebar, ToggleSplitView,
    ToggleViewMode,
};
use crate::models::{ActiveView, FileClipboard, PaneId, SplitPane, ViewMode};
use crate::services::{SearchEvent, SearchService, ServiceHub};
use crate::session::Settings;
use crate::theme::{self, FONT_SIZE_BODY};
use crate::ui::{Alert, AlertStack, BannerData, BannerKind, ProgressBanner};
use crate::views::{
    AppSidebar, AutomationsView, CleanupView, DedupView, DrivesPopover, EditorView,
    FileBrowserView, FileGridView, OnboardingEvent, OnboardingView, PaletteView, SecureEraseView,
    SettingsView, TodoView,
};

pub struct ZeroApp {
    // Navigation
    pub current_path: PathBuf,
    pub history: Vec<PathBuf>,
    pub history_idx: usize,
    pub active_view: ActiveView,
    pub view_mode: ViewMode,
    pub sidebar_open: bool,

    // Services
    pub services: ServiceHub,

    // Lazy views
    pub file_browser: Option<Entity<FileBrowserView>>,
    pub file_grid: Option<Entity<FileGridView>>,
    pub command_palette: Option<Entity<PaletteView>>,
    pub cleanup: Option<Entity<CleanupView>>,
    pub dedup: Option<Entity<DedupView>>,
    pub editor: Option<Entity<EditorView>>,
    pub todo: Option<Entity<TodoView>>,
    pub settings: Option<Entity<SettingsView>>,
    pub secure_erase: Option<Entity<SecureEraseView>>,
    pub automations: Option<Entity<AutomationsView>>,

    // Overlays
    pub command_palette_open: bool,
    pub drives_popover_open: bool,
    pub drives_popover: Option<Entity<DrivesPopover>>,

    // Split view
    pub split_pane: Option<SplitPane>,
    pub split_browser: Option<Entity<FileBrowserView>>,
    pub active_pane: PaneId,

    // File clipboard
    pub file_clipboard: Option<FileClipboard>,

    // Progress
    pub banner: Option<BannerData>,

    // Alerts / toasts
    pub alerts: Vec<Alert>,

    // Onboarding
    pub onboarding: Option<Entity<OnboardingView>>,

    // Sidebar
    pub sidebar: Option<Entity<AppSidebar>>,

    pub focus_handle: FocusHandle,
    pub _subs: Vec<Subscription>,
}

impl ZeroApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let settings = Settings::load();

        let start_path = settings
            .last_path
            .clone()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"));

        let view_mode = if settings.view_mode == "grid" {
            ViewMode::Grid
        } else {
            ViewMode::List
        };

        let services = ServiceHub::new(cx);

        // Subscribe to search service events for progress banner
        let search_sub = cx.subscribe(&services.search, Self::on_search_event);

        // Show loading banner while index loads
        let banner = Some(BannerData {
            kind: BannerKind::Index,
            message: "Loading search index...".to_string(),
            bytes_done: 0,
            bytes_total: 0,
            files_done: 0,
            files_total: 0,
            phase: Some("This only takes a moment...".to_string()),
            indeterminate: true,
        });

        let mut app = Self {
            current_path: start_path.clone(),
            history: vec![start_path],
            history_idx: 0,
            active_view: ActiveView::FileBrowser,
            view_mode,
            sidebar_open: settings.sidebar_open,
            services,
            file_browser: None,
            file_grid: None,
            command_palette: None,
            command_palette_open: false,
            drives_popover_open: false,
            drives_popover: None,
            split_pane: None,
            split_browser: None,
            active_pane: PaneId::Left,
            file_clipboard: None,
            cleanup: None,
            dedup: None,
            editor: None,
            todo: None,
            settings: None,
            secure_erase: None,
            automations: None,
            banner,
            alerts: Vec::new(),
            onboarding: None,
            sidebar: None,
            focus_handle: cx.focus_handle(),
            _subs: vec![search_sub],
        };

        // Show onboarding on first launch
        if !settings.onboarding_complete && settings.search_roots.is_empty() {
            let onboarding = cx.new(OnboardingView::new);
            let sub = cx.subscribe(&onboarding, Self::on_onboarding_event);
            app._subs.push(sub);
            app.onboarding = Some(onboarding);
        }

        app
    }

    fn on_onboarding_event(
        &mut self,
        _: Entity<OnboardingView>,
        event: &OnboardingEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            OnboardingEvent::Complete(roots) => {
                let mut settings = Settings::load();
                settings.onboarding_complete = true;
                for root in roots {
                    if !settings.search_roots.contains(root) {
                        settings.search_roots.push(root.clone());
                    }
                }
                settings.save();

                // Trigger index rebuild with the new roots
                let root_paths = settings.search_roots.clone();
                self.services.search.update(cx, |svc, cx| {
                    svc.rebuild(root_paths, cx);
                });

                self.onboarding = None;
                cx.notify();
            }
            OnboardingEvent::Skip => {
                let mut settings = Settings::load();
                settings.onboarding_complete = true;
                settings.save();

                self.onboarding = None;
                cx.notify();
            }
        }
    }

    fn on_search_event(
        &mut self,
        _: Entity<SearchService>,
        event: &SearchEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SearchEvent::IndexLoaded => {
                if !self.services.search.read(cx).is_indexing() {
                    self.banner = None;
                    // Start file watcher for live updates
                    self.services.search.update(cx, |svc, cx| {
                        svc.start_watcher(cx);
                    });
                    cx.notify();
                }
            }
            SearchEvent::IndexingStarted => {
                self.banner = Some(BannerData {
                    kind: BannerKind::Index,
                    message: "Indexing...".to_string(),
                    bytes_done: 0,
                    bytes_total: 0,
                    files_done: 0,
                    files_total: 0,
                    phase: Some("Scanning files...".to_string()),
                    indeterminate: true,
                });
                cx.notify();
            }
            SearchEvent::IndexingFinished => {
                self.banner = None;
                // Start file watcher after indexing completes
                self.services.search.update(cx, |svc, cx| {
                    svc.start_watcher(cx);
                });
                cx.notify();
            }
            SearchEvent::IndexCleared => {
                cx.notify();
            }
            SearchEvent::IndexUpdated(_) => {
                cx.notify();
            }
        }
    }

    pub fn open_command_palette(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.command_palette_open = true;
        let palette = self.ensure_command_palette(window, cx);
        palette.update(cx, |view, cx| view.reset(window, cx));
        cx.notify();
    }

    #[allow(dead_code)]
    pub fn push_alert(&mut self, alert: Alert, cx: &mut Context<Self>) {
        self.alerts.push(alert);
        cx.notify();

        // Schedule auto-dismiss
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(4200))
                .await;
            this.update(cx, |app, cx| {
                app.alerts.retain(|a| !a.is_expired());
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn render_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match &self.active_view {
            ActiveView::FileBrowser => match self.view_mode {
                ViewMode::List => {
                    let view = self.ensure_file_browser(window, cx);
                    view.into_any_element()
                }
                ViewMode::Grid => {
                    let view = self.ensure_file_grid(window, cx);
                    view.into_any_element()
                }
            },
            ActiveView::Cleanup => {
                let view = self.ensure_cleanup(window, cx);
                view.into_any_element()
            }
            ActiveView::Dedup => {
                let view = self.ensure_dedup(window, cx);
                view.into_any_element()
            }
            ActiveView::Todo => {
                let view = self.ensure_todo(window, cx);
                view.into_any_element()
            }
            ActiveView::Settings => {
                let view = self.ensure_settings(window, cx);
                view.into_any_element()
            }
            ActiveView::Editor(path) => {
                let path = path.clone();
                let view = self.ensure_editor(path, window, cx);
                view.into_any_element()
            }
            ActiveView::SecureErase => {
                let view = self.ensure_secure_erase(window, cx);
                view.into_any_element()
            }
            ActiveView::Automations => {
                let view = self.ensure_automations(window, cx);
                view.into_any_element()
            }
        }
    }
}

impl Render for ZeroApp {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let sidebar_open = self.sidebar_open;
        let palette_open = self.command_palette_open;
        let banner = self.banner.clone();

        let has_alerts = !self.alerts.is_empty();

        let has_split = self.split_pane.is_some();

        let sidebar_view = self.ensure_sidebar(window, cx);
        let titlebar = self.render_titlebar(window, cx);
        let content = self.render_content(window, cx);
        let split_content = if has_split {
            Some(self.render_split_pane(window, cx))
        } else {
            None
        };
        let palette_view = if palette_open {
            Some(self.ensure_command_palette(window, cx))
        } else {
            None
        };
        let drives_open = self.drives_popover_open;
        let drives_view = if drives_open {
            Some(self.ensure_drives_popover(window, cx))
        } else {
            None
        };

        div()
            .id("app-root")
            .text_size(FONT_SIZE_BODY)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.sidebar_open = !this.sidebar_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenCommandPalette, window, cx| {
                this.open_command_palette(window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoBack, _, cx| {
                this.go_back(cx);
            }))
            .on_action(cx.listener(|this, _: &GoForward, _, cx| {
                this.go_forward(cx);
            }))
            .on_action(cx.listener(|this, _: &GoUp, _, cx| {
                if let Some(parent) = this.current_path.parent().map(|p| p.to_path_buf()) {
                    this.navigate_to(parent, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PasteFiles, _, cx| {
                this.paste_files(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSplitView, window, cx| {
                this.toggle_split_view(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleViewMode, _, cx| {
                this.view_mode = match this.view_mode {
                    ViewMode::List => ViewMode::Grid,
                    ViewMode::Grid => ViewMode::List,
                };
                // Persist view mode
                let mut settings = Settings::load();
                settings.view_mode = match this.view_mode {
                    ViewMode::List => "list".to_string(),
                    ViewMode::Grid => "grid".to_string(),
                };
                settings.save();
                // Reset cached grid/browser so it rebuilds
                this.file_grid = None;
                this.file_browser = None;
                cx.notify();
            }))
            .size_full()
            .bg(theme::content_bg(cx))
            .child(
                h_flex()
                    .size_full()
                    .when(sidebar_open, |el| el.child(sidebar_view))
                    .child(
                        v_flex()
                            .size_full()
                            .bg(theme::content_bg(cx))
                            .when(sidebar_open, |el| el.rounded_tl(px(5.0)))
                            .overflow_hidden()
                            .child(titlebar)
                            .child(
                                div()
                                    .w_full()
                                    .h(px(2.0))
                                    .bg(theme::selection_color()),
                            )
                            .when_some(banner, |el, data| {
                                el.child(div().px_3().py_1().child(ProgressBanner::new(data)))
                            })
                            .child(
                                h_flex()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(content)
                                    .when_some(split_content, |el, right| {
                                        el.child(
                                            div()
                                                .w(px(1.0))
                                                .h_full()
                                                .bg(cx.theme().border),
                                        )
                                        .child(right)
                                    }),
                            ),
                    ),
            )
            // Command palette overlay
            .when_some(palette_view, |el, palette| {
                el.child(
                    div()
                        .id("palette-overlay")
                        .absolute()
                        .size_full()
                        .top_0()
                        .left_0()
                        .bg(theme::overlay_backdrop(cx))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.command_palette_open = false;
                                this.focus_content(window, cx);
                                cx.notify();
                            }),
                        )
                        .child(
                            div()
                                .on_mouse_down(MouseButton::Left, |_, _, _| {
                                    // Stop propagation — don't dismiss on panel click
                                })
                                .child(palette),
                        ),
                )
            })
            // Drives popover overlay
            .when_some(drives_view, |el, drives| {
                el.child(
                    div()
                        .id("drives-overlay")
                        .absolute()
                        .size_full()
                        .top_0()
                        .left_0()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.drives_popover_open = false;
                                this.focus_content(window, cx);
                                cx.notify();
                            }),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(44.0))
                                .right(px(52.0))
                                .on_mouse_down(MouseButton::Left, |_, _, _| {})
                                .child(drives),
                        ),
                )
            })
            // Alert toasts
            .when(has_alerts, |el| {
                el.child(AlertStack::render(&self.alerts, cx))
            })
    }
}
