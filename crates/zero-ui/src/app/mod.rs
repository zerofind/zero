pub mod events;
pub mod navigation;
pub mod progress;
pub mod routing;
pub mod split;
pub mod titlebar;
pub mod views;
pub mod workspace;

use std::path::PathBuf;
use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};

use zero::scanner::CrawlProgress;

use crate::actions::{
    About, CheckForUpdates, CloseWindow, GoApplications, GoBack, GoDesktop, GoDocuments,
    GoDownloads, GoForward, GoHome, GoUp, Minimize, OpenAutomations, OpenCleanup,
    OpenCommandPalette, OpenDuplicates, OpenSecureErase, OpenSettings, PasteFiles,
    SwitchWorkspace1, SwitchWorkspace2, SwitchWorkspace3, SwitchWorkspace4, SwitchWorkspace5,
    SwitchWorkspace6, SwitchWorkspace7, SwitchWorkspace8, SwitchWorkspace9, ToggleAsk,
    ToggleSidebar, ToggleSplitView, ToggleTerminal, ToggleToolbar, ToggleViewMode, Zoom,
};
use crate::models::{ActiveView, FileClipboard, PaneId, SplitPane, ViewMode};
use crate::permissions;
use crate::platform::traffic_lights::TrafficLights;
use crate::services::{SearchEvent, SearchService, ServiceHub};
use crate::session::Settings;
use crate::theme::{self, CONTENT_INSET, FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS_LG};
use crate::ui::{Alert, AlertStack, BannerData, BannerKind, ProgressBanner};
use crate::views::{
    AppSidebar, AskView, AutomationsView, CleanupView, DataTableView, DedupView, DrivesPopover,
    EditorView, FdaOnboardingEvent, FdaOnboardingView, FileBrowserView, FileGridView,
    OnboardingEvent, OnboardingView, PaletteView, SecureEraseView, SettingsView, TerminalView,
    TodoView,
};

#[derive(Clone)]
struct ResizeTerminalDrag;

impl Render for ResizeTerminalDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().cursor_row_resize()
    }
}

#[derive(Clone)]
struct ResizeAskDrag;

impl Render for ResizeAskDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().cursor_col_resize()
    }
}

pub struct ZeroApp {
    // Navigation
    pub current_path: PathBuf,
    pub active_view: ActiveView,
    pub nav_stack: Vec<ActiveView>,
    pub nav_idx: usize,
    pub view_mode: ViewMode,
    pub sidebar_open: bool,
    pub toolbar_visible: bool,

    // Services
    pub services: ServiceHub,

    // Lazy views
    pub file_browser: Option<Entity<FileBrowserView>>,
    pub file_grid: Option<Entity<FileGridView>>,
    pub command_palette: Option<Entity<PaletteView>>,
    pub cleanup: Option<Entity<CleanupView>>,
    pub dedup: Option<Entity<DedupView>>,
    pub editor: Option<Entity<EditorView>>,
    pub data_table: Option<Entity<DataTableView>>,
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

    // Terminal panel
    pub terminal: Option<Entity<TerminalView>>,
    pub terminal_open: bool,
    pub terminal_height: Pixels,

    // Ask panel
    pub ask: Option<Entity<AskView>>,
    pub ask_open: bool,
    pub ask_width: Pixels,

    // File clipboard
    pub file_clipboard: Option<FileClipboard>,

    // Progress
    pub banner: Option<BannerData>,
    pub active_progress: Option<Arc<CrawlProgress>>,

    // Alerts / toasts
    pub alerts: Vec<Alert>,

    // Onboarding
    pub fda_onboarding: Option<Entity<FdaOnboardingView>>,
    pub onboarding: Option<Entity<OnboardingView>>,

    // Sidebar
    pub sidebar: Option<Entity<AppSidebar>>,

    // Workspaces
    pub workspace_snapshots: Vec<Option<crate::workspace::WorkspaceSnapshot>>,
    pub active_workspace_idx: usize,

    pub focus_handle: FocusHandle,
    pub focus_redirect_registered: bool,
    pub _subs: Vec<Subscription>,
}

