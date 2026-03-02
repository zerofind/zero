use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable as _};

use crate::theme::{self, FONT_SIZE_BODY, RADIUS};

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A single row in the sidebar — bookmark, drive, or nav item.
#[derive(IntoElement)]
pub struct SidebarRow {
    id: ElementId,
    label: SharedString,
    icon: IconName,
    active: bool,
    on_click: Option<ClickHandler>,
}

impl SidebarRow {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, icon: IconName) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon,
            active: false,
            on_click: None,
        }
    }

    #[allow(dead_code)] // Used by design-system binary
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for SidebarRow {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let hover_bg = theme::surface_hover(cx);
        let muted = cx.theme().muted_foreground;

        let (text_color, icon_color, active_bg) = if self.active {
            let sel = theme::selection_color();
            (sel, sel, Some(theme::selection_active_bg(cx)))
        } else {
            (cx.theme().foreground, muted, None)
        };

        let on_click = self.on_click;

        div()
            .id(self.id)
            .h(px(32.0))
            .flex()
            .flex_row()
            .gap_2()
            .items_center()
            .px_2()
            .rounded(RADIUS)
            .text_size(FONT_SIZE_BODY)
            .text_color(text_color)
            .cursor_pointer()
            .when_some(active_bg, |el, bg| el.bg(bg))
            .hover(|s| s.bg(hover_bg))
            .when_some(on_click, |el, handler| {
                el.on_click(move |event, window, cx| {
                    handler(event, window, cx);
                })
            })
            .child(Icon::new(self.icon).xsmall().text_color(icon_color))
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .flex_1()
                    .min_w_0()
                    .child(self.label),
            )
    }
}
