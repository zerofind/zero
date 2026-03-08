use super::*;

#[test]
fn format_size_bytes() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(512), "512 B");
    assert_eq!(format_size(1023), "1023 B");
}

#[test]
fn format_size_kb() {
    assert_eq!(format_size(1024), "1.0 KB");
    assert_eq!(format_size(1536), "1.5 KB");
}

#[test]
fn format_size_mb() {
    assert_eq!(format_size(1024 * 1024), "1.0 MB");
    assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
}

#[test]
fn format_size_gb() {
    assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
}

#[test]
fn format_time_zero() {
    assert_eq!(format_time(0), "unknown");
}

#[test]
fn format_time_known_date() {
    // 2024-01-01 00:00 UTC = 1704067200
    assert_eq!(format_time(1704067200), "2024-01-01 00:00");
}

#[test]
fn days_to_ymd_epoch() {
    assert_eq!(days_to_ymd(0), (1970, 1, 1));
}

#[test]
fn days_to_ymd_known() {
    // 2024-01-01 is day 19723 from epoch
    assert_eq!(days_to_ymd(19723), (2024, 1, 1));
}
