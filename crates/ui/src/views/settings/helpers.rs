use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::v_flex;

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, SPACE_XS};

pub fn card_border(cx: &App) -> Hsla {
    theme::card_border(cx)
}

pub fn setting_row(
    title: &str,
    description: &str,
    control: AnyElement,
    label_color: Hsla,
    value_color: Hsla,
    border_color: Hsla,
    has_border_bottom: bool,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_5()
        .py_4()
        .gap_6()
        .when(has_border_bottom, |el| {
            el.border_b_1().border_color(border_color)
        })
        .child(
            v_flex()
                .gap(SPACE_XS)
                .flex_1()
                .child(
                    div()
                        .text_size(FONT_SIZE_BODY)
                        .text_color(value_color)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(FONT_SIZE_CAPTION)
                        .text_color(label_color)
                        .child(description.to_string()),
                ),
        )
        .child(div().flex_shrink_0().child(control))
}

pub fn group_label(text: &str, color: Hsla) -> Div {
    div()
        .text_size(FONT_SIZE_BODY)
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .child(text.to_string())
}
