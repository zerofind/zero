//! Tests for file entry type

use super::*;
use std::path::PathBuf;

#[test]
fn test_file_entry_creation() {
    let entry = FileEntry::new(
        PathBuf::from("foo/bar.txt"),
        PathBuf::from("/root/foo/bar.txt"),
        1024,
        1699900000,
    );

    assert_eq!(entry.path, PathBuf::from("foo/bar.txt"));
    assert_eq!(entry.absolute_path, PathBuf::from("/root/foo/bar.txt"));
    assert_eq!(entry.size, 1024);
    assert_eq!(entry.mtime, 1699900000);
    assert!(entry.hash.is_none());
    assert!(entry.hash_xxh3.is_none());
}

#[test]
fn test_with_hash() {
    let entry = FileEntry::new(
        PathBuf::from("foo.txt"),
        PathBuf::from("/root/foo.txt"),
        100,
        1699900000,
    )
    .with_hash([0u8; 32]);

    assert!(entry.hash.is_some());
    assert_eq!(entry.hash.unwrap(), [0u8; 32]);
}

#[test]
fn test_with_hash_xxh3() {
    let entry = FileEntry::new(
        PathBuf::from("foo.txt"),
        PathBuf::from("/root/foo.txt"),
        100,
        1699900000,
    )
    .with_hash_xxh3([1u8; 16]);

    assert!(entry.hash_xxh3.is_some());
    assert_eq!(entry.hash_xxh3.unwrap(), [1u8; 16]);
}

#[test]
fn test_with_both_hashes() {
    let entry = FileEntry::new(
        PathBuf::from("foo.txt"),
        PathBuf::from("/root/foo.txt"),
        100,
        1699900000,
    )
    .with_hash([2u8; 32])
    .with_hash_xxh3([3u8; 16]);

    assert_eq!(entry.hash, Some([2u8; 32]));
    assert_eq!(entry.hash_xxh3, Some([3u8; 16]));
}

#[test]
fn test_metadata_match() {
    let a = FileEntry::new(
        PathBuf::from("file.txt"),
        PathBuf::from("/a/file.txt"),
        500,
        1699900000,
    );
    let b = FileEntry::new(
        PathBuf::from("file.txt"),
        PathBuf::from("/b/file.txt"),
        500,
        1699900000,
    );

    assert!(a.matches_metadata(&b));
}

#[test]
fn test_metadata_no_match_size() {
    let a = FileEntry::new(
        PathBuf::from("file.txt"),
        PathBuf::from("/a/file.txt"),
        500,
        1699900000,
    );
    let b = FileEntry::new(
        PathBuf::from("file.txt"),
        PathBuf::from("/b/file.txt"),
        600,
        1699900000,
    );

    assert!(!a.matches_metadata(&b));
}

#[test]
fn test_metadata_no_match_mtime() {
    let a = FileEntry::new(
        PathBuf::from("file.txt"),
        PathBuf::from("/a/file.txt"),
        500,
        1699900000,
    );
    let b = FileEntry::new(
        PathBuf::from("file.txt"),
        PathBuf::from("/b/file.txt"),
        500,
        1699900001,
    );

    assert!(!a.matches_metadata(&b));
}

#[test]
fn test_hash_match() {
    let a = FileEntry::new(PathBuf::from("a.txt"), PathBuf::from("/a.txt"), 100, 0)
        .with_hash([1u8; 32]);

    let b = FileEntry::new(PathBuf::from("b.txt"), PathBuf::from("/b.txt"), 100, 0)
        .with_hash([1u8; 32]);

    assert_eq!(a.matches_hash(&b), Some(true));
}

#[test]
fn test_hash_no_match() {
    let a = FileEntry::new(PathBuf::from("a.txt"), PathBuf::from("/a.txt"), 100, 0)
        .with_hash([1u8; 32]);

    let b = FileEntry::new(PathBuf::from("b.txt"), PathBuf::from("/b.txt"), 100, 0)
        .with_hash([2u8; 32]);

    assert_eq!(a.matches_hash(&b), Some(false));
}

