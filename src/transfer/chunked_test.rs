//! Tests for chunked file transfer with resume support

use super::*;
use crate::transfer::chunked_types::{
    ChunkedProgress, ChunkedTransferState, DEFAULT_CHUNK_SIZE, DEFAULT_CHUNK_THRESHOLD,
    should_use_chunked,
};
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
// should_use_chunked tests
// ============================================================================

#[test]
fn test_should_use_chunked() {
    assert!(!should_use_chunked(
        50 * 1024 * 1024,
        DEFAULT_CHUNK_THRESHOLD
    )); // 50MB
    assert!(should_use_chunked(
        100 * 1024 * 1024,
        DEFAULT_CHUNK_THRESHOLD
    )); // 100MB
    assert!(should_use_chunked(
        500 * 1024 * 1024,
        DEFAULT_CHUNK_THRESHOLD
    )); // 500MB
}

#[test]
fn test_should_use_chunked_exact_threshold() {
    // At exactly the threshold, should use chunked
    assert!(should_use_chunked(
        DEFAULT_CHUNK_THRESHOLD,
        DEFAULT_CHUNK_THRESHOLD
    ));
}

#[test]
fn test_should_use_chunked_small_files() {
    assert!(!should_use_chunked(0, DEFAULT_CHUNK_THRESHOLD));
    assert!(!should_use_chunked(1024, DEFAULT_CHUNK_THRESHOLD));
    assert!(!should_use_chunked(1024 * 1024, DEFAULT_CHUNK_THRESHOLD)); // 1MB
}

// ============================================================================
// copy_chunked_with_resume tests
// ============================================================================

#[test]
fn test_chunked_copy_small_file() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"hello world");
    let dest = dir.path().join("dest.txt");

    let options = ChunkedCopyOptions {
        verify_after_copy: false,
        ..Default::default()
    };

    let result = copy_chunked_with_resume(&source, &dest, &options, None).unwrap();

    assert_eq!(result.total_bytes, 11);
    assert_eq!(result.bytes_transferred, 11);
    assert_eq!(result.resumed_from_chunk, 0);
    assert!(!result.was_resumed());

    let dest_content = std::fs::read_to_string(&dest).unwrap();
    assert_eq!(dest_content, "hello world");
}

#[test]
fn test_chunked_copy_with_verification() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"hello world verification test");
    let dest = dir.path().join("dest.txt");

    let options = ChunkedCopyOptions {
        verify_after_copy: true,
        ..Default::default()
    };

    let result = copy_chunked_with_resume(&source, &dest, &options, None).unwrap();

    assert!(result.file_hash.is_some());
}

#[test]
fn test_chunked_copy_large_file() {
    let dir = TempDir::new().unwrap();

    // Create a file larger than chunk size
    let content: Vec<u8> = (0..5_000_000).map(|i| (i % 256) as u8).collect();
    let source = create_test_file(dir.path(), "large.bin", &content);
    let dest = dir.path().join("large_copy.bin");

    let options = ChunkedCopyOptions {
        chunk_size: 1024 * 1024, // 1MB chunks
        verify_after_copy: false,
        ..Default::default()
    };

    let result = copy_chunked_with_resume(&source, &dest, &options, None).unwrap();

    assert_eq!(result.total_bytes, 5_000_000);
    assert_eq!(result.bytes_transferred, 5_000_000);
    assert!(result.total_chunks >= 5);

    // Verify content
    let dest_content = std::fs::read(&dest).unwrap();
    assert_eq!(dest_content, content);
}

#[test]
fn test_chunked_copy_resume_from_middle() {
    let dir = TempDir::new().unwrap();

    // Create source file with known pattern
    let chunk_size = 1024; // 1KB for testing
    let total_size = chunk_size * 5; // 5 chunks
    let content: Vec<u8> = (0..total_size).map(|i| (i % 256) as u8).collect();
    let source = create_test_file(dir.path(), "source.bin", &content);
    let dest = dir.path().join("dest.bin");

    // Simulate partial transfer (first 3 chunks = 3KB)
    {
        let mut dest_file = File::create(&dest).unwrap();
        dest_file.write_all(&content[..chunk_size * 3]).unwrap();
    }

    let options = ChunkedCopyOptions {
        chunk_size,
        verify_after_copy: true,
        enable_resume: true,
        ..Default::default()
    };

    let result = copy_chunked_with_resume(&source, &dest, &options, None).unwrap();

    // Should resume from chunk 3 (0-indexed)
    assert_eq!(result.resumed_from_chunk, 3);
    assert!(result.was_resumed());
    assert_eq!(result.chunks_transferred, 2); // Only last 2 chunks
    assert_eq!(result.bytes_transferred, (chunk_size * 2) as u64);
    assert_eq!(result.bytes_saved(), (chunk_size * 3) as u64);

    // Verify full content
    let dest_content = std::fs::read(&dest).unwrap();
    assert_eq!(dest_content, content);
}

#[test]
fn test_chunked_copy_corrupted_partial() {
    let dir = TempDir::new().unwrap();

    // Create source file with known pattern
    let chunk_size = 1024; // 1KB for testing
    let total_size = chunk_size * 5; // 5 chunks
    let content: Vec<u8> = (0..total_size).map(|i| (i % 256) as u8).collect();
    let source = create_test_file(dir.path(), "source.bin", &content);
    let dest = dir.path().join("dest.bin");

    // Simulate partial transfer with CORRUPTED chunk 2
    {
        let mut dest_file = File::create(&dest).unwrap();
        // First 2 chunks correct
        dest_file.write_all(&content[..chunk_size * 2]).unwrap();
        // Chunk 2 corrupted (different data)
        let corrupted: Vec<u8> = (0..chunk_size).map(|_| 0xFF).collect();
        dest_file.write_all(&corrupted).unwrap();
    }

    let options = ChunkedCopyOptions {
        chunk_size,
        verify_after_copy: true,
        enable_resume: true,
        ..Default::default()
    };

    let result = copy_chunked_with_resume(&source, &dest, &options, None).unwrap();

    // Should detect corruption at chunk 2 and resume from there
    assert_eq!(result.resumed_from_chunk, 2);
    assert_eq!(result.chunks_transferred, 3); // Chunks 2, 3, 4

    // Verify full content is now correct
    let dest_content = std::fs::read(&dest).unwrap();
    assert_eq!(dest_content, content);
}

