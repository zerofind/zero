use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Persisted user settings — survives across app restarts.
#[derive(Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_theme_mode")]
    pub theme_mode: String,

    #[serde(default = "default_light_theme")]
    pub light_theme: String,

    #[serde(default = "default_dark_theme")]
    pub dark_theme: String,

    #[serde(default = "default_true")]
    pub sidebar_open: bool,

    #[serde(default)]
    pub sidebar_bookmarks: Vec<PathBuf>,

    /// Non-pinned bookmarks shown as rows in the BOOKMARKS section.
    #[serde(default)]
    pub sidebar_regular_bookmarks: Vec<PathBuf>,

    #[serde(default)]
    pub search_roots: Vec<PathBuf>,

    #[serde(default)]
    pub last_path: Option<PathBuf>,

    #[serde(default = "default_view_mode")]
    pub view_mode: String,

    #[serde(default)]
    pub search_include_trash: bool,

    #[serde(default)]
    pub search_include_offline: bool,

    #[serde(default = "default_true")]
    pub dedup_use_checksums: bool,

    #[serde(default = "default_true")]
    pub auto_update: bool,

    #[serde(default)]
    pub onboarding_complete: bool,

    #[serde(default)]
    pub toolbar_visible: bool,
}

fn default_theme_mode() -> String {
    "system".to_string()
}

fn default_light_theme() -> String {
    "Catppuccin Latte".to_string()
}

fn default_dark_theme() -> String {
    "Catppuccin Mocha".to_string()
}

fn default_view_mode() -> String {
    "list".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme_mode: default_theme_mode(),
            light_theme: default_light_theme(),
            dark_theme: default_dark_theme(),
            sidebar_open: true,
            sidebar_bookmarks: default_bookmarks(),
            sidebar_regular_bookmarks: Vec::new(),
            search_roots: Vec::new(),
            last_path: None,
            view_mode: default_view_mode(),
            search_include_trash: false,
            search_include_offline: false,
            dedup_use_checksums: default_true(),
            auto_update: default_true(),
            onboarding_complete: false,
            toolbar_visible: false,
        }
    }
}

fn default_bookmarks() -> Vec<PathBuf> {
    let mut bookmarks = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for name in ["Desktop", "Documents", "Downloads"] {
            let p = home.join(name);
            if p.exists() {
                bookmarks.push(p);
            }
        }
    }
    bookmarks
}

impl Settings {
    /// Returns true if the path is a system-default bookmark that should not be removed.
    pub fn is_system_bookmark(path: &std::path::Path) -> bool {
        let Some(home) = dirs::home_dir() else {
            return false;
        };
        ["Desktop", "Documents", "Downloads"]
            .iter()
            .any(|name| path == home.join(name))
    }

    fn path() -> Option<PathBuf> {
        zero::dirs::settings_path()
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}
