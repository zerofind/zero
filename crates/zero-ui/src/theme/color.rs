use gpui::{hsla, Hsla};
use gpui_component::ActiveTheme;

// -- Brand colors -------------------------------------------------------------

/// Zero coral accent — #FE764B
pub fn brand_color() -> Hsla {
    hsla(0.033, 0.99, 0.65, 1.0)
}

/// Selection blue — #0074E9
pub fn selection_color() -> Hsla {
    hsla(0.590, 0.90, 0.52, 1.0)
}

pub fn selection_active_bg(cx: &gpui::App) -> Hsla {
    cx.theme().selection.opacity(0.15)
}

/// Success mint — #61DFB8
pub fn success_color() -> Hsla {
    hsla(0.440, 0.68, 0.63, 1.0)
}

/// Positive lime — #B7FA7B
#[allow(dead_code)]
pub fn positive_color() -> Hsla {
    hsla(0.260, 0.93, 0.73, 1.0)
}

// -- Semantic aliases (matching Swift DS.Color) --------------------------------

/// Warning color — coral (same as brand)
#[allow(dead_code)]
pub fn warning_color() -> Hsla {
    brand_color()
}

/// Error color — red
#[allow(dead_code)]
pub fn error_color() -> Hsla {
    hsla(0.0, 0.75, 0.55, 1.0)
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
