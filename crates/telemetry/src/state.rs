//! Telemetry state persistence
//!
//! Stores cumulative usage counters in `~/.zero/telemetry.json`.
//! Counters accumulate across CLI invocations and reset after a successful report.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::id::generate_install_id;
use foundation::VERSION;
use foundation::util::now_timestamp;

/// Cumulative telemetry state persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryState {
    // Identity
    pub install_id: String,
    pub enabled: bool,
    pub first_seen: i64,
    pub last_report: i64,
    pub version: String,

    // Environment
    pub os: String,
    pub arch: String,
    pub cpu_cores: usize,

    // Sync
    pub sync_runs: u64,
    pub sync_files: u64,
    pub sync_bytes: u64,
    pub sync_mirror_used: bool,
    pub sync_verify_used: bool,
    pub sync_cloud_used: bool,
    pub sync_cloud_backends: BTreeSet<String>,

    // Search
    pub search_queries: u64,
    pub search_index_files: u64,
    pub search_watch_used: bool,

    // Dedup
    pub dedup_runs: u64,
    pub dedup_found: u64,
    pub dedup_deleted_bytes: u64,

    // Cleanup
    pub cleanup_runs: u64,
    pub cleanup_categories: BTreeSet<String>,
    pub cleanup_reclaimed_bytes: u64,

    // Erase
    pub erase_runs: u64,
    pub erase_levels: BTreeSet<String>,

    // AI
    pub ai_queries: u64,
    pub ai_provider: Option<String>,

    // UI
    pub ui_launched: bool,

    // Commands
    pub commands_used: BTreeSet<String>,
    pub errors_total: u64,
}

impl TelemetryState {
    /// Create a new state with defaults (enabled, zero counters)
    pub fn new() -> Self {
        let now = now_timestamp();
        Self {
            install_id: generate_install_id(),
            enabled: true,
            first_seen: now,
            last_report: now,
            version: VERSION.to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu_cores: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
            sync_runs: 0,
            sync_files: 0,
            sync_bytes: 0,
            sync_mirror_used: false,
            sync_verify_used: false,
            sync_cloud_used: false,
            sync_cloud_backends: BTreeSet::new(),
            search_queries: 0,
            search_index_files: 0,
            search_watch_used: false,
            dedup_runs: 0,
            dedup_found: 0,
            dedup_deleted_bytes: 0,
            cleanup_runs: 0,
            cleanup_categories: BTreeSet::new(),
            cleanup_reclaimed_bytes: 0,
            erase_runs: 0,
            erase_levels: BTreeSet::new(),
            ai_queries: 0,
            ai_provider: None,
            ui_launched: false,
            commands_used: BTreeSet::new(),
            errors_total: 0,
        }
    }

    /// Load state from file. Returns `(state, is_new)`.
    pub fn load(path: &Path) -> (Self, bool) {
        if let Ok(contents) = fs::read_to_string(path)
            && let Ok(mut state) = serde_json::from_str::<Self>(&contents)
        {
            // Update version on upgrade
            let current = VERSION;
            if state.version != current {
                state.version = current.to_string();
            }
            return (state, false);
        }
        (Self::new(), true)
    }

    /// Save state to file
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json =
            serde_json::to_string_pretty(self).map_err(|e| std::io::Error::other(e.to_string()))?;
        fs::write(path, json)
    }

    /// Reset cumulative counters after a successful report
    pub fn reset_counters(&mut self) {
        self.sync_runs = 0;
        self.sync_files = 0;
        self.sync_bytes = 0;
        self.sync_mirror_used = false;
        self.sync_verify_used = false;
        self.sync_cloud_used = false;
        self.sync_cloud_backends.clear();
        self.search_queries = 0;
        self.search_index_files = 0;
        self.search_watch_used = false;
        self.dedup_runs = 0;
        self.dedup_found = 0;
        self.dedup_deleted_bytes = 0;
        self.cleanup_runs = 0;
        self.cleanup_categories.clear();
        self.cleanup_reclaimed_bytes = 0;
        self.erase_runs = 0;
        self.erase_levels.clear();
        self.ai_queries = 0;
        self.ai_provider = None;
        self.ui_launched = false;
        self.commands_used.clear();
        self.errors_total = 0;
        self.last_report = now_timestamp();
        self.version = VERSION.to_string();
    }

    /// Check if a report is due (7 days since last, 24h grace after install)
    pub fn should_report(&self) -> bool {
        let now = now_timestamp();
        let seven_days = 7 * 24 * 60 * 60;
        let one_day = 24 * 60 * 60;

        // Grace period: don't report within 24h of first install
        if now - self.first_seen < one_day {
            return false;
        }

        now - self.last_report >= seven_days
    }

    /// Build the JSON payload that would be sent
    pub fn to_payload(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "track",
            "event": "heartbeat",
            "userId": self.install_id,
            "timestamp": now_timestamp(),
            "properties": {
                "version": self.version,
                "os": self.os,
                "arch": self.arch,
                "cpu_cores": self.cpu_cores,
                "sync": {
                    "runs": self.sync_runs,
                    "files": self.sync_files,
                    "bytes": self.sync_bytes,
                    "mirror_used": self.sync_mirror_used,
                    "verify_used": self.sync_verify_used,
                    "cloud_used": self.sync_cloud_used,
                    "cloud_backends": self.sync_cloud_backends,
                },
                "search": {
                    "queries": self.search_queries,
                    "index_files": self.search_index_files,
                    "watch_used": self.search_watch_used,
                },
                "dedup": {
                    "runs": self.dedup_runs,
                    "found": self.dedup_found,
                    "deleted_bytes": self.dedup_deleted_bytes,
                },
                "cleanup": {
                    "runs": self.cleanup_runs,
                    "categories": self.cleanup_categories,
                    "reclaimed_bytes": self.cleanup_reclaimed_bytes,
                },
                "erase": {
                    "runs": self.erase_runs,
                    "levels": self.erase_levels,
                },
                "ai": {
                    "queries": self.ai_queries,
                    "provider": self.ai_provider,
                },
                "ui_launched": self.ui_launched,
                "commands_used": self.commands_used,
                "errors_total": self.errors_total,
            }
        })
    }
}

impl Default for TelemetryState {
    fn default() -> Self {
        Self::new()
    }
}
