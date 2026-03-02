//! Formatting helpers for progress display
//!
//! Single source of truth for formatting bytes, duration, throughput, and ETA.

pub use crate::util::format_bytes_decimal as format_bytes;

/// Format a duration in seconds as a human-readable string (e.g., "2h 15m")
pub fn format_duration(secs: f64) -> String {
    // Sanity check for extremely large or invalid values
    if secs.is_nan() || secs.is_infinite() || secs < 0.0 {
        return "unknown".to_string();
    }
    // Cap at 1 year (anything larger is likely a bug)
    if secs > 365.0 * 24.0 * 3600.0 {
        return ">1 year".to_string();
    }
    if secs < 60.0 {
        format!("{:.0}s", secs)
    } else if secs < 3600.0 {
        let mins = (secs / 60.0).floor();
        let remaining_secs = secs % 60.0;
        if remaining_secs >= 1.0 {
            format!("{:.0}m {:.0}s", mins, remaining_secs)
        } else {
            format!("{:.0}m", mins)
        }
    } else {
        let hours = (secs / 3600.0).floor();
        let mins = ((secs % 3600.0) / 60.0).floor();
        if mins >= 1.0 {
            format!("{:.0}h {:.0}m", hours, mins)
        } else {
            format!("{:.0}h", hours)
        }
    }
}

/// Format throughput as a human-readable string (e.g., "125.3 MB/s")
pub fn format_throughput(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1_000_000_000.0 {
        format!("{:.1} GB/s", bytes_per_sec / 1_000_000_000.0)
    } else if bytes_per_sec >= 1_000_000.0 {
        format!("{:.1} MB/s", bytes_per_sec / 1_000_000.0)
    } else if bytes_per_sec >= 1_000.0 {
        format!("{:.0} KB/s", bytes_per_sec / 1_000.0)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

/// Format ETA as a human-readable string (e.g., "~2h 15m left")
pub fn format_eta(eta_secs: Option<f64>) -> String {
    match eta_secs {
        Some(secs) => format!("~{} left", format_duration(secs)),
        None => "calculating...".to_string(),
    }
}

/// Truncate a filename for display, keeping the end visible
pub fn truncate_filename(filename: &str, max_len: usize) -> String {
    if filename.chars().count() <= max_len {
        filename.to_string()
    } else {
        let suffix: String = filename
            .chars()
            .rev()
            .take(max_len.saturating_sub(3))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("...{}", suffix)
    }
}

/// Format a size range (done/total) with appropriate units
pub fn format_size_range(bytes_done: u64, bytes_total: u64) -> String {
    let (done, total, unit) = if bytes_total >= 1_000_000_000 {
        (
            bytes_done as f64 / 1_000_000_000.0,
            bytes_total as f64 / 1_000_000_000.0,
            "GB",
        )
    } else {
        (
            bytes_done as f64 / 1_000_000.0,
            bytes_total as f64 / 1_000_000.0,
            "MB",
        )
    };
    // Use 2 decimal places for GB (more precision needed for large files)
    if bytes_total >= 1_000_000_000 {
        format!("{:.2}/{:.2} {}", done, total, unit)
    } else {
        format!("{:.1}/{:.1} {}", done, total, unit)
    }
}

/// Smooth throughput using exponential moving average
///
/// `alpha` controls smoothing: 0.0 = ignore new value, 1.0 = ignore old value
/// Typical value: 0.2-0.3 for stable display
pub fn smooth_throughput(current: f64, previous: f64, alpha: f64) -> f64 {
    if previous <= 0.0 {
        current
    } else {
        alpha * current + (1.0 - alpha) * previous
    }
}
