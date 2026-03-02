//! Tests for formatting helpers

use super::format::*;

#[test]
fn test_format_bytes() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(500), "500 B");
    assert_eq!(format_bytes(999), "999 B");
    assert_eq!(format_bytes(1_000), "1.0 KB");
    assert_eq!(format_bytes(1_500), "1.5 KB");
    assert_eq!(format_bytes(1_000_000), "1.00 MB");
    assert_eq!(format_bytes(1_500_000), "1.50 MB");
    assert_eq!(format_bytes(1_000_000_000), "1.00 GB");
    assert_eq!(format_bytes(1_500_000_000), "1.50 GB");
}

#[test]
fn test_format_duration() {
    assert_eq!(format_duration(0.0), "0s");
    assert_eq!(format_duration(30.0), "30s");
    assert_eq!(format_duration(59.0), "59s");
    assert_eq!(format_duration(60.0), "1m");
    assert_eq!(format_duration(90.0), "1m 30s");
    assert_eq!(format_duration(3600.0), "1h");
    assert_eq!(format_duration(3660.0), "1h 1m");
    assert_eq!(format_duration(7200.0), "2h");
    assert_eq!(format_duration(7320.0), "2h 2m");
}

#[test]
fn test_format_throughput() {
    assert_eq!(format_throughput(0.0), "0 B/s");
    assert_eq!(format_throughput(500.0), "500 B/s");
    assert_eq!(format_throughput(1_500.0), "2 KB/s");
    assert_eq!(format_throughput(1_500_000.0), "1.5 MB/s");
    assert_eq!(format_throughput(1_500_000_000.0), "1.5 GB/s");
}

#[test]
fn test_format_eta() {
    assert_eq!(format_eta(None), "calculating...");
    assert_eq!(format_eta(Some(30.0)), "~30s left");
    assert_eq!(format_eta(Some(90.0)), "~1m 30s left");
    assert_eq!(format_eta(Some(3720.0)), "~1h 2m left");
}

#[test]
fn test_truncate_filename() {
    // Short filename - no truncation
    assert_eq!(truncate_filename("short.txt", 50), "short.txt");

    // Exact length - no truncation
    assert_eq!(truncate_filename("12345", 5), "12345");

    // Needs truncation
    assert_eq!(
        truncate_filename("this_is_a_very_long_filename.txt", 20),
        "...long_filename.txt"
    );

    // Empty string
    assert_eq!(truncate_filename("", 10), "");
}

#[test]
fn test_format_size_range() {
    // Small sizes (MB)
    assert_eq!(
        format_size_range(500_000_000, 900_000_000),
        "500.0/900.0 MB"
    );

    // Large sizes (GB) - uses 2 decimal places for precision
    assert_eq!(
        format_size_range(1_500_000_000, 3_000_000_000),
        "1.50/3.00 GB"
    );

    // Zero
    assert_eq!(format_size_range(0, 1_000_000), "0.0/1.0 MB");
}
