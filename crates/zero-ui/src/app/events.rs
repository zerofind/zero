use std::sync::Arc;

use gpui::*;

use crate::models::ActiveView;
use crate::ui::BannerKind;
use crate::views::{
    AppSidebar, AskView, AskViewEvent, CleanupView, CleanupViewEvent, DataTableEvent,
    DataTableView, DedupEvent, DedupView, DrivesPopover, EditorEvent, EditorView, FileBrowserEvent,
    FileBrowserView, FileGridEvent, FileGridView, PaletteEvent, PaletteView, SettingsEvent,
    SettingsView, TerminalView, TerminalViewEvent, drives_popover::DrivesPopoverEvent,
    sidebar::SidebarEvent,
};

use super::ZeroApp;

impl ZeroApp {
    pub(crate) fn on_sidebar_event(
        &mut self,
        _: &Entity<AppSidebar>,
        event: &SidebarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SidebarEvent::Navigate(view) => {
                tracing::debug!(?view, "sidebar: navigate view");
                self.push_view(view.clone(), window, cx);
            }
            SidebarEvent::OpenPath(path) => {
                tracing::debug!(path = %path.display(), "sidebar: open path");
                self.navigate_to(path.clone(), window, cx);
            }
            SidebarEvent::RemoveBookmark(path) => {
                tracing::debug!(path = %path.display(), "sidebar: remove bookmark");
                if let Some(sidebar) = &self.sidebar {
                    sidebar.update(cx, |s, cx| s.remove_bookmark(path, cx));
                }
                let mut settings = crate::session::Settings::load();
                settings.sidebar_bookmarks.retain(|b| b != path);
                settings.sidebar_regular_bookmarks.retain(|b| b != path);
                settings.save();
            }
            SidebarEvent::UnpinBookmark(path) => {
                tracing::debug!(path = %path.display(), "sidebar: unpin bookmark");
                if let Some(sidebar) = &self.sidebar {
                    sidebar.update(cx, |s, cx| s.unpin_bookmark(path, cx));
                }
                let mut settings = crate::session::Settings::load();
                if let Some(pos) = settings.sidebar_bookmarks.iter().position(|b| b == path) {
                    let removed = settings.sidebar_bookmarks.remove(pos);
                    if !settings.sidebar_regular_bookmarks.contains(&removed) {
                        settings.sidebar_regular_bookmarks.push(removed);
                    }
                }
                settings.save();
            }
            SidebarEvent::PinBookmark(path) => {
                tracing::debug!(path = %path.display(), "sidebar: pin bookmark");
                if let Some(sidebar) = &self.sidebar {
                    sidebar.update(cx, |s, cx| s.pin_bookmark(path, cx));
                }
                let mut settings = crate::session::Settings::load();
                if let Some(pos) = settings
                    .sidebar_regular_bookmarks
                    .iter()
                    .position(|b| b == path)
                {
                    let removed = settings.sidebar_regular_bookmarks.remove(pos);
                    if !settings.sidebar_bookmarks.contains(&removed) {
                        settings.sidebar_bookmarks.push(removed);
                    }
                }
                settings.save();
            }
            SidebarEvent::FindDuplicates(path) => {
                tracing::debug!(path = %path.display(), "sidebar: find duplicates");
                self.launch_dedup(path, window, cx);
            }
            SidebarEvent::IndexLocation(path) => {
                tracing::debug!(path = %path.display(), "sidebar: index location");
                let path_str = path.to_string_lossy().to_string();
                self.services.search.update(cx, |svc, cx| {
                    svc.add_root(&path_str, cx);
                });
                let mut settings = crate::session::Settings::load();
                if !settings.search_roots.contains(path) {
                    settings.search_roots.push(path.clone());
                    settings.save();
                }
            }
            SidebarEvent::GoBack => {
                tracing::debug!("sidebar: go back");
                self.go_back(window, cx);
            }
            SidebarEvent::GoForward => {
                tracing::debug!("sidebar: go forward");
                self.go_forward(window, cx);
            }
            SidebarEvent::ToggleSidebar => {
                tracing::debug!("sidebar: toggle");
                self.sidebar_open = !self.sidebar_open;
                cx.notify();
            }
            SidebarEvent::OpenSearch => {
                tracing::debug!("sidebar: open search");
                self.open_command_palette(window, cx);
            }
            SidebarEvent::EjectDrive(path) => {
                #[cfg(target_os = "macos")]
                {
                    let mount = path.clone();
                    let sidebar = self.sidebar.clone();
                    cx.spawn(async move |_this, cx| {
                        let result = cx
                            .background_executor()
                            .spawn({
                                let mount = mount.clone();
                                async move { crate::platform::open::eject_drive(&mount) }
                            })
                            .await;

                        if let Err(e) = result {
                            tracing::error!(error = %e, "eject failed");
                        }
                        if let Some(sidebar) = sidebar {
                            _this
                                .update(cx, |_app, cx| {
                                    sidebar.update(cx, |s, cx| s.refresh_drives(cx));
                                })
                                .ok();
                        }
                    })
                    .detach();
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = path;
                    tracing::warn!("eject not supported on this platform");
                }
            }
        }
    }

    pub(crate) fn on_file_browser_event(
        &mut self,
        _: &Entity<FileBrowserView>,
        event: &FileBrowserEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            FileBrowserEvent::NavigateToDir(path) => {
                tracing::debug!(path = %path.display(), "browser: navigate to dir");
                self.navigate_to(path.clone(), window, cx);
            }
            FileBrowserEvent::OpenFile(path) => {
                tracing::debug!(path = %path.display(), "browser: open file");
                self.open_path(path, window, cx);
            }
            FileBrowserEvent::SetClipboard(clipboard) => {
                tracing::debug!(op = ?clipboard.operation, count = clipboard.paths.len(), "browser: set clipboard");
                self.file_clipboard = Some(clipboard.clone());
            }
            FileBrowserEvent::AddBookmark(path) => {
                tracing::debug!(path = %path.display(), "browser: add bookmark");
                if let Some(sidebar) = &self.sidebar {
                    sidebar.update(cx, |s, cx| s.add_bookmark(path.clone(), cx));
                }
                let mut settings = crate::session::Settings::load();
                if !settings.sidebar_regular_bookmarks.contains(path)
                    && !settings.sidebar_bookmarks.contains(path)
                {
                    settings.sidebar_regular_bookmarks.push(path.clone());
                    settings.save();
                }
            }
            FileBrowserEvent::PasteStarted(progress) => {
                tracing::debug!("browser: paste started");
                self.start_progress_polling(
                    BannerKind::Copy,
                    "Copying files...".to_string(),
                    progress.clone(),
                    cx,
                );
            }
            FileBrowserEvent::PasteFinished => {
                tracing::debug!("browser: paste finished");
                self.clear_banner(cx);
            }
            FileBrowserEvent::NewTodoFile(path) => {
                tracing::debug!(path = %path.display(), "browser: new todo file");
                self.todo = None;
                let todo = self.ensure_todo(window, cx);
                todo.update(cx, |view, cx| {
                    view.open_file(path.clone(), cx);
                });
                self.push_view(ActiveView::Todo, window, cx);
            }
            FileBrowserEvent::FindDuplicatesHere(path) => {
                tracing::debug!(path = %path.display(), "browser: find duplicates here");
                self.launch_dedup(path, window, cx);
            }
            FileBrowserEvent::MoveToOtherPane(paths) => {
                tracing::debug!(count = paths.len(), "browser: move to other pane");
                if let Some(ref pane) = self.split_pane {
                    let dest = pane.current_path.clone();
                    for src in paths {
                        if let Some(name) = src.file_name() {
                            let target = dest.join(name);
                            let _ = std::fs::rename(src, &target);
                        }
                    }
                    self.split_browser = None;
                    if let Some(browser) = &self.file_browser {
                        browser.update(cx, |view, cx| view.reload(cx));
                    }
                    cx.notify();
                }
            }
            FileBrowserEvent::CopyToOtherPane(paths) => {
                tracing::debug!(count = paths.len(), "browser: copy to other pane");
                if let Some(ref pane) = self.split_pane {
                    let dest = pane.current_path.clone();
                    for src in paths {
                        if let Some(name) = src.file_name() {
                            let target = dest.join(name);
                            let _ = crate::views::file_browser::copy_recursive(src, &target);
                        }
                    }
                    self.split_browser = None;
                    cx.notify();
                }
            }
        }
    }

    pub(crate) fn on_cleanup_event(
        &mut self,
        _: &Entity<CleanupView>,
        event: &CleanupViewEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            CleanupViewEvent::ScanStarted => {
                tracing::debug!("cleanup: scan started");
                self.banner = Some(crate::ui::BannerData {
                    kind: crate::ui::BannerKind::Cleanup,
                    message: "Scanning for cleanable files...".to_string(),
                    bytes_done: 0,
                    bytes_total: 0,
                    files_done: 0,
                    files_total: 0,
                    phase: Some("Scanning...".to_string()),
                    indeterminate: true,
                    on_cancel: None,
                });
                cx.notify();
            }
            CleanupViewEvent::ScanFinished => {
                tracing::debug!("cleanup: scan finished");
                self.clear_banner(cx);
            }
            CleanupViewEvent::CleanStarted { message, progress } => {
                tracing::info!("cleanup: clean started — {message}");
                self.start_cleanup_progress_polling(
                    message.clone(),
                    std::sync::Arc::clone(progress),
                    cx,
                );
            }
            CleanupViewEvent::CleanFinished {
                ok,
                failed,
                cancelled,
            } => {
                tracing::info!(ok, failed, cancelled, "cleanup: clean finished");
                self.clear_banner(cx);
            }
            CleanupViewEvent::DrillInto(idx) => {
                tracing::debug!(idx, "cleanup: drill into group");
                self.push_view(ActiveView::CleanupDetail(*idx), window, cx);
            }
        }
    }

    pub(crate) fn on_dedup_event(
        &mut self,
        _: &Entity<DedupView>,
        event: &DedupEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            DedupEvent::ScanStarted(progress) => {
                tracing::debug!("dedup: scan started");
                let cancel_progress = progress.clone();
                let on_cancel: Arc<dyn Fn() + Send + Sync> =
                    Arc::new(move || cancel_progress.cancel());
                self.start_dedup_progress_polling(
                    BannerKind::Dedup,
                    "Finding duplicate files...".to_string(),
                    progress.clone(),
                    Some(on_cancel),
                    cx,
                );
            }
            DedupEvent::ScanFinished => {
                tracing::debug!("dedup: scan finished");
                self.clear_banner(cx);
            }
        }
    }

    pub(crate) fn on_settings_event(
        &mut self,
        _: &Entity<SettingsView>,
        event: &SettingsEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SettingsEvent::IndexRebuildStarted(progress) => {
                tracing::debug!("settings: index rebuild started");
                let cancel_progress = progress.clone();
                let on_cancel: std::sync::Arc<dyn Fn() + Send + Sync> =
                    std::sync::Arc::new(move || cancel_progress.cancel());
                self.start_crawl_progress_polling(
                    BannerKind::Index,
                    "Rebuilding search index...".to_string(),
                    progress.clone(),
                    Some(on_cancel),
                    cx,
                );
            }
            SettingsEvent::IndexRebuildFinished => {
                tracing::debug!("settings: index rebuild finished");
                self.clear_banner(cx);
            }
        }
    }

    pub(crate) fn on_editor_event(
        &mut self,
        _: &Entity<EditorView>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            EditorEvent::Close => {
                tracing::debug!("editor: close");
                self.editor = None;
                self.go_back(window, cx);
            }
        }
    }

    pub(crate) fn on_data_table_event(
        &mut self,
        _: &Entity<DataTableView>,
        event: &DataTableEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            DataTableEvent::Close => {
                tracing::debug!("data_table: close");
                self.data_table = None;
                self.go_back(window, cx);
            }
        }
    }

    pub(crate) fn on_palette_event(
        &mut self,
        _: &Entity<PaletteView>,
        event: &PaletteEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PaletteEvent::Dismiss => {
                tracing::debug!("palette: dismiss");
                self.command_palette_open = false;
                self.focus_content(window, cx);
                cx.notify();
            }
            PaletteEvent::OpenResult(path) => {
                tracing::debug!(path = %path.display(), "palette: open result");
                self.command_palette_open = false;

                if let Some(path_str) = path.to_str() {
                    // Navigation / view-toggle actions
                    match path_str {
                        "action://toggle_sidebar" => {
                            self.sidebar_open = !self.sidebar_open;
                            self.focus_content(window, cx);
                            cx.notify();
                            return;
                        }
                        "action://toggle_split_view" => {
                            self.toggle_split_view(window, cx);
                            self.focus_content(window, cx);
                            cx.notify();
                            return;
                        }
                        "action://go_back" => {
                            self.go_back(window, cx);
                            return;
                        }
                        "action://go_forward" => {
                            self.go_forward(window, cx);
                            return;
                        }
                        "action://go_up" => {
                            if let Some(parent) =
                                self.current_path.parent().map(|p| p.to_path_buf())
                            {
                                self.navigate_to(parent, window, cx);
                            }
                            self.focus_content(window, cx);
                            cx.notify();
                            return;
                        }
                        "action://go_home" => {
                            if let Some(home) = dirs::home_dir() {
                                self.navigate_to(home, window, cx);
                            }
                            self.focus_content(window, cx);
                            cx.notify();
                            return;
                        }
                        "action://new_folder" => {
                            if let Some(browser) = &self.file_browser {
                                browser.update(cx, |view, cx| {
                                    view.start_new_folder(window, cx);
                                });
                            }
                            self.focus_content(window, cx);
                            cx.notify();
                            return;
                        }
                        _ => {}
                    }

                    // View-switching actions
                    if let Some(view) = Self::action_view(path_str) {
                        self.push_view(view, window, cx);
                        return;
                    }
                    if let Some(app_path) = path_str.strip_prefix("app://") {
                        crate::services::AppService::launch(&std::path::PathBuf::from(app_path));
                        self.focus_content(window, cx);
                        cx.notify();
                        return;
                    }
                    if path_str.starts_with("type://") {
                        tracing::warn!(path = %path_str, "unexpected type:// path reached navigation");
                        return;
                    }
                }

                self.open_path(path, window, cx);
                self.focus_content(window, cx);
                cx.notify();
            }
            PaletteEvent::ShowInBrowser(query) => {
                tracing::debug!(query = %query, "palette: show in browser");
                self.command_palette_open = false;

                let results = self.services.search.read(cx).search(query, 0);
                let entries: Vec<crate::views::file_browser::state::BrowserEntry> = results
                    .iter()
                    .filter_map(|r| {
                        let path = std::path::Path::new(&r.node.path);
                        crate::views::file_browser::state::BrowserEntry::from_fs(path, 0)
                    })
                    .collect();

                let current = self.current_path.clone();
                self.active_view = ActiveView::FileBrowser(current.clone());
                self.file_browser = None;
                self.file_grid = None;
                cx.notify();

                let browser = self.ensure_file_browser(window, cx);
                browser.update(cx, |view, cx| {
                    view.show_search_results(query.clone(), entries, cx);
                });

                self.focus_content(window, cx);
                cx.notify();
            }
            PaletteEvent::ShowTypeInBrowser {
                type_filter,
                query,
                label,
            } => {
                tracing::debug!(
                    type_filter = %type_filter,
                    query = %query,
                    "palette: show type in browser"
                );
                self.command_palette_open = false;

                let search = self.services.search.read(cx);
                let results = if query.is_empty() {
                    search.search_by_type(type_filter, 0)
                } else {
                    search.search_with_type(query, type_filter, 0)
                };
                let entries: Vec<crate::views::file_browser::state::BrowserEntry> = results
                    .iter()
                    .filter_map(|r| {
                        let path = std::path::Path::new(&r.node.path);
                        crate::views::file_browser::state::BrowserEntry::from_fs(path, 0)
                    })
                    .collect();

                let display_query = if query.is_empty() {
                    label.clone()
                } else {
                    format!("{label}: {query}")
                };

                let current = self.current_path.clone();
                self.active_view = ActiveView::FileBrowser(current.clone());
                self.file_browser = None;
                self.file_grid = None;
                cx.notify();

                let browser = self.ensure_file_browser(window, cx);
                browser.update(cx, |view, cx| {
                    view.show_search_results(display_query, entries, cx);
                });

                self.focus_content(window, cx);
                cx.notify();
            }
        }
    }

    pub(crate) fn on_file_grid_event(
        &mut self,
        _: &Entity<FileGridView>,
        event: &FileGridEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            FileGridEvent::NavigateToDir(path) => {
                tracing::debug!(path = %path.display(), "grid: navigate to dir");
                self.navigate_to(path.clone(), window, cx);
            }
        }
    }

    pub(crate) fn on_drives_popover_event(
        &mut self,
        _: &Entity<DrivesPopover>,
        event: &DrivesPopoverEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            DrivesPopoverEvent::Dismiss => {
                tracing::debug!("drives: dismiss");
                self.drives_popover_open = false;
                self.focus_content(window, cx);
                cx.notify();
            }
            DrivesPopoverEvent::OpenPath(path) => {
                tracing::debug!(path = %path.display(), "drives: open path");
                self.drives_popover_open = false;
                self.navigate_to(path.clone(), window, cx);
            }
            DrivesPopoverEvent::RunAutomation(name) => {
                // TODO: Trigger automation runner with the given automation name
                tracing::info!(automation = %name, "run automation");
            }
            DrivesPopoverEvent::EjectDrive(path) => {
                #[cfg(target_os = "macos")]
                {
                    let mount = path.clone();
                    let drives_popover = self.drives_popover.clone();
                    let sidebar = self.sidebar.clone();
                    cx.spawn(async move |_this, cx| {
                        let result = cx
                            .background_executor()
                            .spawn({
                                let mount = mount.clone();
                                async move { crate::platform::open::eject_drive(&mount) }
                            })
                            .await;

                        if let Err(e) = result {
                            tracing::error!(error = %e, "eject failed");
                        }
                        _this
                            .update(cx, |_app, cx| {
                                if let Some(dp) = drives_popover {
                                    dp.update(cx, |v, cx| v.refresh(cx));
                                }
                                if let Some(sb) = sidebar {
                                    sb.update(cx, |s, cx| s.refresh_drives(cx));
                                }
                            })
                            .ok();
                    })
                    .detach();
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = path;
                    tracing::warn!("eject not supported on this platform");
                }
            }
        }
    }

    pub(crate) fn on_terminal_event(
        &mut self,
        _: &Entity<TerminalView>,
        event: &TerminalViewEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TerminalViewEvent::Close => {
                tracing::debug!("terminal: close");
                self.terminal_open = false;
                self.terminal = None;
                cx.notify();
            }
        }
    }

    pub(crate) fn on_ask_event(
        &mut self,
        _: &Entity<AskView>,
        event: &AskViewEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            AskViewEvent::Close => {
                tracing::debug!("ask: close");
                self.ask_open = false;
                self.ask = None;
                cx.notify();
            }
        }
    }
}
