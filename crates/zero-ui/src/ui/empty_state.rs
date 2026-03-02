use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{v_flex, ActiveTheme, Icon, IconName, Sizable as _};

use crate::theme::{FONT_SIZE_BODY, FONT_SIZE_CALLOUT};

/// Centered icon + message for empty views.
#[derive(IntoElement)]
pub struct EmptyState {
    icon: IconName,
    title: SharedString,
    subtitle: Option<SharedString>,
}

impl EmptyState {
    pub fn new(icon: IconName, title: impl Into<SharedString>) -> Self {
        Self {
            icon,
            title: title.into(),
            subtitle: None,
        }
    }

    pub fn subtitle(mut self, text: impl Into<SharedString>) -> Self {
        self.subtitle = Some(text.into());
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                Icon::new(self.icon)
                    .large()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .text_size(FONT_SIZE_CALLOUT)
                    .text_color(cx.theme().muted_foreground)
                    .child(self.title),
            )
            .when_some(self.subtitle, |el, sub| {
                el.child(
                    div()
                        .text_size(FONT_SIZE_BODY)
                        .text_color(cx.theme().muted_foreground)
                        .child(sub),
                )
            })
    }
}
