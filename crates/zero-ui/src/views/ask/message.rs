use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::theme::{FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS, SPACE_SM};

#[derive(Clone)]
pub struct ToolCall {
    pub name: String,
    pub done: bool,
}

/// Right-aligned user message bubble.
#[derive(IntoElement)]
pub struct UserBubble {
    text: SharedString,
}

impl UserBubble {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into() }
    }
}

impl RenderOnce for UserBubble {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div().w_full().flex().flex_row_reverse().py(SPACE_SM).child(
            div()
                .max_w_3_4()
                .px(px(12.0))
                .py(px(8.0))
                .rounded(RADIUS)
                .bg(cx.theme().primary)
                .text_color(cx.theme().primary_foreground)
                .text_size(FONT_SIZE_BODY)
                .child(self.text),
        )
    }
}

/// Left-aligned assistant message with optional tool call indicators.
#[derive(IntoElement)]
pub struct AssistantMessage {
    text: SharedString,
    tools: Vec<ToolCall>,
}

impl AssistantMessage {
    pub fn new(text: impl Into<SharedString>, tools: Vec<ToolCall>) -> Self {
        Self {
            text: text.into(),
            tools,
        }
    }
}

impl RenderOnce for AssistantMessage {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        div().w_full().py(SPACE_SM).child(
            div()
                .flex()
                .flex_col()
                .gap(SPACE_SM)
                .children(self.tools.iter().map(|tool| {
                    let label = if tool.done {
                        SharedString::from(format!("Called {}", tool.name))
                    } else {
                        SharedString::from(format!("{}...", tool.name))
                    };
                    div()
                        .text_size(FONT_SIZE_CAPTION)
                        .text_color(muted)
                        .child(label)
                }))
                .when(!self.text.is_empty(), |el| {
                    el.child(
                        div()
                            .text_size(FONT_SIZE_BODY)
                            .text_color(cx.theme().foreground)
                            .child(self.text),
                    )
                }),
        )
    }
}
