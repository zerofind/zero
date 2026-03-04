use gpui::{Hsla, hsla};
use gpui_component::ActiveTheme;

// -- Brand colors -------------------------------------------------------------

/// Primary accent color — reads from the active theme.
pub fn brand_color(cx: &gpui::App) -> Hsla {
    cx.theme().primary
}

/// Selection accent — reads from the active theme.
pub fn selection_color(cx: &gpui::App) -> Hsla {
    cx.theme().selection
}

pub fn selection_active_bg(cx: &gpui::App) -> Hsla {
    cx.theme().list_active
}

// -- Semantic aliases (matching Swift DS.Color) --------------------------------

/// Success color — reads from the active theme.
pub fn success_color(cx: &gpui::App) -> Hsla {
    cx.theme().success
}

/// Warning color — reads from the active theme.
pub fn warning_color(cx: &gpui::App) -> Hsla {
    cx.theme().warning
}

/// Error color — reads from the active theme.
pub fn error_color(cx: &gpui::App) -> Hsla {
    cx.theme().danger
}

// -- Alert-level colors -------------------------------------------------------

pub fn alert_info(cx: &gpui::App) -> Hsla {
    cx.theme().info
}

pub fn alert_success(cx: &gpui::App) -> Hsla {
    cx.theme().success
}

pub fn alert_warning(cx: &gpui::App) -> Hsla {
    cx.theme().warning
}

pub fn alert_error(cx: &gpui::App) -> Hsla {
    cx.theme().danger
}

// -- Overlay / backdrop -------------------------------------------------------

/// Semi-transparent backdrop for modals and overlays
pub fn overlay_backdrop(cx: &gpui::App) -> Hsla {
    cx.theme().overlay
}

/// Popover / dialog background
pub fn popover_bg(cx: &gpui::App) -> Hsla {
    cx.theme().popover
}

/// Toast notification background (slightly transparent)
pub fn toast_bg(cx: &gpui::App) -> Hsla {
    let p = cx.theme().popover;
    hsla(p.h, p.s, p.l, 0.95)
}

/// Card border for settings cards and similar containers
pub fn card_border(cx: &gpui::App) -> Hsla {
    cx.theme().border
}
