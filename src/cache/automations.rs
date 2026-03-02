//! Automation types for sync rules
//!
//! Automations define what to sync, where, and when. They connect triggers
//! (USB mount, file changes, schedules) to sync actions.
//! The actual persistence is handled by ControlState (etch-backed).

use serde::{Deserialize, Serialize};

/// Triggers that can start an automation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Triggers {
    /// Trigger when the destination USB device mounts
    #[serde(default)]
    pub on_mount: bool,

    /// Trigger when source files change
    #[serde(default)]
    pub on_change: bool,

    /// Trigger on a schedule (cron format, future)
    #[serde(default)]
    pub on_schedule: Option<String>,
}

/// A path mapping within an automation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathMapping {
    /// Source path on local machine
    pub source: String,

    /// Destination subpath on target (relative to mount point)
    pub dest: String,

    /// Patterns to exclude (glob format)
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Settings for how the sync should run
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Verify checksums after copy
    #[serde(default = "default_true")]
    pub verify: bool,

    /// Delete files in destination that don't exist in source
    #[serde(default)]
    pub delete_orphans: bool,

    /// Show notification when complete
    #[serde(default = "default_true")]
    pub notify: bool,

    /// Debounce delay for file change triggers (milliseconds)
    /// How long to wait after a file change before syncing
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
}

fn default_true() -> bool {
    true
}

fn default_debounce_ms() -> u64 {
    5000 // 5 seconds
}

/// An automation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Automation {
    pub id: i64,
    pub name: String,

    /// USB device serial (None if not USB-based)
    pub dest_device_serial: Option<String>,

    /// Volume name for display
    pub dest_volume_name: Option<String>,

    /// Direct destination path (for non-USB destinations)
    pub dest_path: Option<String>,

    /// When to trigger
    pub triggers: Triggers,

    /// What to sync
    pub paths: Vec<PathMapping>,

    /// How to sync
    pub settings: Settings,

    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Input for creating a new automation
#[derive(Debug, Clone)]
pub struct NewAutomation {
    pub name: String,
    pub dest_device_serial: Option<String>,
    pub dest_volume_name: Option<String>,
    pub dest_path: Option<String>,
    pub triggers: Triggers,
    pub paths: Vec<PathMapping>,
    pub settings: Settings,
}
