use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _, StyledExt as _,
    clipboard::Clipboard,
    text::{TextView, TextViewStyle},
};

use crate::theme::{
    FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS, RADIUS_SM, SPACE_MD, SPACE_SM, SPACE_XS,
};

// -- Tool call ---------------------------------------------------------------

#[derive(Clone)]
pub struct ToolCall {
    pub name: String,
    pub done: bool,
}

// -- User bubble -------------------------------------------------------------

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

// -- Tool call card ----------------------------------------------------------

/// A single tool call rendered as a compact card with left-border accent.
#[derive(IntoElement)]
struct ToolCallCard {
    name: SharedString,
    done: bool,
}

impl ToolCallCard {
    fn new(tool: &ToolCall) -> Self {
        Self {
            name: SharedString::from(tool.name.clone()),
            done: tool.done,
        }
    }
}

impl RenderOnce for ToolCallCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let accent = if self.done {
            cx.theme().primary.opacity(0.5)
        } else {
            cx.theme().primary
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(SPACE_SM)
            .pl(SPACE_MD)
            .pr(SPACE_MD)
            .py(SPACE_XS)
            .border_l_2()
            .border_color(accent)
            .rounded_r(RADIUS_SM)
            .bg(cx.theme().secondary)
            .child(
                Icon::new(if self.done {
                    IconName::CircleCheck
                } else {
                    IconName::Loader
                })
                .with_size(px(12.0))
                .text_color(if self.done { muted } else { accent }),
            )
            .child(
                div()
                    .text_size(FONT_SIZE_CAPTION)
                    .text_color(muted)
                    .child(self.name),
            )
    }
}

// -- Streaming cursor --------------------------------------------------------

/// Blinking block cursor shown at end of streaming text.
#[derive(IntoElement)]
pub struct StreamingCursor;

impl RenderOnce for StreamingCursor {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .w(px(2.0))
            .h(px(14.0))
            .rounded(px(1.0))
            .bg(cx.theme().primary)
    }
}

// -- Assistant message -------------------------------------------------------

/// Left-aligned assistant message with markdown, tool cards, and copy button.
#[derive(IntoElement)]
pub struct AssistantMessage {
    text: SharedString,
    tools: Vec<ToolCall>,
    msg_index: usize,
    streaming: bool,
}

impl AssistantMessage {
    pub fn new(text: impl Into<SharedString>, tools: Vec<ToolCall>) -> Self {
        Self {
            text: text.into(),
            tools,
            msg_index: 0,
            streaming: false,
        }
    }

    pub fn index(mut self, idx: usize) -> Self {
        self.msg_index = idx;
        self
    }

    pub fn streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }
}

impl RenderOnce for AssistantMessage {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let has_text = !self.text.is_empty();
        let has_tools = !self.tools.is_empty();
        let is_dark = cx.theme().is_dark();

        let style = TextViewStyle::default()
            .paragraph_gap(rems(0.6))
            .code_block(gpui::StyleRefinement::default().corner_radii(RADIUS_SM.into()));
        let style = TextViewStyle { is_dark, ..style };

        div()
            .id(SharedString::from(format!(
                "assistant-msg-{}",
                self.msg_index
            )))
            .group("assistant-msg")
            .relative()
            .w_full()
            .py(SPACE_SM)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(SPACE_SM)
                    // Tool call cards
                    .when(has_tools, |el| {
                        el.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(SPACE_XS)
                                .children(self.tools.iter().map(ToolCallCard::new)),
                        )
                    })
                    // Markdown content
                    .when(has_text, |el| {
                        el.child(
                            div()
                                .flex()
                                .flex_row()
                                .items_end()
                                .gap(SPACE_XS)
                                .child(
                                    TextView::markdown(
                                        ElementId::Name(
                                            format!("msg-md-{}", self.msg_index).into(),
                                        ),
                                        self.text.clone(),
                                        window,
                                        cx,
                                    )
                                    .style(style)
                                    .text_size(FONT_SIZE_BODY)
                                    .selectable(true)
                                    .code_block_actions(
                                        |code_block, _window, _cx| {
                                            Clipboard::new(SharedString::from(format!(
                                                "copy-code-{}",
                                                code_block.code().len()
                                            )))
                                            .value(code_block.code())
                                        },
                                    ),
                                )
                                .when(self.streaming, |el| el.child(StreamingCursor)),
                        )
                    })
                    // Streaming cursor when no text yet
                    .when(self.streaming && !has_text && !has_tools, |el| {
                        el.child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(SPACE_SM)
                                .child(StreamingCursor)
                                .child(
                                    div()
                                        .text_size(FONT_SIZE_CAPTION)
                                        .text_color(cx.theme().muted_foreground)
                                        .child("Thinking..."),
                                ),
                        )
                    }),
            )
            // Copy button (hover-reveal, top-right)
            .when(has_text && !self.streaming, |el| {
                el.child(
                    div()
                        .absolute()
                        .top(SPACE_SM)
                        .right(px(0.0))
                        .opacity(0.0)
                        .group_hover("assistant-msg", |s| s.opacity(1.0))
                        .on_mouse_down(MouseButton::Left, |_, _, _| {})
                        .child(
                            Clipboard::new(SharedString::from(format!(
                                "copy-msg-{}",
                                self.msg_index
                            )))
                            .value(self.text.clone()),
                        ),
                )
            })
    }
}
