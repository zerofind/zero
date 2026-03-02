use std::path::PathBuf;

use gpui::*;

use crate::models::ActiveView;
use crate::ui::BannerKind;
use crate::views::{
    AppSidebar, AutomationsView, CleanupView, DedupEvent, DedupView, DrivesPopover, EditorEvent,
    EditorView, FileBrowserEvent, FileBrowserView, FileGridEvent, FileGridView, PaletteEvent,
    PaletteView, SecureEraseView, SettingsEvent, SettingsView, TodoView,
    drives_popover::DrivesPopoverEvent, sidebar::SidebarEvent,
};

use super::ZeroApp;

impl ZeroApp {
    pub fn focus_content(&self, window: &mut Window, cx: &App) {
        if self.active_view == ActiveView::FileBrowser {
            match self.view_mode {
                crate::models::ViewMode::List => {
                    if let Some(view) = &self.file_browser {
                        view.read(cx).focus_handle.focus(window);
                        return;
                    }
                }
                crate::models::ViewMode::Grid => {
                    if let Some(view) = &self.file_grid {
                        view.read(cx).focus_handle.focus(window);
                        return;
                    }
                }
            }
        }
        self.focus_handle.focus(window);
    }

    pub fn navigate_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.history_idx + 1 < self.history.len() {
            self.history.truncate(self.history_idx + 1);
        }
        self.history.push(path.clone());
        self.history_idx = self.history.len() - 1;
        self.current_path = path;

