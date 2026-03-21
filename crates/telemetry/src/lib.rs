//! Anonymous product telemetry
//!
//! Collects aggregate usage statistics to improve Zero. No file names, paths,
//! or personal data is ever collected.
//!
//! - `zero telemetry show` -- see exactly what is sent
//! - `zero telemetry off` -- disable collection
//! - Reports every 7 days, on app launch (fire-and-forget)
//! - 24-hour grace period before first report

mod id;
mod reporter;
mod state;

pub use state::TelemetryState;

use std::path::PathBuf;

/// Check if telemetry is enabled
pub fn is_enabled() -> bool {
    let Some(path) = telemetry_path() else {
        return false;
    };
    let (state, _) = TelemetryState::load(&path);
    state.enabled
}

/// Set telemetry enabled/disabled
pub fn set_enabled(enabled: bool) {
    let Some(path) = telemetry_path() else {
        return;
    };
    let (mut state, _) = TelemetryState::load(&path);
    state.enabled = enabled;
    let _ = state.save(&path);
}

/// Get the payload that would be sent (for `zero telemetry show`)
pub fn show_payload() -> Option<String> {
    let path = telemetry_path()?;
    let (state, _) = TelemetryState::load(&path);
    let payload = state.to_payload();
    serde_json::to_string_pretty(&payload).ok()
}

/// Reset the install ID (generates a new anonymous identity)
pub fn reset_install_id() {
    let Some(path) = telemetry_path() else {
        return;
    };
    let (mut state, _) = TelemetryState::load(&path);
    state.install_id = id::generate_install_id();
    let _ = state.save(&path);
}

/// Check if this is the first run (state file doesn't exist yet).
/// Creates the state file on first run and returns true.
pub fn check_first_run() -> bool {
    let Some(path) = telemetry_path() else {
        return false;
    };
    let (state, is_new) = TelemetryState::load(&path);
    if is_new {
        let _ = state.save(&path);
    }
    is_new
}

/// Check if a report is due and send in a background thread.
/// Call early in `main()` -- never blocks the main flow.
pub fn maybe_report() {
    let Some(path) = telemetry_path() else {
        return;
    };
    let (state, _) = TelemetryState::load(&path);

    if !state.enabled || !state.should_report() {
        return;
    }

    let report_state = state.clone();
    std::thread::spawn(move || {
        if reporter::send_report(&report_state).is_ok()
            && let Some(path) = telemetry_path()
        {
            let (mut state, _) = TelemetryState::load(&path);
            state.reset_counters();
            let _ = state.save(&path);
        }
    });
}

// ========================= Recording functions =========================
// Each function loads state, applies the mutation, and saves.
// Designed for CLI: one command per invocation, so one load/save is fine.

/// Record a sync operation
pub fn record_sync(
    files: u64,
    bytes: u64,
    mirror: bool,
    verify: bool,
    cloud_backend: Option<&str>,
) {
    mutate(|s| {
        s.commands_used.insert("sync".into());
        s.sync_runs += 1;
        s.sync_files += files;
        s.sync_bytes += bytes;
        if mirror {
            s.sync_mirror_used = true;
        }
        if verify {
            s.sync_verify_used = true;
        }
        if let Some(backend) = cloud_backend {
            s.sync_cloud_used = true;
            s.sync_cloud_backends.insert(backend.into());
        }
    });
}

/// Record a search query
pub fn record_search(index_files: u64) {
    mutate(|s| {
        s.commands_used.insert("search".into());
        s.search_queries += 1;
        s.search_index_files = s.search_index_files.max(index_files);
    });
}

/// Record search watch mode usage
pub fn record_search_watch() {
    mutate(|s| s.search_watch_used = true);
}

/// Record a dedup operation
pub fn record_dedup(found: u64, deleted_bytes: u64) {
    mutate(|s| {
        s.commands_used.insert("dupes".into());
        s.dedup_runs += 1;
        s.dedup_found += found;
        s.dedup_deleted_bytes += deleted_bytes;
    });
}

/// Record a cleanup operation
pub fn record_cleanup(categories: &[String], reclaimed_bytes: u64) {
    mutate(|s| {
        s.commands_used.insert("cleanup".into());
        s.cleanup_runs += 1;
        for cat in categories {
            s.cleanup_categories.insert(cat.clone());
        }
        s.cleanup_reclaimed_bytes += reclaimed_bytes;
    });
}

/// Record an erase operation
pub fn record_erase(level: &str) {
    mutate(|s| {
        s.commands_used.insert("erase".into());
        s.erase_runs += 1;
        s.erase_levels.insert(level.into());
    });
}

/// Record an AI query
pub fn record_ai_query(provider: &str) {
    mutate(|s| {
        s.ai_queries += 1;
        s.ai_provider = Some(provider.into());
    });
}

/// Record UI launch
pub fn record_ui_launch() {
    mutate(|s| s.ui_launched = true);
}

/// Record a CLI command (for commands without specific recording)
pub fn record_command(name: &str) {
    mutate(|s| {
        s.commands_used.insert(name.into());
    });
}

/// Record an error
pub fn record_error() {
    mutate(|s| s.errors_total += 1);
}

// ========================= Internal =========================

fn telemetry_path() -> Option<PathBuf> {
    foundation::dirs::telemetry_path()
}

/// Load state, apply mutation, save. Silently ignores errors.
fn mutate(f: impl FnOnce(&mut TelemetryState)) {
    let Some(path) = telemetry_path() else {
        return;
    };
    let (mut state, _) = TelemetryState::load(&path);
    if !state.enabled {
        return;
    }
    f(&mut state);
    let _ = state.save(&path);
}
