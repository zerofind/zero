use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
};

use crate::theme::{RADIUS_LG, SPACE_LG, SPACE_MD};

pub struct ChatPrompt;

impl ChatPrompt {
    pub fn render(
        input: &Entity<InputState>,
        loading: bool,
        on_send: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        cx: &App,
    ) -> impl IntoElement {
        div().w_full().px(SPACE_LG).py(SPACE_MD).child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(SPACE_MD)
                .px(SPACE_LG)
                .py(px(6.0))
                .rounded(RADIUS_LG)
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().secondary)
                .child(
                    div()
                        .flex_1()
                        .child(Input::new(input).appearance(false).cleanable(false)),
                )
                .child(
                    Button::new("ask-send")
                        .primary()
                        .small()
                        .rounded(px(20.0))
                        .icon(IconName::ArrowUp)
                        .disabled(loading)
                        .on_click(on_send),
                ),
        )
    }
}
