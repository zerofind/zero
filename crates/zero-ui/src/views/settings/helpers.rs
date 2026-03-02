use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{h_flex, v_flex, ActiveTheme, Icon, IconName, Sizable as _};

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION};

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
                .gap(px(2.0))
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

pub fn render_theme_picker(
    cx: &App,
    theme_mode: &str,
    on_light: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_dark: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_system: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let pill_bg = cx.theme().secondary;
    let is_dark = cx.theme().is_dark();
    let is_system = theme_mode == "system";

    h_flex()
        .rounded(px(100.0))
        .bg(pill_bg)
        .p(px(3.0))
        .gap(px(2.0))
        .child(theme_option("Light", IconName::Sun, !is_dark && !is_system, on_light, cx))
        .child(theme_option("Dark", IconName::Moon, is_dark && !is_system, on_dark, cx))
        .child(theme_option("System", IconName::Settings, is_system, on_system, cx))
        .into_any_element()
}

fn theme_option(
    label: &str,
    icon: IconName,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let (bg, text_color, weight) = if active {
        (
            cx.theme().list_active,
            cx.theme().foreground,
            FontWeight::SEMIBOLD,
        )
    } else {
        (
            gpui::transparent_black(),
            cx.theme().muted_foreground,
            FontWeight::NORMAL,
        )
    };

    h_flex()
        .id(SharedString::from(format!("theme-{label}")))
        .cursor_pointer()
        .rounded(px(100.0))
        .bg(bg)
        .px(px(12.0))
        .py(px(4.0))
        .gap(px(5.0))
        .items_center()
        .child(Icon::new(icon).xsmall().text_color(text_color))
        .child(
            div()
                .text_size(FONT_SIZE_CAPTION)
                .font_weight(weight)
                .text_color(text_color)
                .child(label.to_string()),
        )
        .on_click(on_click)
}
