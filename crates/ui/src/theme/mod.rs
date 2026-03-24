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
    // First theme set is always the built-in Zero theme
    #[allow(clippy::indexing_slicing)]
    apply_theme_set(&sets[0], cx);

    cx.set_global(ThemeStore { sets });

    // Apply saved user preference (dual-theme model)
    let settings = crate::session::Settings::load();
    apply_dual_themes(
        &settings.light_theme,
        &settings.dark_theme,
        &settings.theme_mode,
        cx,
    );
}

/// Apply separate light and dark themes, then activate the given mode.
pub fn apply_dual_themes(light_name: &str, dark_name: &str, mode_pref: &str, cx: &mut gpui::App) {
    let (light_config, dark_config) = {
        let store = cx.global::<ThemeStore>();

        let light = store
            .sets
            .iter()
            .find(|s| s.name.as_ref() == light_name)
            .and_then(|set| {
                set.themes
                    .iter()
                    .find(|t| t.mode == ThemeMode::Light)
                    .or(set.themes.first())
            })
            .cloned();

        let dark = store
            .sets
            .iter()
            .find(|s| s.name.as_ref() == dark_name)
            .and_then(|set| {
                set.themes
                    .iter()
                    .find(|t| t.mode == ThemeMode::Dark)
                    .or(set.themes.first())
            })
            .cloned();

        (light, dark)
    };

    let theme = Theme::global_mut(cx);
    if let Some(config) = light_config {
        theme.light_theme = Rc::new(config);
    }
    if let Some(config) = dark_config {
        theme.dark_theme = Rc::new(config);
    }

    let mode = match mode_pref {
        "light" => ThemeMode::Light,
        "system" => detect_system_theme(),
        _ => ThemeMode::Dark,
    };
    Theme::change(mode, None, cx);
}

/// Returns theme names that have a variant matching the given mode.
pub fn theme_names_for_mode(mode: ThemeMode, cx: &gpui::App) -> Vec<String> {
    let store = cx.global::<ThemeStore>();
    THEME_NAMES
        .iter()
        .filter(|name| {
            store
                .sets
                .iter()
                .find(|s| s.name.as_ref() == **name)
                .is_some_and(|set| set.themes.iter().any(|t| t.mode == mode))
        })
        .map(std::string::ToString::to_string)
        .collect()
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
    let dir =
        dirs::config_dir().map_or_else(|| PathBuf::from(""), |d| d.join("zero").join("themes"));

    if dir.as_os_str().is_empty() {
        return;
    }

    let _ = std::fs::create_dir_all(&dir);
    if let Err(e) = ThemeRegistry::watch_dir(dir, cx, |_| {}) {
        eprintln!("theme watch failed: {e}");
    }
}
