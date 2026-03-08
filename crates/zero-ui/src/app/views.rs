use std::path::PathBuf;

use gpui::*;

use crate::views::{
    AppSidebar, AskView, AutomationsView, CleanupView, DataTableView, DedupView, DrivesPopover,
    EditorView, FileBrowserView, FileGridView, PaletteView, SecureEraseView, SettingsView,
    TerminalView, TodoView,
};

use super::ZeroApp;

impl ZeroApp {
    pub fn ensure_sidebar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<AppSidebar> {
        if let Some(view) = &self.sidebar {
            let active = self.active_view.clone();
            let path = self.current_path.clone();
            let toolbar_visible = self.toolbar_visible;
            let can_back = self.can_go_back();
            let can_forward = self.can_go_forward();
            view.update(cx, |sidebar, cx| {
                sidebar.set_active_view(active, cx);
                sidebar.set_current_path(path, cx);
                sidebar.set_toolbar_state(toolbar_visible, can_back, can_forward, cx);
            });
            return view.clone();
        }

        let settings = crate::session::Settings::load();
        let pinned = settings.sidebar_bookmarks;
        let regular = settings.sidebar_regular_bookmarks;
        let active = self.active_view.clone();
        let path = self.current_path.clone();
        let toolbar_visible = self.toolbar_visible;
        let can_back = self.can_go_back();
        let can_forward = self.can_go_forward();
        let view = cx.new(|cx| {
            let mut sidebar = AppSidebar::new(active, path, pinned, regular, cx);
            sidebar.set_toolbar_state(toolbar_visible, can_back, can_forward, cx);
            sidebar
        });

        let sub = cx.subscribe_in(&view, window, Self::on_sidebar_event);
        self._subs.push(sub);
        self.sidebar = Some(view.clone());
        view
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

        view.read(cx).table_state.focus_handle(cx).focus(window);

        view
    }

    pub fn ensure_cleanup(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<CleanupView> {
        if let Some(view) = &self.cleanup {
            return view.clone();
        }
        let search = self.services.search.clone();
        let view = cx.new(|cx| CleanupView::new(search, cx));
        let sub = cx.subscribe_in(&view, window, Self::on_cleanup_event);
        self._subs.push(sub);
        self.cleanup = Some(view.clone());

        // If the view started scanning during construction, the ScanStarted
        // event was emitted before our subscription — show banner manually.
        if view.read(cx).is_scanning() {
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

        view
    }

    pub fn ensure_dedup(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<DedupView> {
        if let Some(view) = &self.dedup {
            return view.clone();
        }
        let search = self.services.search.clone();
        let view = cx.new(|cx| DedupView::new(search, cx));
        let sub = cx.subscribe_in(&view, window, Self::on_dedup_event);
        self._subs.push(sub);
        self.dedup = Some(view.clone());
        view
    }

    pub fn ensure_todo(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<TodoView> {
        if let Some(view) = &self.todo {
            return view.clone();
        }
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
        let mcp = self.services.mcp.clone();
        let view = cx.new(|cx| SettingsView::new(search, mcp, window, cx));

        let sub = cx.subscribe_in(&view, window, Self::on_settings_event);
        self._subs.push(sub);
        self.settings = Some(view.clone());
        view
    }

    pub fn ensure_secure_erase(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SecureEraseView> {
        if let Some(view) = &self.secure_erase {
            return view.clone();
        }
        let view = cx.new(|cx| SecureEraseView::new(window, cx));
        self.secure_erase = Some(view.clone());
        view
    }

    pub fn ensure_automations(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<AutomationsView> {
        if let Some(view) = &self.automations {
            return view.clone();
        }
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

    pub fn ensure_data_table(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<DataTableView> {
        if let Some(view) = &self.data_table {
            return view.clone();
        }

        let view = cx.new(|cx| DataTableView::new(path, window, cx));

        let sub = cx.subscribe_in(&view, window, Self::on_data_table_event);
        self._subs.push(sub);
        self.data_table = Some(view.clone());

        view.read(cx).focus_handle.focus(window);

        view
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

    pub fn ensure_drives_popover(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<DrivesPopover> {
        if let Some(view) = &self.drives_popover {
            return view.clone();
        }
        let view = cx.new(DrivesPopover::new);
        let sub = cx.subscribe_in(&view, window, Self::on_drives_popover_event);
        self._subs.push(sub);
        self.drives_popover = Some(view.clone());
        view
    }

    pub fn ensure_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TerminalView> {
        if let Some(view) = &self.terminal {
            return view.clone();
        }

        let cwd = self.current_path.clone();
        let view = cx.new(|cx| TerminalView::new(cwd, window, cx));

        let sub = cx.subscribe_in(&view, window, Self::on_terminal_event);
        self._subs.push(sub);
        self.terminal = Some(view.clone());
        view
    }

    pub fn ensure_ask(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<AskView> {
        if let Some(view) = &self.ask {
            return view.clone();
        }

        let llm = self.services.llm.clone();
        let view = cx.new(|cx| AskView::new(llm, window, cx));

        let sub = cx.subscribe_in(&view, window, Self::on_ask_event);
        self._subs.push(sub);
        self.ask = Some(view.clone());
        view
    }
}
