//! Tests for transfer job orchestration

use super::*;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

fn create_test_source(dir: &Path) {
    fs::create_dir_all(dir.join("subdir")).unwrap();

    File::create(dir.join("file1.txt"))
        .unwrap()
        .write_all(b"hello world")
        .unwrap();

    File::create(dir.join("file2.txt"))
        .unwrap()
        .write_all(b"goodbye world")
        .unwrap();

    File::create(dir.join("subdir/nested.txt"))
        .unwrap()
        .write_all(b"nested content")
        .unwrap();
}

// ============================================================================
// TransferJob tests
// ============================================================================

#[test]
fn test_transfer_job_fresh() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");

    create_test_source(&source);

    let options = TransferOptions::default();
    let job = TransferJob::new(&source, &dest, options).unwrap();

    let result = job.run(|_| {}).unwrap();

    assert_eq!(result.files_transferred, 3);
    assert_eq!(result.files_failed, 0);

    // Verify files exist
    assert!(dest.join("file1.txt").exists());
    assert!(dest.join("file2.txt").exists());
    assert!(dest.join("subdir/nested.txt").exists());

    // Verify content
    let content = fs::read_to_string(dest.join("file1.txt")).unwrap();
    assert_eq!(content, "hello world");
}

#[test]
fn test_transfer_job_empty_source() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");

    fs::create_dir_all(&source).unwrap();

    let job = TransferJob::new(&source, &dest, TransferOptions::default()).unwrap();
    let result = job.run(|_| {}).unwrap();

    assert_eq!(result.files_transferred, 0);
    assert_eq!(result.files_failed, 0);
}

#[test]
fn test_transfer_job_source_not_found() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("nonexistent");
    let dest = dir.path().join("dest");

    let result = TransferJob::new(&source, &dest, TransferOptions::default());
    assert!(matches!(result, Err(TransferError::SourceNotFound(_))));
}

#[test]
fn test_transfer_overwrites_existing() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");

    // Create source
    fs::create_dir_all(&source).unwrap();
    File::create(source.join("file.txt"))
        .unwrap()
        .write_all(b"new content")
        .unwrap();

    // Create dest with old content
    fs::create_dir_all(&dest).unwrap();
    File::create(dest.join("file.txt"))
        .unwrap()
        .write_all(b"old content")
        .unwrap();

    let job = TransferJob::new(&source, &dest, TransferOptions::default()).unwrap();
    let result = job.run(|_| {}).unwrap();

    assert_eq!(result.files_transferred, 1);

    // Should have new content
    let content = fs::read_to_string(dest.join("file.txt")).unwrap();
    assert_eq!(content, "new content");
}

#[test]
fn test_transfer_with_verification() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");

    create_test_source(&source);

    let options = TransferOptions {
        verify: true,
        ..Default::default()
    };
    let job = TransferJob::new(&source, &dest, options).unwrap();
    let result = job.run(|_| {}).unwrap();

    assert_eq!(result.files_transferred, 3);
    assert_eq!(result.files_failed, 0);
}

// ============================================================================
// TransferProgress tests
// ============================================================================

#[test]
fn test_transfer_progress_percent() {
    let progress = TransferProgress {
        total_files: 10,
        files_done: 5,
        total_bytes: 1000,
        bytes_done: 500,
        current_file: None,
        errors: 0,
    };

    assert!((progress.percent() - 50.0).abs() < 0.001);
}

#[test]
fn test_transfer_progress_percent_complete() {
    let progress = TransferProgress {
        total_files: 10,
        files_done: 10,
        total_bytes: 1000,
        bytes_done: 1000,
        current_file: None,
        errors: 0,
    };

    assert!((progress.percent() - 100.0).abs() < 0.001);
}

#[test]
fn test_transfer_progress_percent_zero_bytes() {
    let progress = TransferProgress {
        total_files: 0,
        files_done: 0,
        total_bytes: 0,
        bytes_done: 0,
        current_file: None,
        errors: 0,
    };

    // With zero bytes, should return 100% or handle gracefully
    let percent = progress.percent();
    assert!(percent >= 0.0 && percent <= 100.0);
}

#[test]
fn test_transfer_progress_with_current_file() {
    let progress = TransferProgress {
        total_files: 10,
        files_done: 5,
        total_bytes: 1000,
        bytes_done: 500,
        current_file: Some("test.txt".to_string()),
        errors: 0,
    };

    assert_eq!(progress.current_file, Some("test.txt".to_string()));
}

// ============================================================================
// TransferOptions tests
// ============================================================================

#[test]
fn test_transfer_options_default() {
    let options = TransferOptions::default();
    assert!(!options.verify);
}

#[test]
fn test_transfer_options_with_verify() {
    let options = TransferOptions {
        verify: true,
        ..Default::default()
    };
    assert!(options.verify);
}

// ============================================================================
// TransferError tests
// ============================================================================

#[test]
fn test_transfer_error_source_not_found_display() {
    let err = TransferError::SourceNotFound(std::path::PathBuf::from("/test/path"));
    let display = format!("{}", err);
    assert!(display.contains("/test/path"));
}

// ============================================================================
// TransferResult tests
// ============================================================================

#[test]
fn test_transfer_result_success() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");

    create_test_source(&source);

    let job = TransferJob::new(&source, &dest, TransferOptions::default()).unwrap();
    let result = job.run(|_| {}).unwrap();

    assert!(result.files_failed == 0);
    assert!(result.bytes_transferred > 0);
}

#[test]
fn test_transfer_creates_dest_directory() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("new_dest");

    create_test_source(&source);

    // dest doesn't exist yet
    assert!(!dest.exists());

    let job = TransferJob::new(&source, &dest, TransferOptions::default()).unwrap();
    let _result = job.run(|_| {}).unwrap();

    // dest should be created
    assert!(dest.exists());
}

#[test]
fn test_transfer_nested_directories() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");

    // Create deeply nested source
    fs::create_dir_all(source.join("a/b/c")).unwrap();
    File::create(source.join("a/b/c/deep.txt"))
        .unwrap()
        .write_all(b"deep content")
        .unwrap();

    let job = TransferJob::new(&source, &dest, TransferOptions::default()).unwrap();
    let result = job.run(|_| {}).unwrap();

    assert_eq!(result.files_transferred, 1);
    assert!(dest.join("a/b/c/deep.txt").exists());
}
