use std::path::PathBuf;

use llm::LlmConfig;
use serde::{Deserialize, Serialize};

use crate::workspace::Workspace;

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

    /// Flat bookmark fields — only deserialized for migration to workspaces.
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    sidebar_bookmarks: Vec<PathBuf>,

    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    sidebar_regular_bookmarks: Vec<PathBuf>,

    #[serde(default)]
    pub search_roots: Vec<PathBuf>,

    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    last_path: Option<PathBuf>,

    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    view_mode: String,

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

    /// Visible columns in the file browser (serialized FileColumn names).
    #[serde(default)]
    pub visible_columns: Vec<String>,

    /// Whether the MCP server is enabled.
    #[serde(default)]
    pub mcp_enabled: bool,

    /// Port for the MCP HTTP server.
    #[serde(default = "default_mcp_port")]
    pub mcp_port: u16,

    /// LLM agent configuration.
    #[serde(default)]
    pub llm: LlmConfig,

    /// Arc-style workspaces — each has its own bookmarks and last path.
    #[serde(default)]
    pub workspaces: Vec<Workspace>,

    /// Index of the currently active workspace.
    #[serde(default)]
    pub active_workspace: usize,
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

fn default_true() -> bool {
    true
}

fn default_mcp_port() -> u16 {
    45557
}

impl Default for Settings {
    fn default() -> Self {
        let pinned = default_bookmarks();
        Self {
            theme_mode: default_theme_mode(),
            light_theme: default_light_theme(),
            dark_theme: default_dark_theme(),
            sidebar_open: true,
            sidebar_bookmarks: Vec::new(),
            sidebar_regular_bookmarks: Vec::new(),
            search_roots: Vec::new(),
            last_path: None,
            view_mode: String::new(),
            search_include_trash: false,
            search_include_offline: false,
            dedup_use_checksums: default_true(),
            auto_update: default_true(),
            onboarding_complete: false,
            toolbar_visible: false,
            visible_columns: Vec::new(),
            mcp_enabled: false,
            mcp_port: default_mcp_port(),
            llm: LlmConfig::default(),
            workspaces: vec![Workspace::new_default(pinned, Vec::new())],
            active_workspace: 0,
        }
    }
}

pub(crate) fn default_bookmarks() -> Vec<PathBuf> {
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
        foundation::dirs::settings_path()
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let mut settings: Self = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        // Migration: if workspaces is empty, create a "Default" workspace
        // from the legacy flat bookmark fields. Existing users see no change.
        if settings.workspaces.is_empty() {
            settings.workspaces.push(Workspace::new_default(
                settings.sidebar_bookmarks.clone(),
                settings.sidebar_regular_bookmarks.clone(),
            ));
            settings.active_workspace = 0;
        }

        // Clamp active_workspace to valid range
        if settings.active_workspace >= settings.workspaces.len() {
            settings.active_workspace = 0;
        }

        settings
    }

    /// Returns a reference to the currently active workspace.
    #[allow(clippy::indexing_slicing)] // active_workspace always valid
    pub fn active_ws(&self) -> &Workspace {
        &self.workspaces[self.active_workspace]
    }

    /// Returns a mutable reference to the currently active workspace.
    #[allow(clippy::indexing_slicing)] // active_workspace always valid
    pub fn active_ws_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active_workspace]
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
