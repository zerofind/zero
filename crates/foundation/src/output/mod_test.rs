//! Tests for output module

use super::*;
use std::time::Duration;

// ============================================================================
// format_bytes tests
// ============================================================================

#[test]
fn test_format_bytes_bytes() {
    assert_eq!(format_bytes(0), "0 bytes");
    assert_eq!(format_bytes(1), "1 bytes");
    assert_eq!(format_bytes(500), "500 bytes");
    assert_eq!(format_bytes(1023), "1023 bytes");
}

#[test]
fn test_format_bytes_kilobytes() {
    assert_eq!(format_bytes(1024), "1.00 KB");
    assert_eq!(format_bytes(1536), "1.50 KB");
    assert_eq!(format_bytes(10240), "10.00 KB");
    assert_eq!(format_bytes(1048575), "1024.00 KB");
}

#[test]
fn test_format_bytes_megabytes() {
    assert_eq!(format_bytes(1048576), "1.00 MB");
    assert_eq!(format_bytes(1572864), "1.50 MB");
    assert_eq!(format_bytes(104857600), "100.00 MB");
}

#[test]
fn test_format_bytes_gigabytes() {
    assert_eq!(format_bytes(1073741824), "1.00 GB");
    assert_eq!(format_bytes(1610612736), "1.50 GB");
    assert_eq!(format_bytes(10737418240), "10.00 GB");
}

#[test]
fn test_format_bytes_terabytes() {
    assert_eq!(format_bytes(1099511627776), "1.00 TB");
    assert_eq!(format_bytes(1649267441664), "1.50 TB");
}

// ============================================================================
// format_count tests
// ============================================================================

#[test]
fn test_format_count_small() {
    assert_eq!(format_count(0), "0");
    assert_eq!(format_count(1), "1");
    assert_eq!(format_count(100), "100");
    assert_eq!(format_count(999), "999");
}

#[test]
fn test_format_count_thousands() {
    assert_eq!(format_count(1000), "1,000");
    assert_eq!(format_count(1234), "1,234");
    assert_eq!(format_count(12345), "12,345");
    assert_eq!(format_count(123456), "123,456");
}

#[test]
fn test_format_count_millions() {
    assert_eq!(format_count(1000000), "1,000,000");
    assert_eq!(format_count(1234567), "1,234,567");
    assert_eq!(format_count(12345678), "12,345,678");
}

// ============================================================================
// format_duration tests
// ============================================================================

#[test]
fn test_format_duration_microseconds() {
    assert!(format_duration(Duration::from_micros(500)).contains("µs"));
    assert!(format_duration(Duration::from_nanos(500)).contains("µs"));
}

#[test]
fn test_format_duration_milliseconds() {
    assert!(format_duration(Duration::from_millis(1)).contains("ms"));
    assert!(format_duration(Duration::from_millis(500)).contains("ms"));
    assert!(format_duration(Duration::from_millis(999)).contains("ms"));
}

#[test]
fn test_format_duration_seconds() {
    let result = format_duration(Duration::from_secs(5));
    assert!(result.contains('s'));
    assert!(!result.contains("ms"));
    assert!(!result.contains('m'));
}

#[test]
fn test_format_duration_minutes() {
    let result = format_duration(Duration::from_secs(90));
    assert!(result.contains('m'));
    assert!(result.contains('s'));
}

#[test]
fn test_format_duration_longer() {
    let result = format_duration(Duration::from_secs(125));
    assert!(result.contains("2m"));
}

// ============================================================================
// format_throughput tests
// ============================================================================

#[test]
fn test_format_throughput() {
    let result = format_throughput(100_000_000, Duration::from_secs(1));
    assert!(result.contains("100.0 MB/s"));
}

#[test]
fn test_format_throughput_zero_duration() {
    let result = format_throughput(1000, Duration::from_secs(0));
    assert!(result.contains("∞"));
}

#[test]
fn test_format_throughput_fractional() {
    let result = format_throughput(50_000_000, Duration::from_secs(2));
    assert!(result.contains("25.0 MB/s"));
}