#[test]
fn test_chunked_copy_creates_parent_dirs() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"test content");
    let dest = dir.path().join("nested").join("deep").join("dest.txt");

    let options = ChunkedCopyOptions {
        verify_after_copy: false,
        ..Default::default()
    };

    let result = copy_chunked_with_resume(&source, &dest, &options, None).unwrap();

    assert_eq!(result.total_bytes, 12);
    assert!(dest.exists());
}

#[test]
fn test_chunked_copy_empty_file() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "empty.txt", b"");
    let dest = dir.path().join("empty_copy.txt");

    let options = ChunkedCopyOptions {
        verify_after_copy: false,
        ..Default::default()
    };

    let result = copy_chunked_with_resume(&source, &dest, &options, None).unwrap();

    assert_eq!(result.total_bytes, 0);
    assert!(dest.exists());
}

#[test]
fn test_chunked_copy_fresh_no_resume() {
    let dir = TempDir::new().unwrap();

    let content: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
    let source = create_test_file(dir.path(), "source.bin", &content);
    let dest = dir.path().join("dest.bin");

    // Dest doesn't exist - should be fresh copy
    let options = ChunkedCopyOptions {
        chunk_size: 1024,
        verify_after_copy: true,
        enable_resume: true,
        ..Default::default()
    };

    let result = copy_chunked_with_resume(&source, &dest, &options, None).unwrap();

    assert_eq!(result.resumed_from_chunk, 0);
    assert!(!result.was_resumed());
    assert_eq!(result.bytes_transferred, 10000);
}

// ============================================================================
// ChunkedProgress tests
// ============================================================================

#[test]
fn test_chunked_progress_percent() {
    let progress = ChunkedProgress {
        total_bytes: 1000,
        bytes_copied: 250,
        current_chunk: 1,
        total_chunks: 4,
        resumed_from_chunk: 0,
        complete: false,
    };

    assert!((progress.percent() - 25.0).abs() < 0.001);
}

#[test]
fn test_chunked_progress_percent_complete() {
    let progress = ChunkedProgress {
        total_bytes: 1000,
        bytes_copied: 1000,
        current_chunk: 4,
        total_chunks: 4,
        resumed_from_chunk: 0,
        complete: true,
    };

    assert!((progress.percent() - 100.0).abs() < 0.001);
}

#[test]
fn test_chunked_progress_percent_zero_bytes() {
    let progress = ChunkedProgress {
        total_bytes: 0,
        bytes_copied: 0,
        current_chunk: 0,
        total_chunks: 0,
        resumed_from_chunk: 0,
        complete: true,
    };

    assert!((progress.percent() - 100.0).abs() < 0.001);
}

// ============================================================================
// ChunkedTransferState tests
// ============================================================================

#[test]
fn test_transfer_state() {
    let state = ChunkedTransferState::new(std::path::PathBuf::from("test.txt"), 1000);

    assert_eq!(state.bytes_done, 0);
    assert!(!state.is_complete());
    assert!((state.percent() - 0.0).abs() < 0.001);

    let mut state = state;
    state.bytes_done = 1000;
    assert!(state.is_complete());
    assert!((state.percent() - 100.0).abs() < 0.001);
}

#[test]
fn test_transfer_state_partial() {
    let mut state = ChunkedTransferState::new(std::path::PathBuf::from("test.txt"), 1000);
    state.bytes_done = 500;

    assert!(!state.is_complete());
    assert!((state.percent() - 50.0).abs() < 0.001);
}

// ============================================================================
// ChunkedCopyResult tests
// ============================================================================

#[test]
fn test_chunked_copy_result_was_resumed() {
    let dir = TempDir::new().unwrap();
    let source = create_test_file(dir.path(), "source.txt", b"test");
    let dest = dir.path().join("dest.txt");

    let options = ChunkedCopyOptions::default();
    let result = copy_chunked_with_resume(&source, &dest, &options, None).unwrap();

    // Fresh copy should not be marked as resumed
    assert!(!result.was_resumed());
    assert_eq!(result.bytes_saved(), 0);
}

// ============================================================================
// ChunkedCopyOptions tests
// ============================================================================

#[test]
fn test_chunked_copy_options_default() {
    let options = ChunkedCopyOptions::default();
    assert_eq!(options.chunk_size, DEFAULT_CHUNK_SIZE);
    assert!(options.verify_after_copy);
    assert!(options.enable_resume);
}

#[test]
fn test_chunked_copy_options_custom_chunk_size() {
    let options = ChunkedCopyOptions {
        chunk_size: 512 * 1024, // 512KB
        ..Default::default()
    };
    assert_eq!(options.chunk_size, 512 * 1024);
}

// ============================================================================
// Error handling tests
// ============================================================================

#[test]
fn test_chunked_copy_nonexistent_source() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("nonexistent.txt");
    let dest = dir.path().join("dest.txt");

    let options = ChunkedCopyOptions::default();
    let result = copy_chunked_with_resume(&source, &dest, &options, None);

    assert!(result.is_err());
}
