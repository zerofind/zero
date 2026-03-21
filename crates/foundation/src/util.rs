//! Shared utility functions
//!
//! This module consolidates commonly used utilities to avoid duplication:
//! - Time utilities (timestamps)
//! - Byte formatting

use std::time::{SystemTime, UNIX_EPOCH};

/// Get current unix timestamp in seconds
#[inline]
pub fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Format bytes as human-readable string (binary units: KB, MB, GB, TB)
///
/// Uses 1024-based units (KiB/MiB/GiB but labeled KB/MB/GB for familiarity)
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} bytes")
    }
}

/// Format bytes as human-readable string (decimal units: 1000-based)
///
/// Uses SI units (1000-based) - useful for disk sizes as advertised
pub fn format_bytes_decimal(bytes: u64) -> String {
    const KB: u64 = 1000;
    const MB: u64 = 1000 * KB;
    const GB: u64 = 1000 * MB;
    const TB: u64 = 1000 * GB;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_now_timestamp() {
        let ts = now_timestamp();
        // Should be a reasonable unix timestamp (after 2020)
        assert!(ts > 1577836800); // 2020-01-01
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 bytes");
        assert_eq!(format_bytes(512), "512 bytes");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
        assert_eq!(format_bytes(1099511627776), "1.00 TB");
    }

    #[test]
    fn test_format_bytes_decimal() {
        assert_eq!(format_bytes_decimal(0), "0 B");
        assert_eq!(format_bytes_decimal(500), "500 B");
        assert_eq!(format_bytes_decimal(1000), "1.0 KB");
        assert_eq!(format_bytes_decimal(1500), "1.5 KB");
        assert_eq!(format_bytes_decimal(1_000_000), "1.00 MB");
        assert_eq!(format_bytes_decimal(1_000_000_000), "1.00 GB");
        assert_eq!(format_bytes_decimal(1_000_000_000_000), "1.00 TB");
    }
}
