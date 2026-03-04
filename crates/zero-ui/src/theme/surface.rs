use gpui::Hsla;
use gpui_component::ActiveTheme;

/// Interactive surface states — matches Swift SurfaceState
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SurfaceState {
    /// Clear background
    Rest,
    /// primary @ 5% — used for pinned/filled items
    RestFilled,
    /// primary @ 8% (dark) / 5% (light)
    Hover,
    /// primary @ 12% (dark) / 8% (light)
    Active,
    /// sky @ 25% — accent-tinted focus ring
    Focused,
}

/// Returns the fill color for a given surface state.
#[allow(dead_code)]
pub fn surface_fill(state: SurfaceState, cx: &gpui::App) -> Hsla {
    match state {
        SurfaceState::Rest => gpui::transparent_black(),
        SurfaceState::RestFilled => cx.theme().primary.opacity(0.05),
        SurfaceState::Hover => surface_hover(cx),
        SurfaceState::Active => surface_active(cx),
        SurfaceState::Focused => cx.theme().selection.opacity(0.25),
    }
}

/// Border color for surfaces (currently transparent for all states).
#[allow(dead_code)]
pub fn surface_border(_state: SurfaceState) -> Hsla {
    gpui::transparent_black()
}

// -- Convenience aliases (preserve existing API) ------------------------------

pub fn sidebar_bg(cx: &gpui::App) -> Hsla {
    cx.theme().sidebar
}

pub fn content_bg(cx: &gpui::App) -> Hsla {
    cx.theme().background
}

pub fn surface_hover(cx: &gpui::App) -> Hsla {
    cx.theme().list_hover
}

pub fn surface_active(cx: &gpui::App) -> Hsla {
    cx.theme().list_active
}

#[allow(dead_code)]
pub fn brand_active_bg(cx: &gpui::App) -> Hsla {
    cx.theme().primary.opacity(0.12)
}

/// Status bar background — subtle tinted bar for directory stats.
/// Matches the SwiftUI summary row tint.
pub fn status_bar_bg(cx: &gpui::App) -> Hsla {
    cx.theme().secondary
}

/// Banner background — matches Swift DS.Color.bannerBackground
/// dark: success tint @ 8%, light: black @ 6%
pub fn banner_bg(cx: &gpui::App) -> Hsla {
    if cx.theme().is_dark() {
        cx.theme().success.opacity(0.08)
    } else {
        gpui::black().opacity(0.06)
    }
}