        self.file_browser = None;
        self.file_grid = None;
        self.active_view = ActiveView::FileBrowser;
        cx.notify();
    }

    pub fn go_back(&mut self, cx: &mut Context<Self>) {
        if self.history_idx == 0 {
            return;
        }
        self.history_idx -= 1;
        self.current_path = self.history[self.history_idx].clone();
        self.file_browser = None;
        self.file_grid = None;
        cx.notify();
    }

    pub fn go_forward(&mut self, cx: &mut Context<Self>) {
        if self.history_idx + 1 >= self.history.len() {
            return;
        }
        self.history_idx += 1;
        self.current_path = self.history[self.history_idx].clone();
        self.file_browser = None;
        self.file_grid = None;
        cx.notify();
    }

    pub fn can_go_back(&self) -> bool { self.history_idx > 0 }
    pub fn can_go_forward(&self) -> bool { self.history_idx + 1 < self.history.len() }

    pub fn ensure_sidebar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<AppSidebar> {
        if let Some(view) = &self.sidebar {
            let active = self.active_view.clone();
            let path = self.current_path.clone();
            view.update(cx, |sidebar, cx| {
                sidebar.set_active_view(active, cx);
                sidebar.set_current_path(path, cx);
            });
            return view.clone();
        }

        let bookmarks = crate::session::Settings::load().sidebar_bookmarks;
        let active = self.active_view.clone();
        let path = self.current_path.clone();
        let view = cx.new(|cx| AppSidebar::new(active, path, bookmarks, cx));

        let sub = cx.subscribe_in(&view, window, Self::on_sidebar_event);
        self._subs.push(sub);
        self.sidebar = Some(view.clone());
        view
    }

    fn on_sidebar_event(
        &mut self, _: &Entity<AppSidebar>, event: &SidebarEvent,
        window: &mut Window, cx: &mut Context<Self>,
    ) {
        match event {
            SidebarEvent::Navigate(view) => {
                self.active_view = view.clone();
                self.focus_content(window, cx);
                cx.notify();
            }
            SidebarEvent::OpenPath(path) => self.navigate_to(path.clone(), cx),
            SidebarEvent::RemoveBookmark(path) => {
                if let Some(sidebar) = &self.sidebar {
                    sidebar.update(cx, |s, cx| s.remove_bookmark(path, cx));
                }
                let mut settings = crate::session::Settings::load();
                settings.sidebar_bookmarks.retain(|b| b != path);
                settings.save();
            }
            SidebarEvent::EjectDrive(path) => {
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
                        eprintln!("[zero-ui] eject error: {e}");
                    }
                    // Refresh drives list
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
        }
    }

    pub fn ensure_file_browser(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<FileBrowserView> {
        if let Some(view) = &self.file_browser {
            return view.clone();
        }

        let path = self.current_path.clone();
        let search = self.services.search.clone();
        let view = cx.new(|cx| FileBrowserView::new(path, search, window, cx));

        let sub = cx.subscribe_in(&view, window, Self::on_file_browser_event);
        self._subs.push(sub);
        self.file_browser = Some(view.clone());

        view.read(cx).focus_handle.focus(window);

        view
    }

    fn on_file_browser_event(
        &mut self, _: &Entity<FileBrowserView>, event: &FileBrowserEvent,
        _window: &mut Window, cx: &mut Context<Self>,
    ) {
        match event {
            FileBrowserEvent::NavigateToDir(path) => self.navigate_to(path.clone(), cx),
            FileBrowserEvent::OpenFile(path) => {
                self.editor = None;
                self.active_view = ActiveView::Editor(path.clone());
                cx.notify();
            }
            FileBrowserEvent::SetClipboard(clipboard) => {
                self.file_clipboard = Some(clipboard.clone());
            }
            FileBrowserEvent::AddBookmark(path) => {
                if let Some(sidebar) = &self.sidebar {
                    sidebar.update(cx, |s, cx| s.add_bookmark(path.clone(), cx));
                }
                let mut settings = crate::session::Settings::load();
                if !settings.sidebar_bookmarks.contains(path) {
                    settings.sidebar_bookmarks.push(path.clone());
                    settings.save();
                }
            }
            FileBrowserEvent::PasteStarted(progress) => {
                self.start_progress_polling(
                    BannerKind::Copy,
                    "Copying files...".to_string(),
                    progress.clone(),
                    cx,
                );
            }
            FileBrowserEvent::PasteFinished => { self.clear_banner(cx); }
            FileBrowserEvent::NewTodoFile(path) => {
                // Open the new .todo file in the Todo view
                self.todo = None; // Reset so it recreates fresh
                let todo = self.ensure_todo(_window, cx);
                todo.update(cx, |view, cx| {
                    view.open_file(path.clone(), cx);
                });
                self.active_view = ActiveView::Todo;
                cx.notify();
            }
            FileBrowserEvent::FindDuplicatesHere(path) => {
                // Reset dedup view so it recreates with the new path
                self.dedup = None;
                let dedup = self.ensure_dedup(_window, cx);
                dedup.update(cx, |view, cx| {
                    view.set_scan_path(path.clone(), cx);
                });
                self.active_view = ActiveView::Dedup;
                cx.notify();
            }
            FileBrowserEvent::MoveToOtherPane(paths) => {
                if let Some(ref pane) = self.split_pane {
                    let dest = pane.current_path.clone();
                    for src in paths {
                        if let Some(name) = src.file_name() {
                            let target = dest.join(name);
                            let _ = std::fs::rename(src, &target);
                        }
                    }
                    // Refresh both panes
                    self.split_browser = None;
                    if let Some(browser) = &self.file_browser {
                        browser.update(cx, |view, cx| view.reload(cx));
                    }
                    cx.notify();
                }
            }
            FileBrowserEvent::CopyToOtherPane(paths) => {
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

    pub fn paste_files(&mut self, cx: &mut Context<Self>) {
        let Some(clipboard) = self.file_clipboard.clone() else {
            return;
        };
        if let Some(browser) = &self.file_browser {
            browser.update(cx, |view, cx| {
                view.paste_files(&clipboard, cx);
            });
        }
        if clipboard.operation == crate::models::ClipboardOperation::Cut {
            self.file_clipboard = None;
        }
    }

    pub fn ensure_cleanup(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Entity<CleanupView> {
        if let Some(view) = &self.cleanup { return view.clone(); }
        let search = self.services.search.clone();
        let view = cx.new(|cx| CleanupView::new(search, cx));
        self.cleanup = Some(view.clone());
        view
    }

    pub fn ensure_dedup(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<DedupView> {
        if let Some(view) = &self.dedup { return view.clone(); }
        let view = cx.new(DedupView::new);
        let sub = cx.subscribe_in(&view, window, Self::on_dedup_event);
        self._subs.push(sub);
        self.dedup = Some(view.clone());
        view
    }

    fn on_dedup_event(
        &mut self, _: &Entity<DedupView>, event: &DedupEvent,
        _window: &mut Window, cx: &mut Context<Self>,
    ) {
        match event {
            DedupEvent::ScanStarted(progress) => {
                self.start_dedup_progress_polling(
                    BannerKind::Dedup,
                    "Finding duplicate files...".to_string(),
                    progress.clone(),
                    cx,
                );
            }
            DedupEvent::ScanFinished => {
                self.clear_banner(cx);
            }
        }
    }

    pub fn ensure_todo(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<TodoView> {
        if let Some(view) = &self.todo { return view.clone(); }
        let view = cx.new(|cx| TodoView::new(window, cx));
        self.todo = Some(view.clone());
        view
    }

    pub fn ensure_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SettingsView> {
        if let Some(view) = &self.settings {
            return view.clone();
        }

        let search = self.services.search.clone();
        let view = cx.new(|cx| SettingsView::new(search, window, cx));

        let sub = cx.subscribe_in(&view, window, Self::on_settings_event);
        self._subs.push(sub);
        self.settings = Some(view.clone());
        view
    }

    fn on_settings_event(
        &mut self, _: &Entity<SettingsView>, event: &SettingsEvent,
        _window: &mut Window, cx: &mut Context<Self>,
    ) {
        match event {
            SettingsEvent::IndexRebuildStarted(progress) => {
                self.start_crawl_progress_polling(
                    BannerKind::Index,
                    "Rebuilding search index...".to_string(),
                    progress.clone(),
                    cx,
                );
            }
            SettingsEvent::IndexRebuildFinished => {
                self.clear_banner(cx);
            }
        }
    }

    pub fn ensure_secure_erase(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<SecureEraseView> {
        if let Some(view) = &self.secure_erase { return view.clone(); }
        let view = cx.new(|cx| SecureEraseView::new(window, cx));
        self.secure_erase = Some(view.clone());
        view
    }

    pub fn ensure_automations(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Entity<AutomationsView> {
        if let Some(view) = &self.automations { return view.clone(); }
        let view = cx.new(AutomationsView::new);
        self.automations = Some(view.clone());
        view
    }

    pub fn ensure_editor(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EditorView> {
        if let Some(view) = &self.editor {
            return view.clone();
        }

        let view = cx.new(|cx| EditorView::new(path, window, cx));

        let sub = cx.subscribe_in(&view, window, Self::on_editor_event);
        self._subs.push(sub);
        self.editor = Some(view.clone());

        view.read(cx).focus_handle.focus(window);

        view
    }

    fn on_editor_event(
        &mut self, _: &Entity<EditorView>, event: &EditorEvent,
        window: &mut Window, cx: &mut Context<Self>,
    ) {
        match event {
            EditorEvent::Close => {
                self.editor = None;
                self.active_view = ActiveView::FileBrowser;
                self.focus_content(window, cx);
                cx.notify();
            }
        }
    }

    pub fn ensure_command_palette(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<PaletteView> {
        if let Some(view) = &self.command_palette {
            return view.clone();
        }

        let search = self.services.search.clone();
        let apps = self.services.apps.clone();
        let view = cx.new(|cx| PaletteView::new(search, apps, window, cx));

        let sub = cx.subscribe_in(&view, window, Self::on_palette_event);
        self._subs.push(sub);
        self.command_palette = Some(view.clone());
        view
    }

    fn action_view(action: &str) -> Option<ActiveView> {
        match action {
            "action://settings" => Some(ActiveView::Settings),
            "action://cleanup" => Some(ActiveView::Cleanup),
            "action://dedup" => Some(ActiveView::Dedup),
            "action://todo" => Some(ActiveView::Todo),
            "action://secure_erase" => Some(ActiveView::SecureErase),
            "action://automations" => Some(ActiveView::Automations),
            _ => None,
        }
    }

    fn on_palette_event(
        &mut self, _: &Entity<PaletteView>, event: &PaletteEvent,
        window: &mut Window, cx: &mut Context<Self>,
    ) {
        match event {
            PaletteEvent::Dismiss => {
                self.command_palette_open = false;
                self.focus_content(window, cx);
                cx.notify();
            }
            PaletteEvent::OpenResult(path) => {
                self.command_palette_open = false;

                if let Some(path_str) = path.to_str() {
                    if let Some(view) = Self::action_view(path_str) {
                        self.active_view = view;
                        self.focus_content(window, cx);
                        cx.notify();
                        return;
                    }
                    if let Some(app_path) = path_str.strip_prefix("app://") {
                        crate::services::AppService::launch(
                            &std::path::PathBuf::from(app_path),
                        );
                        self.focus_content(window, cx);
                        cx.notify();
                        return;
                    }
                    if let Some(type_name) = path_str.strip_prefix("type://") {
                        let prefix = format!("@{} ", type_name);
                        self.command_palette_open = true;
                        if let Some(palette) = &self.command_palette {
                            palette.update(cx, |view, cx| {
                                view.set_query(&prefix, window, cx);
                            });
                        }
                        cx.notify();
                        return;
                    }
                }

                if path.is_dir() {
                    self.navigate_to(path.clone(), cx);
                } else if crate::views::editor::is_editable(path) {
                    self.editor = None;
                    self.active_view = ActiveView::Editor(path.clone());
                    cx.notify();
                    return;
                } else {
                    #[cfg(target_os = "macos")]
                    crate::platform::open::open_path(path);
                }
                self.focus_content(window, cx);
                cx.notify();
            }
            PaletteEvent::ShowInBrowser(query) => {
                self.command_palette_open = false;

                let results = self.services.search.read(cx).search(query, 500);
                let entries: Vec<crate::views::file_browser::state::BrowserEntry> = results
                    .iter()
                    .filter_map(|r| {
                        let path = std::path::Path::new(&r.node.path);
                        crate::views::file_browser::state::BrowserEntry::from_fs(path, 0)
                    })
                    .collect();

                self.active_view = ActiveView::FileBrowser;
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
        }
    }

    pub fn ensure_file_grid(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<FileGridView> {
        if let Some(view) = &self.file_grid {
            return view.clone();
        }

        let entries = crate::views::file_browser::state::load_directory(&self.current_path);
        let view = cx.new(|cx| FileGridView::new(entries, cx));

        let sub = cx.subscribe_in(&view, window, Self::on_file_grid_event);
        self._subs.push(sub);
        self.file_grid = Some(view.clone());
        view
    }

    fn on_file_grid_event(
        &mut self, _: &Entity<FileGridView>, event: &FileGridEvent,
        _window: &mut Window, cx: &mut Context<Self>,
    ) {
        match event {
            FileGridEvent::NavigateToDir(path) => self.navigate_to(path.clone(), cx),
        }
    }

    pub fn ensure_drives_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<DrivesPopover> {
        if let Some(view) = &self.drives_popover {
            view.update(cx, |v, cx| v.refresh(cx));
            return view.clone();
        }
        let view = cx.new(DrivesPopover::new);
        let sub = cx.subscribe_in(&view, window, Self::on_drives_popover_event);
        self._subs.push(sub);
        self.drives_popover = Some(view.clone());
        view
    }

    fn on_drives_popover_event(
        &mut self, _: &Entity<DrivesPopover>, event: &DrivesPopoverEvent,
        window: &mut Window, cx: &mut Context<Self>,
    ) {
        match event {
            DrivesPopoverEvent::Dismiss => {
                self.drives_popover_open = false;
                self.focus_content(window, cx);
                cx.notify();
            }
            DrivesPopoverEvent::OpenPath(path) => {
                self.drives_popover_open = false;
                self.navigate_to(path.clone(), cx);
                self.focus_content(window, cx);
            }
            DrivesPopoverEvent::RunAutomation(name) => {
                // TODO: Trigger automation runner with the given automation name
                eprintln!("[zero-ui] run automation: {}", name);
            }
            DrivesPopoverEvent::EjectDrive(path) => {
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
                        eprintln!("[zero-ui] eject error: {e}");
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
        }
    }

}
