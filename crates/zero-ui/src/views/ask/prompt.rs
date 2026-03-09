use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
};

use crate::theme::{FONT_SIZE_CAPTION, RADIUS, SPACE_MD, SPACE_SM};

pub struct ChatPrompt;

impl ChatPrompt {
    /// Render the two-part prompt area: input on top, toolbar below.
    ///
    /// `model_name`     — display name for the active model (empty if not ready).
    /// `on_model_click` — fires when the user clicks the model label.
    /// `on_send`        — fires when the user clicks the send button.
    pub fn render(
        input: &Entity<InputState>,
        loading: bool,
        model_name: &str,
        on_model_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        on_send: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        cx: &App,
    ) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let model_label = SharedString::from(model_name.to_string());

        div()
            .w_full()
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(cx.theme().border)
            // Input area — taller, no visible border
            .child(
                div()
                    .w_full()
                    .px(SPACE_MD)
                    .pt(SPACE_MD)
                    .child(Input::new(input).appearance(false).cleanable(false)),
            )
            // Toolbar row — model selector + send button
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px(SPACE_MD)
                    .py(SPACE_SM)
                    // Model selector (left side)
                    .child(
                        div()
                            .id("prompt-model-selector")
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(3.0))
                            .cursor_pointer()
                            .rounded(RADIUS)
                            .px(px(6.0))
                            .py(px(2.0))
                            .hover(|s| s.bg(cx.theme().secondary))
                            .on_click(on_model_click)
                            .when(!model_label.is_empty(), |el| {
                                el.child(
                                    div()
                                        .text_size(FONT_SIZE_CAPTION)
                                        .text_color(muted)
                                        .child(model_label),
                                )
                                .child(
                                    Icon::new(IconName::ChevronDown)
                                        .with_size(px(10.0))
                                        .text_color(muted),
                                )
                            }),
                    )
                    // Send button (right side)
                    .child(
                        Button::new("ask-send")
                            .ghost()
                            .small()
                            .icon(IconName::ArrowUp)
                            .disabled(loading)
                            .on_click(on_send),
                    ),
            )
    }
}
