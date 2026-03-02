//! Tests for chunk-based file transfer

use super::*;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

fn create_test_file(dir: &Path, name: &str, content: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    let mut file = File::create(&path).unwrap();
    file.write_all(content).unwrap();
    path
}

// ============================================================================
// copy_file_with_hash tests
// ============================================================================

#[test]
fn test_copy_with_hash_small_file() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"hello world");
    let dest = dir.path().join("dest.txt");

    let result = copy_file_with_hash(&source, &dest, false).unwrap();

    assert_eq!(result.bytes_copied, 11);
    assert!(!result.source_hash.is_empty());
    assert!(!result.dest_verified);
    assert!(result.dest_hash.is_none());

    // Verify content was copied correctly
    let dest_content = fs::read_to_string(&dest).unwrap();
    assert_eq!(dest_content, "hello world");
}

#[test]
fn test_copy_with_hash_and_verify() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"test content for hashing");
    let dest = dir.path().join("dest.txt");

    let result = copy_file_with_hash(&source, &dest, true).unwrap();

    assert_eq!(result.bytes_copied, 24);
    assert!(!result.source_hash.is_empty());
    assert!(result.dest_verified);
    assert!(result.dest_hash.is_some());
    assert_eq!(result.source_hash, result.dest_hash.unwrap());
}

#[test]
fn test_copy_with_hash_large_file() {
    let dir = TempDir::new().unwrap();

    // Create a 2MB file
    let content: Vec<u8> = (0..2_000_000).map(|i| (i % 256) as u8).collect();
    let source = create_test_file(dir.path(), "large.bin", &content);
    let dest = dir.path().join("large_copy.bin");

    let result = copy_file_with_hash(&source, &dest, true).unwrap();

    assert_eq!(result.bytes_copied, 2_000_000);
    assert!(result.dest_verified);
    assert_eq!(result.source_hash, result.dest_hash.unwrap());

    // Verify content
    let dest_content = fs::read(&dest).unwrap();
    assert_eq!(dest_content, content);
}

#[test]
fn test_copy_with_hash_creates_parent_dirs() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"test");
    let dest = dir.path().join("nested").join("deep").join("dest.txt");

    let result = copy_file_with_hash(&source, &dest, false).unwrap();

    assert_eq!(result.bytes_copied, 4);
    assert!(dest.exists());
}

#[test]
fn test_copy_with_hash_empty_file() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "empty.txt", b"");
    let dest = dir.path().join("empty_copy.txt");

    let result = copy_file_with_hash(&source, &dest, true).unwrap();

    assert_eq!(result.bytes_copied, 0);
    assert!(result.dest_verified);
    // Hash of empty input should still be valid
    assert!(!result.source_hash.is_empty());
}

#[test]
fn test_copy_with_hash_preserves_mtime() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"test");
    let dest = dir.path().join("dest.txt");

    let result = copy_file_with_hash(&source, &dest, false).unwrap();

    // mtime should be captured
    assert!(result.source_mtime > 0);
}

#[test]
fn test_copy_with_hash_consistent_hash() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"consistent content");
    let dest1 = dir.path().join("dest1.txt");
    let dest2 = dir.path().join("dest2.txt");

    let result1 = copy_file_with_hash(&source, &dest1, false).unwrap();
    let result2 = copy_file_with_hash(&source, &dest2, false).unwrap();

    // Same source should produce same hash
    assert_eq!(result1.source_hash, result2.source_hash);
}

#[test]
fn test_copy_with_hash_nonexistent_source() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("nonexistent.txt");
    let dest = dir.path().join("dest.txt");

    let result = copy_file_with_hash(&source, &dest, false);
    assert!(matches!(result, Err(CopyError::MetadataError { .. })));
}

// ============================================================================
// copy_file tests
// ============================================================================

#[test]
fn test_copy_small_file() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"hello world");
    let dest = dir.path().join("dest.txt");

    let options = CopyOptions::default();
    let (progress, _) = copy_file(&source, &dest, &options).unwrap();

    assert!(progress.complete);
    assert_eq!(progress.bytes_copied, 11);
    assert_eq!(progress.total_bytes, 11);

    let dest_content = fs::read_to_string(&dest).unwrap();
    assert_eq!(dest_content, "hello world");
}

#[test]
fn test_copy_with_verification() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"test content");
    let dest = dir.path().join("dest.txt");

    let options = CopyOptions {
        verify: true,
        ..Default::default()
    };
    let (progress, hash) = copy_file(&source, &dest, &options).unwrap();

    assert!(progress.complete);
    assert!(hash.is_some());
    assert!(!hash.unwrap().is_empty());
}

