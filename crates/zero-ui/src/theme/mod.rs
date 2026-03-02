#[cfg(test)]
mod mod_test;

mod color;
mod container;
mod icon;
mod layout;
mod shadow;
mod surface;

// Re-export everything so `use crate::theme::*` continues to work.
pub use color::*;
#[allow(unused_imports)]
pub use container::*;
#[allow(unused_imports)]
pub use icon::*;
pub use layout::*;
#[allow(unused_imports)]
pub use shadow::*;
pub use surface::*;

use std::path::PathBuf;
use std::rc::Rc;

use gpui::{Pixels, px};
use gpui_component::theme::{Theme, ThemeMode, ThemeRegistry, ThemeSet};

// -- Zero theme JSON ----------------------------------------------------------

pub const ZERO_THEME: &str = include_str!("./zero.json");

// -- Typography ---------------------------------------------------------------

pub const FONT_SIZE_CAPTION: Pixels = px(11.0);
pub const FONT_SIZE_BODY: Pixels = px(13.0);
#[allow(dead_code)] // Used by design-system binary via lib crate
pub const FONT_SIZE_CALLOUT: Pixels = px(14.0);
#[allow(dead_code)] // Used by design-system binary via lib crate
pub const FONT_SIZE_TITLE: Pixels = px(16.0);

// -- Corner radii -------------------------------------------------------------
// Radius scale: SM(6) < DEFAULT(8) < MD(10) < LG(12)
// DEFAULT is the most-used; SM/MD/LG match Swift DS.Radius.small/medium/large

pub const RADIUS_SM: Pixels = px(6.0);
pub const RADIUS: Pixels = px(8.0);
#[allow(dead_code)]
pub const RADIUS_MD: Pixels = px(10.0);
pub const RADIUS_LG: Pixels = px(12.0);

// -- Spacing ------------------------------------------------------------------

#[allow(dead_code)]
pub const PADDING_SM: Pixels = px(4.0);
pub const PADDING_MD: Pixels = px(8.0);
pub const PADDING_LG: Pixels = px(12.0);

// -- Load Zero theme into the global Theme ------------------------------------

/// Parse zero.json and register both dark/light variants as the default themes.
/// Call this once at startup, before `Theme::change()`.
pub fn init_zero_theme(cx: &mut gpui::App) {
    let set: ThemeSet =
        serde_json::from_str(ZERO_THEME).expect("zero.json must be valid ThemeSet JSON");

    let theme = Theme::global_mut(cx);
    for config in &set.themes {
        let rc = Rc::new(config.clone());
        match config.mode {
            ThemeMode::Dark => theme.dark_theme = rc,
            ThemeMode::Light => theme.light_theme = rc,
        }
    }
}

/// Watch `~/.config/zero/themes/` for user-supplied theme JSON files.
/// Any `.json` file dropped there is auto-loaded via `ThemeRegistry`.
pub fn watch_user_themes(cx: &mut gpui::App) {
    let dir = dirs::config_dir()
        .map(|d| d.join("zero").join("themes"))
        .unwrap_or_else(|| PathBuf::from(""));

    if dir.as_os_str().is_empty() {
        return;
    }

    let _ = std::fs::create_dir_all(&dir);
    if let Err(e) = ThemeRegistry::watch_dir(dir, cx, |_| {}) {
        eprintln!("theme watch failed: {e}");
    }
}

// -- Legacy branding (no-op, kept for compile compat during migration) --------

/// Deprecated: colors now come from zero.json via `init_zero_theme()`.
#[allow(dead_code)]
pub fn apply_branding(_cx: &mut gpui::App) {
    // Intentionally empty — all branding is in zero.json.
}
