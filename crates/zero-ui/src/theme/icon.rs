use gpui::{px, Hsla, Pixels};
use gpui_component::ActiveTheme as _;

use super::selection_color;

/// Icon interactive states — matches Swift IconState
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum IconState {
    /// Muted foreground (secondary)
    Default,
    /// Full foreground (primary)
    Hover,
    /// Accent-tinted (selection blue)
    Active,
    /// Foreground at 25% opacity
    Disabled,
}

/// Returns the color for a given icon state.
#[allow(dead_code)]
pub fn icon_color(state: IconState, cx: &gpui::App) -> Hsla {
    match state {
        IconState::Default => cx.theme().muted_foreground,
        IconState::Hover => cx.theme().foreground,
        IconState::Active => selection_color(),
        IconState::Disabled => {
            let fg = cx.theme().foreground;
            Hsla {
                a: fg.a * 0.25,
                ..fg
            }
        }
    }
}

// -- Standard icon sizes (matching Swift InterfaceIcon / AppIcon) -------------

#[allow(dead_code)]
pub const ICON_SM: Pixels = px(14.0);
#[allow(dead_code)]
pub const ICON_MD: Pixels = px(16.0);
#[allow(dead_code)]
pub const ICON_LG: Pixels = px(18.0);

#[allow(dead_code)]
pub const APP_ICON_SM: Pixels = px(16.0);
#[allow(dead_code)]
pub const APP_ICON_MD: Pixels = px(24.0);
#[allow(dead_code)]
pub const APP_ICON_LG: Pixels = px(32.0);