#[test]
fn test_copy_creates_parent_dirs() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"test");
    let dest = dir.path().join("nested").join("deep").join("dest.txt");

    let options = CopyOptions::default();
    let (progress, _) = copy_file(&source, &dest, &options).unwrap();

    assert!(progress.complete);
    assert!(dest.exists());
}

#[test]
fn test_copy_empty_file() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "empty.txt", b"");
    let dest = dir.path().join("empty_copy.txt");

    let options = CopyOptions::default();
    let (progress, _) = copy_file(&source, &dest, &options).unwrap();

    assert!(progress.complete);
    assert_eq!(progress.bytes_copied, 0);
    assert_eq!(progress.total_bytes, 0);
}

#[test]
fn test_copy_large_file() {
    let dir = TempDir::new().unwrap();

    // Create a 5MB file
    let content: Vec<u8> = (0..5_000_000).map(|i| (i % 256) as u8).collect();
    let source = create_test_file(dir.path(), "large.bin", &content);
    let dest = dir.path().join("large_copy.bin");

    let options = CopyOptions {
        verify: true,
        ..Default::default()
    };
    let (progress, hash) = copy_file(&source, &dest, &options).unwrap();

    assert!(progress.complete);
    assert_eq!(progress.bytes_copied, 5_000_000);
    assert!(hash.is_some());

    // Verify content
    let dest_content = fs::read(&dest).unwrap();
    assert_eq!(dest_content, content);
}

#[test]
fn test_copy_overwrites_existing() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"new content");
    let dest = create_test_file(dir.path(), "dest.txt", b"old content");

    let options = CopyOptions::default();
    let (progress, _) = copy_file(&source, &dest, &options).unwrap();

    assert!(progress.complete);
    let dest_content = fs::read_to_string(&dest).unwrap();
    assert_eq!(dest_content, "new content");
}

#[test]
fn test_copy_nonexistent_source() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("nonexistent.txt");
    let dest = dir.path().join("dest.txt");

    let result = copy_file(&source, &dest, &CopyOptions::default());
    assert!(matches!(result, Err(CopyError::MetadataError { .. })));
}

// ============================================================================
// Utility function tests
// ============================================================================

#[test]
fn test_get_partial_size() {
    let dir = TempDir::new().unwrap();
    let file = create_test_file(dir.path(), "partial.txt", b"12345");

    assert_eq!(get_partial_size(&file), Some(5));
    assert_eq!(get_partial_size(&dir.path().join("nonexistent")), None);
}

#[test]
fn test_remove_partial() {
    let dir = TempDir::new().unwrap();
    let file = create_test_file(dir.path(), "partial.txt", b"12345");

    assert!(file.exists());
    remove_partial(&file).unwrap();
    assert!(!file.exists());

    // Removing nonexistent file should not error
    remove_partial(&file).unwrap();
}

#[test]
fn test_progress_percent() {
    let progress = CopyProgress {
        total_bytes: 100,
        bytes_copied: 50,
        complete: false,
    };
    assert!((progress.percent() - 50.0).abs() < 0.001);

    let empty_progress = CopyProgress {
        total_bytes: 0,
        bytes_copied: 0,
        complete: true,
    };
    assert!((empty_progress.percent() - 100.0).abs() < 0.001);
}

#[test]
fn test_progress_percent_full() {
    let progress = CopyProgress {
        total_bytes: 100,
        bytes_copied: 100,
        complete: true,
    };
    assert!((progress.percent() - 100.0).abs() < 0.001);
}

#[test]
fn test_copy_options_default() {
    let options = CopyOptions::default();
    assert!(!options.verify);
}

// ============================================================================
// CopyError tests
// ============================================================================

#[test]
fn test_copy_error_display() {
    let err = CopyError::MetadataError {
        path: "/test/path".to_string(),
        source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
    };
    let display = format!("{}", err);
    assert!(display.contains("/test/path"));
}

#[test]
fn test_copy_error_read_error() {
    let err = CopyError::ReadError {
        path: "/test/source".to_string(),
        source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
    };
    let display = format!("{}", err);
    assert!(display.contains("/test/source"));
}

#[test]
fn test_copy_error_write_error() {
    let err = CopyError::WriteError {
        path: "/test/dest".to_string(),
        source: std::io::Error::new(std::io::ErrorKind::Other, "write failed"),
    };
    let display = format!("{}", err);
    assert!(display.contains("/test/dest"));
}

// ============================================================================
// CopyWithHashResult tests
// ============================================================================

#[test]
fn test_copy_with_hash_result_fields() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"test data");
    let dest = dir.path().join("dest.txt");

    let result = copy_file_with_hash(&source, &dest, true).unwrap();

    assert_eq!(result.bytes_copied, 9);
    assert!(result.source_mtime > 0);
    assert!(!result.source_hash.is_empty());
    assert!(result.dest_verified);
    assert!(result.dest_hash.is_some());
}
