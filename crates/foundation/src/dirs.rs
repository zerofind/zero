//! Centralized directory paths for Zero
//!
//! All persistent data lives under `~/.zero/`:
//!
//! ```text
//! ~/.zero/
//!   settings.json           # UI settings (theme, sidebar, search roots)
//!   indexes/                # Per-root search indexes
//!   db/                     # ControlDb (etch WAL) + per-device caches
//!   logs/                   # Service daemon logs
//!   profiles/               # User profile overrides (file types, cleanup)
//!   todo-context            # Which todo file is currently open
//! ```

use std::path::PathBuf;
use std::sync::OnceLock;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Root data directory: `~/.zero/`
///
/// All Zero state — settings, indexes, databases, logs — lives here.
/// Returns `None` only if the home directory can't be determined.
pub fn data_dir() -> Option<&'static PathBuf> {
    Some(DATA_DIR.get_or_init(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".zero")
    }))
}

/// Search indexes: `~/.zero/indexes/`
pub fn indexes_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("indexes"))
}

/// Etch stores: `~/.zero/db/`
pub fn db_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("db"))
}

/// Service logs: `~/.zero/logs/`
pub fn logs_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("logs"))
}

/// User profile overrides: `~/.zero/profiles/`
pub fn profiles_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("profiles"))
}

/// UI settings file: `~/.zero/settings.json`
pub fn settings_path() -> Option<PathBuf> {
    data_dir().map(|d| d.join("settings.json"))
}

/// Todo context file: `~/.zero/todo-context`
pub fn todo_context_path() -> Option<PathBuf> {
    data_dir().map(|d| d.join("todo-context"))
}

/// Telemetry state file: `~/.zero/telemetry.json`
pub fn telemetry_path() -> Option<PathBuf> {
    data_dir().map(|d| d.join("telemetry.json"))
}

/// Code indexes: `~/.zero/code/`
pub fn code_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("code"))
}
