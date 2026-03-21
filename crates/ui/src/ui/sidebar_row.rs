use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable as _};

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, ICON_SM, RADIUS, SIDEBAR_ROW_HEIGHT};

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A single row in the sidebar — bookmark, drive, or nav item.
#[derive(IntoElement)]
pub struct SidebarRow {
    id: ElementId,
    label: SharedString,
    subtitle: Option<SharedString>,
    icon: IconName,
    active: bool,
    color_override: Option<Hsla>,
    on_click: Option<ClickHandler>,
}

impl SidebarRow {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, icon: IconName) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            subtitle: None,
            icon,
            active: false,
            color_override: None,
            on_click: None,
        }
    }

    #[allow(dead_code)] // Used by design-system binary
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Override both text and icon color (e.g. warning for git-dirty bookmarks).
    pub fn color_override(mut self, color: Hsla) -> Self {
        self.color_override = Some(color);
        self
    }

    pub fn subtitle(mut self, text: impl Into<SharedString>) -> Self {
        self.subtitle = Some(text.into());
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

        let (mut text_color, mut icon_color, active_bg) = if self.active {
            (
                cx.theme().foreground,
                theme::selection_color(cx),
                Some(theme::selection_active_bg(cx)),
            )
        } else {
            (cx.theme().foreground, muted, None)
        };

        if let Some(c) = self.color_override {
            text_color = c;
            icon_color = c;
        }

        let row_height = if self.subtitle.is_some() {
            px(36.0)
        } else {
            SIDEBAR_ROW_HEIGHT
        };

        let on_click = self.on_click;
        let subtitle = self.subtitle;

        let label_el = if let Some(sub) = subtitle {
            div()
                .overflow_hidden()
                .flex()
                .flex_col()
                .justify_center()
                .flex_1()
                .min_w_0()
                .child(div().text_ellipsis().whitespace_nowrap().child(self.label))
                .child(
                    div()
                        .text_size(FONT_SIZE_CAPTION)
                        .text_color(muted.opacity(0.7))
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(sub),
                )
                .into_any_element()
        } else {
            div()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .flex_1()
                .min_w_0()
                .child(self.label)
                .into_any_element()
        };

        div()
            .id(self.id)
            .h(row_height)
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
            .child(
                Icon::new(self.icon)
                    .with_size(ICON_SM)
                    .text_color(icon_color),
            )
            .child(label_el)
    }
}