#[test]
fn test_hash_match_none_when_missing() {
    let a = FileEntry::new(PathBuf::from("a.txt"), PathBuf::from("/a.txt"), 100, 0)
        .with_hash([1u8; 32]);

    let c = FileEntry::new(PathBuf::from("c.txt"), PathBuf::from("/c.txt"), 100, 0);

    assert_eq!(a.matches_hash(&c), None); // c has no hash
}

#[test]
fn test_hash_match_both_missing() {
    let a = FileEntry::new(PathBuf::from("a.txt"), PathBuf::from("/a.txt"), 100, 0);
    let b = FileEntry::new(PathBuf::from("b.txt"), PathBuf::from("/b.txt"), 100, 0);

    assert_eq!(a.matches_hash(&b), None);
}

#[test]
fn test_ordering() {
    let a = FileEntry::new(
        PathBuf::from("aaa.txt"),
        PathBuf::from("/root/aaa.txt"),
        100,
        0,
    );
    let b = FileEntry::new(
        PathBuf::from("bbb.txt"),
        PathBuf::from("/root/bbb.txt"),
        100,
        0,
    );

    assert!(a < b);
    assert!(b > a);
}

#[test]
fn test_ordering_equal() {
    let a = FileEntry::new(
        PathBuf::from("same.txt"),
        PathBuf::from("/root/same.txt"),
        100,
        0,
    );
    let b = FileEntry::new(
        PathBuf::from("same.txt"),
        PathBuf::from("/other/same.txt"),
        200,
        1000,
    );

    // Ordering is by path only
    assert_eq!(a.cmp(&b), std::cmp::Ordering::Equal);
}

#[test]
fn test_partial_ord() {
    let a = FileEntry::new(PathBuf::from("a.txt"), PathBuf::from("/a.txt"), 100, 0);
    let b = FileEntry::new(PathBuf::from("b.txt"), PathBuf::from("/b.txt"), 100, 0);

    assert_eq!(a.partial_cmp(&b), Some(std::cmp::Ordering::Less));
}

#[test]
fn test_clone() {
    let entry = FileEntry::new(
        PathBuf::from("test.txt"),
        PathBuf::from("/root/test.txt"),
        1024,
        1699900000,
    )
    .with_hash([5u8; 32]);

    let cloned = entry.clone();

    assert_eq!(entry.path, cloned.path);
    assert_eq!(entry.absolute_path, cloned.absolute_path);
    assert_eq!(entry.size, cloned.size);
    assert_eq!(entry.mtime, cloned.mtime);
    assert_eq!(entry.hash, cloned.hash);
}

#[test]
fn test_equality() {
    let a = FileEntry::new(
        PathBuf::from("test.txt"),
        PathBuf::from("/root/test.txt"),
        1024,
        1699900000,
    );
    let b = FileEntry::new(
        PathBuf::from("test.txt"),
        PathBuf::from("/root/test.txt"),
        1024,
        1699900000,
    );

    assert_eq!(a, b);
}

#[test]
fn test_inequality() {
    let a = FileEntry::new(
        PathBuf::from("test.txt"),
        PathBuf::from("/root/test.txt"),
        1024,
        1699900000,
    );
    let b = FileEntry::new(
        PathBuf::from("other.txt"),
        PathBuf::from("/root/other.txt"),
        1024,
        1699900000,
    );

    assert_ne!(a, b);
}

#[test]
fn test_debug() {
    let entry = FileEntry::new(
        PathBuf::from("test.txt"),
        PathBuf::from("/root/test.txt"),
        1024,
        1699900000,
    );

    let debug_str = format!("{:?}", entry);
    assert!(debug_str.contains("test.txt"));
    assert!(debug_str.contains("1024"));
}