// ============================================================================
// format_mb tests
// ============================================================================

#[test]
fn test_format_mb() {
    assert_eq!(format_mb(1_000_000), "1.00 MB");
    assert_eq!(format_mb(1_500_000), "1.50 MB");
    assert_eq!(format_mb(0), "0.00 MB");
}

// ============================================================================
// OutputMode tests
// ============================================================================

#[test]
fn test_output_mode_default() {
    assert_eq!(OutputMode::default(), OutputMode::Human);
}

#[test]
fn test_output_mode_equality() {
    assert_eq!(OutputMode::Human, OutputMode::Human);
    assert_eq!(OutputMode::Json, OutputMode::Json);
    assert_ne!(OutputMode::Human, OutputMode::Json);
}

// ============================================================================
// Outputter tests
// ============================================================================

#[test]
fn test_outputter_new() {
    let out = Outputter::new(OutputMode::Human);
    assert_eq!(out.mode(), OutputMode::Human);

    let out_json = Outputter::new(OutputMode::Json);
    assert_eq!(out_json.mode(), OutputMode::Json);
}

#[test]
fn test_outputter_is_json() {
    let human = Outputter::new(OutputMode::Human);
    assert!(!human.is_json());

    let json = Outputter::new(OutputMode::Json);
    assert!(json.is_json());
}

#[test]
fn test_outputter_clone() {
    let out = Outputter::new(OutputMode::Json);
    let cloned = out.clone();
    assert_eq!(out.mode(), cloned.mode());
}

// ============================================================================
// Response tests
// ============================================================================

#[test]
fn test_response_success() {
    let response = Response::success("test", 100, "data");
    assert!(response.success);
    assert_eq!(response.command, "test");
    assert_eq!(response.duration_ms, 100);
    assert_eq!(response.data, Some("data"));
    assert!(response.error.is_none());
}

#[test]
fn test_response_error() {
    let error = ErrorInfo::new("ERR001", "Something went wrong");
    let response: Response<()> = Response::error("test", 50, error);
    assert!(!response.success);
    assert_eq!(response.command, "test");
    assert!(response.data.is_none());
    assert!(response.error.is_some());
    assert_eq!(response.error.as_ref().unwrap().code, "ERR001");
}

// ============================================================================
// ErrorInfo tests
// ============================================================================

#[test]
fn test_error_info_new() {
    let err = ErrorInfo::new("CODE", "message");
    assert_eq!(err.code, "CODE");
    assert_eq!(err.message, "message");
}

#[test]
fn test_error_info_from_error() {
    let anyhow_err = anyhow::anyhow!("test error");
    let err = ErrorInfo::from_error(&anyhow_err);
    assert_eq!(err.code, "ERROR");
    assert!(err.message.contains("test error"));
}

// ============================================================================
// ProgressInfo tests
// ============================================================================

#[test]
fn test_progress_info_new() {
    let info = ProgressInfo::new(50.0, "scanning", "Processing files...");
    assert_eq!(info.percent, 50.0);
    assert_eq!(info.phase, "scanning");
    assert_eq!(info.message, "Processing files...");
    assert!(info.files_processed.is_none());
    assert!(info.bytes_processed.is_none());
}

#[test]
fn test_progress_info_with_files() {
    let info = ProgressInfo::new(25.0, "copying", "Copying...").with_files(100);
    assert_eq!(info.files_processed, Some(100));
}

#[test]
fn test_progress_info_with_bytes() {
    let info = ProgressInfo::new(75.0, "hashing", "Hashing...").with_bytes(1024);
    assert_eq!(info.bytes_processed, Some(1024));
}

#[test]
fn test_progress_info_chained() {
    let info = ProgressInfo::new(100.0, "done", "Complete")
        .with_files(500)
        .with_bytes(1_000_000);
    assert_eq!(info.files_processed, Some(500));
    assert_eq!(info.bytes_processed, Some(1_000_000));
}
