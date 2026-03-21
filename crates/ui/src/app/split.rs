use gpui::*;

use crate::models::{PaneId, SplitPane};
use crate::views::{FileBrowserEvent, FileBrowserView};

use super::ZeroApp;

impl ZeroApp {
    pub fn toggle_split_view(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!("action: toggle split view");
        if self.split_pane.is_some() {
            self.split_pane = None;
            self.split_browser = None;
            self.active_pane = PaneId::Left;
        } else {
            let path = self.current_path.clone();
            self.split_pane = Some(SplitPane::new(path));
            self.split_browser = None;
            self.active_pane = PaneId::Right;
        }
        cx.notify();
    }

    pub fn ensure_split_browser(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<FileBrowserView> {
        if let Some(view) = &self.split_browser {
            return view.clone();
        }

        let path = self
            .split_pane
            .as_ref()
            .map(|p| p.current_path.clone())
            .unwrap_or_else(|| self.current_path.clone());
        let search = self.services.search.clone();
        let view = cx.new(|cx| FileBrowserView::new(path, search, window, cx));

        let sub = cx.subscribe_in(&view, window, Self::on_split_browser_event);
        self._subs.push(sub);
        self.split_browser = Some(view.clone());
        view
    }

    fn on_split_browser_event(
        &mut self,
        _: &Entity<FileBrowserView>,
        event: &FileBrowserEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            FileBrowserEvent::NavigateToDir(path) => {
                if let Some(ref mut pane) = self.split_pane {
                    pane.navigate_to(path.clone());
                }
                self.split_browser = None;
                cx.notify();
            }
            FileBrowserEvent::OpenFile(path) => {
                self.open_path(path, window, cx);
            }
            FileBrowserEvent::SetClipboard(clipboard) => {
                self.file_clipboard = Some(clipboard.clone());
            }
            _ => {}
        }
    }

    pub fn render_split_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = self.ensure_split_browser(window, cx);
        div().flex_1().size_full().child(view).into_any_element()
    }
}