impl ZeroApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let settings = Settings::load();

        let active_ws = settings.active_ws();
        let start_path = active_ws
            .last_path
            .clone()
            .or_else(|| active_ws.pinned_bookmarks.first().cloned())
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"));

        let view_mode = if active_ws.view_mode == "grid" {
            ViewMode::Grid
        } else {
            ViewMode::List
        };

        let workspace_count = settings.workspaces.len();
        let active_workspace_idx = settings.active_workspace;

        let has_fda = permissions::has_full_disk_access();

        // Only initialize services if we have Full Disk Access — avoids triggering
        // TCC permission prompts before the user has granted FDA.
        let (services, subs) = if has_fda {
            let services = ServiceHub::new(cx);
            let search_sub = cx.subscribe(&services.search, Self::on_search_event);
            (services, vec![search_sub])
        } else {
            // Create services without triggering any file access
            let services = ServiceHub::new_deferred(cx);
            (services, Vec::new())
        };

        let mut app = Self {
            current_path: start_path.clone(),
            active_view: ActiveView::FileBrowser(start_path.clone()),
            nav_stack: vec![ActiveView::FileBrowser(start_path)],
            nav_idx: 0,
            view_mode,
            sidebar_open: settings.sidebar_open,
            toolbar_visible: settings.toolbar_visible,
            services,
            file_browser: None,
            file_grid: None,
            command_palette: None,
            command_palette_open: false,
            drives_popover_open: false,
            drives_popover: None,
            terminal: None,
            terminal_open: false,
            terminal_height: px(300.0),
            ask: None,
            ask_open: false,
            ask_width: px(360.0),
            split_pane: None,
            split_browser: None,
            active_pane: PaneId::Left,
            file_clipboard: None,
            cleanup: None,
            dedup: None,
            editor: None,
            data_table: None,
            todo: None,
            settings: None,
            secure_erase: None,
            automations: None,
            banner: None,
            active_progress: None,
            alerts: Vec::new(),
            fda_onboarding: None,
            onboarding: None,
            sidebar: None,
            workspace_snapshots: vec![None; workspace_count],
            active_workspace_idx,
            focus_handle: cx.focus_handle(),
            focus_redirect_registered: false,
            _subs: subs,
        };

        if !has_fda {
            // Show FDA onboarding — blocks everything until permission is granted
            let fda_view = cx.new(FdaOnboardingView::new);
            let sub = cx.subscribe(&fda_view, Self::on_fda_onboarding_event);
            app._subs.push(sub);
            app.fda_onboarding = Some(fda_view);
        } else if !settings.onboarding_complete && settings.search_roots.is_empty() {
            // Show folder selection onboarding
            let onboarding = cx.new(OnboardingView::new);
            let sub = cx.subscribe(&onboarding, Self::on_onboarding_event);
            app._subs.push(sub);
            app.onboarding = Some(onboarding);
        }

        app
    }

    fn on_fda_onboarding_event(
        &mut self,
        _: Entity<FdaOnboardingView>,
        event: &FdaOnboardingEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            FdaOnboardingEvent::Granted => {
                tracing::info!("Full Disk Access granted — initializing services");
                self.fda_onboarding = None;

                // Now safe to initialize services and start indexing
                self.services.init(cx);
                let search_sub = cx.subscribe(&self.services.search, Self::on_search_event);
                self._subs.push(search_sub);

                // Show folder onboarding if first launch
                let settings = Settings::load();
                if !settings.onboarding_complete && settings.search_roots.is_empty() {
                    let onboarding = cx.new(OnboardingView::new);
                    let sub = cx.subscribe(&onboarding, Self::on_onboarding_event);
                    self._subs.push(sub);
                    self.onboarding = Some(onboarding);
                }

                cx.notify();
            }
        }
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
                let is_indexing = self.services.search.read(cx).is_indexing();
                tracing::debug!(is_indexing, "event: IndexLoaded");
                if !is_indexing {
                    self.banner = None;
                    // Start file watcher for live updates
                    self.services.search.update(cx, |svc, cx| {
                        svc.start_watcher(cx);
                    });

                    // Auto-start MCP server if enabled in settings
                    let settings = Settings::load();
                    if settings.mcp_enabled {
                        let manager = self.services.search.read(cx).clone_manager();
                        let port = settings.mcp_port;
                        self.services.mcp.update(cx, |mcp, cx| {
                            mcp.start(manager, port, cx);
                        });
                    }

                    // Provide the index to LLM tools (agent is independent)
                    let llm_manager = self.services.search.read(cx).clone_manager();
                    self.services.llm.read(cx).set_index(llm_manager);

                    cx.notify();
                }
            }
            SearchEvent::RootLoaded { root, file_count } => {
                tracing::info!(root = %root, files = file_count, "root loaded");
                let loaded = self.services.search.read(cx).indexes_count();
                let total = self.services.search.read(cx).roots_count();

                if let Some(banner) = &mut self.banner {
                    banner.phase = Some(format!("{}/{} locations loaded", loaded, total));
                }
                cx.notify();
            }
            SearchEvent::IndexingStarted { progress, path } => {
                tracing::debug!(path = %path, "event: IndexingStarted");
                self.active_progress = Some(progress.clone());

                // Abbreviate home dir prefix with ~
                let display_path = dirs::home_dir()
                    .and_then(|h| {
                        let h = h.to_string_lossy();
                        path.strip_prefix(h.as_ref()).map(|rest| {
                            if rest.is_empty() {
                                "~".to_string()
                            } else {
                                format!("~{rest}")
                            }
                        })
                    })
                    .unwrap_or_else(|| path.clone());

                let cancel_progress = progress.clone();
                let on_cancel: Arc<dyn Fn() + Send + Sync> =
                    Arc::new(move || cancel_progress.cancel());

                self.start_crawl_progress_polling(
                    BannerKind::Index,
                    format!("Indexing {display_path}"),
                    progress.clone(),
                    Some(on_cancel),
                    cx,
                );
            }
            SearchEvent::IndexingFinished => {
                tracing::debug!("event: IndexingFinished");
                self.active_progress = None;
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
            SearchEvent::IndexUpdated(_) | SearchEvent::StoragesChanged => {
                cx.notify();
            }
        }
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

    /// Re-check git dirty state for sidebar bookmarks (throttled to 5s).
    fn refresh_sidebar_git(&mut self, cx: &mut Context<Self>) {
        let settings = crate::session::Settings::load();
        let ws = settings.active_ws();
        let mut paths = ws.pinned_bookmarks.clone();
        paths.extend(ws.regular_bookmarks.iter().cloned());
        if paths.is_empty() {
            return;
        }
        self.services.git.update(cx, |svc, cx| {
            svc.refresh_if_stale(paths, std::time::Duration::from_secs(5), cx);
        });
    }

    fn render_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match &self.active_view {
            ActiveView::FileBrowser(_) => match self.view_mode {
                ViewMode::List => {
                    let view = self.ensure_file_browser(window, cx);
                    view.into_any_element()
                }
                ViewMode::Grid => {
                    let view = self.ensure_file_grid(window, cx);
                    view.into_any_element()
                }
            },
            ActiveView::Cleanup | ActiveView::CleanupDetail(_) => {
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
            ActiveView::DataTable(path) => {
                let path = path.clone();
                let view = self.ensure_data_table(path, window, cx);
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // FDA onboarding takes priority over everything
        if let Some(fda) = &self.fda_onboarding {
            return fda.clone().into_any_element();
        }

        let sidebar_open = self.sidebar_open;
        // Toolbar must be visible when sidebar is hidden — otherwise there's
        // no navigation UI and traffic lights overlap the content.
        let toolbar_visible = self.toolbar_visible || !sidebar_open;
        let palette_open = self.command_palette_open;

        let has_alerts = !self.alerts.is_empty();

        // Banner priority: content banner (editor/data table) > progress banner
        let content_banner: Option<AnyElement> =
            if matches!(self.active_view, ActiveView::Editor(_)) {
                self.editor.as_ref().map(|editor| {
                    let editor = editor.read(cx);
                    let name = editor.file_name();
                    let modified = editor.is_modified();
                    let saving = editor.is_saving();
                    let path_str = editor.path_str();
                    let muted = cx.theme().muted_foreground;

                    h_flex()
                        .w_full()
                        .px_4()
                        .py_2()
                        .items_center()
                        .gap_3()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .bg(theme::banner_bg(cx))
                        .child(
                            h_flex()
                                .flex_1()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .text_size(FONT_SIZE_BODY)
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(SharedString::from(name)),
                                )
                                .when(modified, |el| {
                                    el.child(
                                        div()
                                            .w(px(8.0))
                                            .h(px(8.0))
                                            .rounded(px(4.0))
                                            .bg(theme::brand_color(cx)),
                                    )
                                }),
                        )
                        .when(modified || saving, |el| {
                            el.child(
                                Button::new("save-editor")
                                    .compact()
                                    .small()
                                    .primary()
                                    .label(if saving { "Saving..." } else { "Save" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(editor) = &this.editor {
                                            editor.update(cx, |e, cx| e.save(cx));
                                        }
                                    })),
                            )
                        })
                        .child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .child(SharedString::from(path_str)),
                        )
                        .into_any_element()
                })
            } else if matches!(self.active_view, ActiveView::DataTable(_)) {
                self.data_table.as_ref().map(|dt| {
                    let dt = dt.read(cx);
                    let name = dt.file_name();
                    let info = dt.info().unwrap_or("").to_string();
                    let path_str = dt.path_str();
                    let muted = cx.theme().muted_foreground;

                    h_flex()
                        .w_full()
                        .px_4()
                        .py_2()
                        .items_center()
                        .gap_3()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .bg(theme::banner_bg(cx))
                        .child(
                            div()
                                .text_size(FONT_SIZE_BODY)
                                .font_weight(FontWeight::MEDIUM)
                                .child(SharedString::from(name)),
                        )
                        .when(!info.is_empty(), |el| {
                            el.child(
                                div()
                                    .text_size(FONT_SIZE_CAPTION)
                                    .text_color(muted)
                                    .child(SharedString::from(info)),
                            )
                        })
                        .child(
                            div()
                                .flex_1()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .min_w_0()
                                .child(SharedString::from(path_str)),
                        )
                        .into_any_element()
                })
            } else {
                None
            };

        let banner = if content_banner.is_none() {
            self.banner.clone()
        } else {
            None
        };

        let has_split = self.split_pane.is_some();

        let sidebar_view = self.ensure_sidebar(window, cx);
        let titlebar = if toolbar_visible {
            Some(self.render_titlebar(window, cx))
        } else {
            None
        };
        let content = self.render_content(window, cx);
        let split_content = if has_split {
            Some(self.render_split_pane(window, cx))
        } else {
            None
        };
        let terminal_open = self.terminal_open;
        let terminal_height = self.terminal_height;
        let terminal_view = if terminal_open {
            self.terminal.clone()
        } else {
            None
        };
        let ask_open = self.ask_open;
        let ask_width = self.ask_width;
        let ask_view = if ask_open { self.ask.clone() } else { None };

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

        // Register focus redirector once: when the app root gets focus
        // (sidebar click, window re-activation, etc.), redirect to content.
        if !self.focus_redirect_registered {
            let sub = cx.on_focus(&self.focus_handle, window, |this, window, cx| {
                this.focus_content(window, cx);
                this.refresh_sidebar_git(cx);
            });
            self._subs.push(sub);
            self.focus_redirect_registered = true;
        }

        div()
            .id("app-root")
            .text_size(FONT_SIZE_BODY)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|_, _: &About, window, cx| {
                let version = env!("CARGO_PKG_VERSION");
                let debug = if cfg!(debug_assertions) { " (debug)" } else { "" };
                let message = format!("Zero {version}{debug}");
                let detail = format!(
                    "A privacy-first Finder & Spotlight replacement\n\n\
                     Platform: {} {}\n\
                     Rust edition: 2024",
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                );
                let prompt =
                    window.prompt(PromptLevel::Info, &message, Some(&detail), &["Copy", "OK"], cx);
                cx.spawn(async move |_, cx| {
                    if let Ok(0) = prompt.await {
                        let text = format!("{message}\n{detail}");
                        cx.update(|cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                        })
                        .ok();
                    }
                })
                .detach();
            }))
            .on_action(cx.listener(|_, _: &CheckForUpdates, _, cx| {
                cx.spawn(async move |_, cx| {
                    let status = smol::unblock(zero::updater::check_latest).await;
                    let (msg, detail, level) = match &status {
                        Ok(zero::updater::UpdateStatus::UpToDate) => {
                            let v = env!("CARGO_PKG_VERSION");
                            (format!("You're up to date! (v{v})"), None, PromptLevel::Info)
                        }
                        Ok(zero::updater::UpdateStatus::Available { version }) => (
                            format!("Update available: v{version}"),
                            Some("Run `zero update` in your terminal to install.".to_string()),
                            PromptLevel::Info,
                        ),
                        Err(e) => (
                            "Update check failed".to_string(),
                            Some(e.to_string()),
                            PromptLevel::Warning,
                        ),
                    };
                    cx.update(|cx| {
                        if let Some(window) = cx.active_window() {
                            window
                                .update(cx, |_, window, cx| {
                                    drop(window.prompt(
                                        level,
                                        &msg,
                                        detail.as_deref(),
                                        &["OK"],
                                        cx,
                                    ));
                                })
                                .ok();
                        }
                    })
                    .ok();
                })
                .detach();
            }))
            .on_action(cx.listener(|this, _: &ToggleToolbar, _, cx| {
                // Can't hide toolbar when sidebar is already hidden
                if !this.sidebar_open && this.toolbar_visible {
                    return;
                }
                tracing::debug!(visible = !this.toolbar_visible, "action: toggle toolbar");
                this.toolbar_visible = !this.toolbar_visible;
                let mut settings = Settings::load();
                settings.toolbar_visible = this.toolbar_visible;
                settings.save();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                tracing::debug!(open = !this.sidebar_open, "action: toggle sidebar");
                this.sidebar_open = !this.sidebar_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenCommandPalette, window, cx| {
                this.open_command_palette(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                tracing::debug!("action: open settings");
                this.push_view(ActiveView::Settings, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoBack, window, cx| {
                this.go_back(window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoForward, window, cx| {
                this.go_forward(window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoUp, window, cx| {
                if let Some(parent) = this.current_path.parent().map(|p| p.to_path_buf()) {
                    this.navigate_to(parent, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PasteFiles, _, cx| {
                tracing::debug!("action: paste files");
                this.paste_files(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSplitView, window, cx| {
                tracing::debug!("action: toggle split view");
                this.toggle_split_view(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleAsk, window, cx| {
                tracing::debug!(open = !this.ask_open, "action: toggle ask");
                this.ask_open = !this.ask_open;
                if this.ask_open {
                    this.ensure_ask(window, cx);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleTerminal, window, cx| {
                tracing::debug!(open = !this.terminal_open, "action: toggle terminal");
                this.terminal_open = !this.terminal_open;
                if this.terminal_open {
                    this.ensure_terminal(window, cx);
                    // Focus the terminal
                    if let Some(tv) = &this.terminal {
                        tv.read(cx).focus_handle.focus(window);
                    }
                } else {
                    this.focus_content(window, cx);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleViewMode, _, cx| {
                let new_mode = match this.view_mode {
                    ViewMode::List => "grid",
                    ViewMode::Grid => "list",
                };
                tracing::debug!(mode = new_mode, "action: toggle view mode");
                this.view_mode = match this.view_mode {
                    ViewMode::List => ViewMode::Grid,
                    ViewMode::Grid => ViewMode::List,
                };
                // Persist view mode to active workspace
                let mut settings = Settings::load();
                settings.active_ws_mut().view_mode = match this.view_mode {
                    ViewMode::List => "list".to_string(),
                    ViewMode::Grid => "grid".to_string(),
                };
                settings.save();
                // Reset cached grid/browser so it rebuilds
                this.file_grid = None;
                this.file_browser = None;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SwitchWorkspace1, window, cx| {
                this.switch_workspace(0, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchWorkspace2, window, cx| {
                this.switch_workspace(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchWorkspace3, window, cx| {
                this.switch_workspace(2, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchWorkspace4, window, cx| {
                this.switch_workspace(3, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchWorkspace5, window, cx| {
                this.switch_workspace(4, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchWorkspace6, window, cx| {
                this.switch_workspace(5, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchWorkspace7, window, cx| {
                this.switch_workspace(6, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchWorkspace8, window, cx| {
                this.switch_workspace(7, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchWorkspace9, window, cx| {
                this.switch_workspace(8, window, cx);
            }))
            // Go shortcuts
            .on_action(cx.listener(|this, _: &GoHome, window, cx| {
                if let Some(home) = dirs::home_dir() {
                    this.navigate_to(home, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &GoDesktop, window, cx| {
                if let Some(p) = dirs::desktop_dir() {
                    this.navigate_to(p, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &GoDocuments, window, cx| {
                if let Some(p) = dirs::document_dir() {
                    this.navigate_to(p, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &GoDownloads, window, cx| {
                if let Some(p) = dirs::download_dir() {
                    this.navigate_to(p, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &GoApplications, window, cx| {
                this.navigate_to("/Applications".into(), window, cx);
            }))
            // Tool views
            .on_action(cx.listener(|this, _: &OpenCleanup, window, cx| {
                this.push_view(ActiveView::Cleanup, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenDuplicates, window, cx| {
                this.push_view(ActiveView::Dedup, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSecureErase, window, cx| {
                this.push_view(ActiveView::SecureErase, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenAutomations, window, cx| {
                this.push_view(ActiveView::Automations, window, cx);
            }))
            // Window actions
            .on_action(cx.listener(|_, _: &CloseWindow, window, _| {
                window.remove_window();
            }))
            .on_action(cx.listener(|_, _: &Minimize, window, _| {
                window.minimize_window();
            }))
            .on_action(cx.listener(|_, _: &Zoom, window, _| {
                window.zoom_window();
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
                            .when(sidebar_open, |el| el.rounded_tl(RADIUS_LG))
                            .shadow_md()
                            .overflow_hidden()
                            .pt(CONTENT_INSET)
                            .pr(CONTENT_INSET)
                            .pb(CONTENT_INSET)
                            .when(sidebar_open, |el| el.pl(px(0.0)))
                            .when(!sidebar_open, |el| el.pl(CONTENT_INSET))
                            .when_some(titlebar, |el, tb| el.child(tb))
                            .when_some(content_banner, |el, banner_el| el.child(banner_el))
                            .when_some(banner, |el, data| el.child(ProgressBanner::new(data)))
                            .child(
                                h_flex()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .flex_1()
                                            .size_full()
                                            .flex()
                                            .flex_row()
                                            .overflow_hidden()
                                            .child(content)
                                            .when_some(split_content, |el, right| {
                                                el.child(div().w(px(1.0)).h_full().bg(cx.theme().border))
                                                    .child(right)
                                            }),
                                    )
                                    .when_some(ask_view, |el, ask| {
                                        el.child(
                                            div()
                                                .id("ask-resize-handle")
                                                .w(px(1.0))
                                                .h_full()
                                                .cursor_col_resize()
                                                .bg(cx.theme().border)
                                                .on_drag(ResizeAskDrag, |drag, _, _window, cx| {
                                                    cx.new(|_| drag.clone())
                                                })
                                                .on_drag_move(cx.listener(
                                                    |this, event: &DragMoveEvent<ResizeAskDrag>, window, _cx| {
                                                        let window_width = window.viewport_size().width;
                                                        let new_width = window_width - event.event.position.x;
                                                        this.ask_width = new_width.clamp(px(280.0), px(600.0));
                                                    },
                                                )),
                                        )
                                        .child(
                                            div()
                                                .w(ask_width)
                                                .h_full()
                                                .child(ask),
                                        )
                                    }),
                            )
                            .when_some(terminal_view, |el, tv| {
                                el.child(
                                    div()
                                        .id("terminal-resize-handle")
                                        .h(px(4.0))
                                        .w_full()
                                        .cursor_row_resize()
                                        .bg(cx.theme().border)
                                        .on_drag(ResizeTerminalDrag, |drag, _, _window, cx| {
                                            cx.new(|_| drag.clone())
                                        })
                                        .on_drag_move(cx.listener(
                                            |this, event: &DragMoveEvent<ResizeTerminalDrag>, window, _cx| {
                                                let window_height = window.viewport_size().height;
                                                let max_height = window_height * 0.6;
                                                let new_height = window_height - event.event.position.y;
                                                this.terminal_height =
                                                    new_height.clamp(px(100.0), max_height);
                                            },
                                        )),
                                )
                                .child(
                                    div()
                                        .h(terminal_height)
                                        .w_full()
                                        .child(tv),
                                )
                            }),
                    ),
            )
            // Traffic lights when sidebar is hidden (native ones are offscreen)
            .when(!sidebar_open, |el| {
                el.child(
                    div()
                        .absolute()
                        .top(px(11.0))
                        .left(px(11.0))
                        .child(TrafficLights::new()),
                )
            })
            // Command palette overlay
            .when_some(palette_view, |el, palette| {
                el.child(
                    div()
                        .id("palette-overlay")
                        .absolute()
                        .size_full()
                        .top_0()
                        .left_0()
                        .occlude()
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
            .into_any_element()
    }
}
