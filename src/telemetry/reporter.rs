//! Telemetry reporter
//!
//! Sends accumulated metrics via HTTP POST. Fire-and-forget with timeout.
//! Uses ureq (already a dependency for the updater).

use super::state::TelemetryState;

/// Telemetry collection endpoint
const ENDPOINT: &str = "https://t.zero.dev/v1/track";

/// Send a telemetry report. Fire-and-forget from a background thread.
pub fn send_report(state: &TelemetryState) -> Result<(), String> {
    let payload = state.to_payload();
    let body = serde_json::to_string(&payload).map_err(|e| e.to_string())?;

    ureq::post(ENDPOINT)
        .header("Content-Type", "application/json")
        .header("User-Agent", &format!("zero/{}", state.version))
        .send(body.as_bytes())
        .map_err(|e| e.to_string())?;

    Ok(())
}
