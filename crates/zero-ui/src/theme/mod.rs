#[cfg(test)]
mod mod_test;

mod color;
mod surface;
mod tokens;

pub use color::*;
pub use surface::*;
pub use tokens::*;

use std::path::PathBuf;
use std::rc::Rc;

use gpui::Global;
use gpui_component::theme::{Theme, ThemeMode, ThemeRegistry, ThemeSet};

// -- Theme JSON sources -------------------------------------------------------

pub const ZERO_THEME: &str = include_str!("./zero.json");
const CATPPUCCIN_LATTE: &str = include_str!("./catppuccin-latte.json");
const CATPPUCCIN_FRAPPE: &str = include_str!("./catppuccin-frappe.json");
const CATPPUCCIN_MACCHIATO: &str = include_str!("./catppuccin-macchiato.json");
const CATPPUCCIN_MOCHA: &str = include_str!("./catppuccin-mocha.json");

/// All available theme names in display order.
pub const THEME_NAMES: &[&str] = &[
    "Zero",
    "Catppuccin Latte",
    "Catppuccin Frappe",
    "Catppuccin Macchiato",
    "Catppuccin Mocha",
];

// -- Theme store (global) -----------------------------------------------------

struct ThemeStore {
    sets: Vec<ThemeSet>,
}

impl Global for ThemeStore {}

// -- Init + switching ---------------------------------------------------------

/// Parse all built-in themes and apply the saved user preference.
/// Call once at startup before the first render.
pub fn init_zero_theme(cx: &mut gpui::App) {
    let jsons = [
        ZERO_THEME,
        CATPPUCCIN_LATTE,
        CATPPUCCIN_FRAPPE,
        CATPPUCCIN_MACCHIATO,
        CATPPUCCIN_MOCHA,
    ];

    let sets: Vec<ThemeSet> = jsons
        .iter()
        .map(|j| serde_json::from_str(j).expect("theme JSON must be valid"))
        .collect();

    // Apply default (Zero) immediately
    apply_theme_set(&sets[0], cx);

    cx.set_global(ThemeStore { sets });

    // Apply saved user preference
    let settings = crate::session::Settings::load();
    apply_named_theme(&settings.theme, &settings.theme_mode, cx);
}

/// Switch to a named theme. Finds the matching ThemeSet and applies it.
pub fn apply_named_theme(name: &str, mode_pref: &str, cx: &mut gpui::App) {
    let configs: Option<Vec<_>> = {
        let store = cx.global::<ThemeStore>();
        store
            .sets
            .iter()
            .find(|s| s.name.as_ref() == name)
            .map(|set| set.themes.clone())
    };

    if let Some(configs) = configs {
        let theme = Theme::global_mut(cx);
        for config in configs {
            let rc = Rc::new(config.clone());
            match config.mode {
                ThemeMode::Dark => theme.dark_theme = rc,
                ThemeMode::Light => theme.light_theme = rc,
            }
        }
    }

    let mode = match mode_pref {
        "light" => ThemeMode::Light,
        "system" => detect_system_theme(),
        _ => ThemeMode::Dark,
    };
    Theme::change(mode, None, cx);
}

/// Returns the forced ThemeMode for single-mode themes, or None for dual-mode.
pub fn forced_mode_for_theme(name: &str, cx: &gpui::App) -> Option<ThemeMode> {
    let store = cx.global::<ThemeStore>();
    store
        .sets
        .iter()
        .find(|s| s.name.as_ref() == name)
        .filter(|set| set.themes.len() == 1)
        .map(|set| set.themes[0].mode)
}

fn apply_theme_set(set: &ThemeSet, cx: &mut gpui::App) {
    let theme = Theme::global_mut(cx);
    for config in &set.themes {
        let rc = Rc::new(config.clone());
        match config.mode {
            ThemeMode::Dark => theme.dark_theme = rc,
            ThemeMode::Light => theme.light_theme = rc,
        }
    }
}

/// Detect macOS system appearance preference.
pub fn detect_system_theme() -> ThemeMode {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output();
        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.trim().eq_ignore_ascii_case("dark") {
                return ThemeMode::Dark;
            }
        }
        ThemeMode::Light
    }
    #[cfg(not(target_os = "macos"))]
    {
        ThemeMode::Dark
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
