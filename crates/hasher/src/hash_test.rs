//! Tests for file hashing implementation

use super::*;
use std::io::Write;
use std::sync::atomic::Ordering;
use tempfile::NamedTempFile;

fn create_test_file(content: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content).unwrap();
    file.flush().unwrap();
    file
}

#[test]
fn test_hash_small_file_blake3() {
    let file = create_test_file(b"hello world");
    let result = hash_file(file.path(), HashAlgorithm::Blake3).unwrap();

    assert_eq!(result.algorithm, HashAlgorithm::Blake3);
    assert_eq!(result.bytes_hashed, 11);
    assert_eq!(result.hash.len(), 32);

    // Known blake3 hash for "hello world"
    assert_eq!(
        result.to_hex(),
        "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
    );
}

#[test]
fn test_hash_small_file_xxh3() {
    let file = create_test_file(b"hello world");
    let result = hash_file(file.path(), HashAlgorithm::Xxh3).unwrap();

    assert_eq!(result.algorithm, HashAlgorithm::Xxh3);
    assert_eq!(result.bytes_hashed, 11);
    assert_eq!(result.hash.len(), 16);
}

#[test]
fn test_hash_empty_file() {
    let file = create_test_file(b"");
    let result = hash_file(file.path(), HashAlgorithm::Blake3).unwrap();

    assert_eq!(result.bytes_hashed, 0);
    // Blake3 hash of empty input
    assert_eq!(
        result.to_hex(),
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );
}

#[test]
fn test_hash_empty_file_xxh3() {
    let file = create_test_file(b"");
    let result = hash_file(file.path(), HashAlgorithm::Xxh3).unwrap();

    assert_eq!(result.bytes_hashed, 0);
    assert_eq!(result.hash.len(), 16);
}

#[test]
fn test_hash_with_buffer_reuse() {
    let file1 = create_test_file(b"file one content");
    let file2 = create_test_file(b"file two content");

    let mut buffer = vec![0u8; 128 * 1024];

    let result1 = hash_file_with_buffer(file1.path(), HashAlgorithm::Blake3, &mut buffer).unwrap();
    let result2 = hash_file_with_buffer(file2.path(), HashAlgorithm::Blake3, &mut buffer).unwrap();

    // Different content should produce different hashes
    assert_ne!(result1.to_hex(), result2.to_hex());
}

#[test]
fn test_hash_with_buffer_xxh3() {
    let file = create_test_file(b"test content");
    let mut buffer = vec![0u8; 128 * 1024];

    let result = hash_file_with_buffer(file.path(), HashAlgorithm::Xxh3, &mut buffer).unwrap();

    assert_eq!(result.algorithm, HashAlgorithm::Xxh3);
    assert_eq!(result.bytes_hashed, 12);
}

#[test]
fn test_hash_consistency() {
    let file = create_test_file(b"test content for consistency check");

    // Hash the same file twice, should get same result
    let result1 = hash_file(file.path(), HashAlgorithm::Blake3).unwrap();
    let result2 = hash_file(file.path(), HashAlgorithm::Blake3).unwrap();

    assert_eq!(result1.to_hex(), result2.to_hex());
}

#[test]
fn test_hash_consistency_xxh3() {
    let file = create_test_file(b"test content for consistency check");

    let result1 = hash_file(file.path(), HashAlgorithm::Xxh3).unwrap();
    let result2 = hash_file(file.path(), HashAlgorithm::Xxh3).unwrap();

    assert_eq!(result1.to_hex(), result2.to_hex());
}

#[test]
fn test_hash_nonexistent_file() {
    let result = hash_file(Path::new("/nonexistent/file.txt"), HashAlgorithm::Blake3);
    assert!(matches!(result, Err(HashError::OpenError { .. })));
}

#[test]
fn test_hash_nonexistent_file_xxh3() {
    let result = hash_file(Path::new("/nonexistent/file.txt"), HashAlgorithm::Xxh3);
    assert!(matches!(result, Err(HashError::OpenError { .. })));
}

#[test]
fn test_as_blake3_hash() {
    let file = create_test_file(b"test");
    let result = hash_file(file.path(), HashAlgorithm::Blake3).unwrap();

    let arr = result.as_blake3_hash();
    assert!(arr.is_some());
    assert_eq!(arr.unwrap().len(), 32);

    // XXH3 result shouldn't convert to blake3 array
    let result_xxh3 = hash_file(file.path(), HashAlgorithm::Xxh3).unwrap();
    assert!(result_xxh3.as_blake3_hash().is_none());
}

#[test]
fn test_as_xxh3_hash() {
    let file = create_test_file(b"test");
    let result = hash_file(file.path(), HashAlgorithm::Xxh3).unwrap();

    let arr = result.as_xxh3_hash();
    assert!(arr.is_some());
    assert_eq!(arr.unwrap().len(), 16);

    // Blake3 result shouldn't convert to xxh3 array
    let result_blake3 = hash_file(file.path(), HashAlgorithm::Blake3).unwrap();
    assert!(result_blake3.as_xxh3_hash().is_none());
}

#[test]
fn test_to_hex() {
    let file = create_test_file(b"test");
    let result = hash_file(file.path(), HashAlgorithm::Blake3).unwrap();

    let hex = result.to_hex();
    // Should be all lowercase hex characters
    assert!(
        hex.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
    );
    // Blake3 produces 32 bytes = 64 hex characters
    assert_eq!(hex.len(), 64);
}

#[test]
fn test_to_hex_xxh3() {
    let file = create_test_file(b"test");
    let result = hash_file(file.path(), HashAlgorithm::Xxh3).unwrap();

    let hex = result.to_hex();
    // XXH3 produces 16 bytes = 32 hex characters
    assert_eq!(hex.len(), 32);
}

#[test]
fn test_hash_with_progress() {
    let content = b"test content for progress tracking";
    let file = create_test_file(content);
    let mut buffer = vec![0u8; 16]; // Small buffer to ensure multiple callbacks

    let progress_values = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let progress_clone = progress_values.clone();

    let result =
        hash_file_with_progress(file.path(), HashAlgorithm::Blake3, &mut buffer, |bytes| {
            progress_clone.lock().unwrap().push(bytes);
        })
        .unwrap();

    assert_eq!(result.bytes_hashed, content.len() as u64);

    let values = progress_values.lock().unwrap();
    // Should have recorded progress
    assert!(!values.is_empty());
    // Final value should equal total bytes
    assert_eq!(*values.last().unwrap(), content.len() as u64);
    // Progress should be monotonically increasing
    for i in 1..values.len() {
        assert!(values[i] >= values[i - 1]);
    }
}

#[test]
fn test_hash_with_progress_xxh3() {
    let content = b"test content for xxh3 progress";
    let file = create_test_file(content);
    let mut buffer = vec![0u8; 16];

    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let call_count_clone = call_count.clone();

    let result = hash_file_with_progress(file.path(), HashAlgorithm::Xxh3, &mut buffer, |_| {
        call_count_clone.fetch_add(1, Ordering::Relaxed);
    })
    .unwrap();

    assert_eq!(result.bytes_hashed, content.len() as u64);
    assert!(call_count.load(Ordering::Relaxed) > 0);
}

#[test]
fn test_hash_with_atomic_progress() {
    let content = b"test content for atomic progress";
    let file = create_test_file(content);
    let mut buffer = vec![0u8; 128 * 1024];

    let progress = Arc::new(AtomicProgress::new(1, content.len() as u64));

    let result =
        hash_file_with_atomic_progress(file.path(), HashAlgorithm::Blake3, &mut buffer, &progress)
            .unwrap();

    assert_eq!(result.bytes_hashed, content.len() as u64);
    // Progress should have been updated
    assert_eq!(progress.bytes_done(), content.len() as u64);
}

#[test]
fn test_hash_with_atomic_progress_xxh3() {
    let content = b"test content for atomic xxh3";
    let file = create_test_file(content);
    let mut buffer = vec![0u8; 128 * 1024];

    let progress = Arc::new(AtomicProgress::new(1, content.len() as u64));

    let result =
        hash_file_with_atomic_progress(file.path(), HashAlgorithm::Xxh3, &mut buffer, &progress)
            .unwrap();

    assert_eq!(result.bytes_hashed, content.len() as u64);
    assert_eq!(progress.bytes_done(), content.len() as u64);
}

#[test]
fn test_different_content_different_hash() {
    let file1 = create_test_file(b"content A");
    let file2 = create_test_file(b"content B");

    let hash1 = hash_file(file1.path(), HashAlgorithm::Blake3).unwrap();
    let hash2 = hash_file(file2.path(), HashAlgorithm::Blake3).unwrap();

    assert_ne!(hash1.to_hex(), hash2.to_hex());
}

#[test]
fn test_same_content_same_hash() {
    let content = b"identical content";
    let file1 = create_test_file(content);
    let file2 = create_test_file(content);

    let hash1 = hash_file(file1.path(), HashAlgorithm::Blake3).unwrap();
    let hash2 = hash_file(file2.path(), HashAlgorithm::Blake3).unwrap();

    assert_eq!(hash1.to_hex(), hash2.to_hex());
}

#[test]
fn test_hash_result_clone() {
    let file = create_test_file(b"test");
    let result = hash_file(file.path(), HashAlgorithm::Blake3).unwrap();

    let cloned = result.clone();
    assert_eq!(result.to_hex(), cloned.to_hex());
    assert_eq!(result.algorithm, cloned.algorithm);
    assert_eq!(result.bytes_hashed, cloned.bytes_hashed);
}

#[test]
fn test_hash_error_display() {
    let err = HashError::OpenError {
        path: "/test/path".to_string(),
        source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
    };
    let display = format!("{err}");
    assert!(display.contains("/test/path"));
}
