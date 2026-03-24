//! Tests for duplicate file finder

use super::*;
use crate::types::*;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

/// Test options with a low min_size so small test files are included.
fn test_options() -> DedupOptions {
    DedupOptions {
        min_size: 1,
        ..Default::default()
    }
}

fn create_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut file = File::create(&path).unwrap();
    file.write_all(content).unwrap();
    path
}

#[test]
fn test_find_no_duplicates() {
    let dir = TempDir::new().unwrap();
    create_file(dir.path(), "a.txt", b"content a");
    create_file(dir.path(), "b.txt", b"content b");
    create_file(dir.path(), "c.txt", b"content c");

    let result = find_duplicates(dir.path(), &test_options()).unwrap();

    assert!(!result.has_duplicates());
    assert_eq!(result.groups.len(), 0);
    assert_eq!(result.files_scanned, 3);
}

#[test]
fn test_find_duplicates() {
    let dir = TempDir::new().unwrap();
    create_file(dir.path(), "a.txt", b"same content");
    create_file(dir.path(), "b.txt", b"same content");
    create_file(dir.path(), "c.txt", b"different");

    let result = find_duplicates(dir.path(), &test_options()).unwrap();

    assert!(result.has_duplicates());
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.groups[0].files.len(), 2);
    assert_eq!(result.duplicate_count, 1);
    assert_eq!(result.wasted_bytes, 12); // "same content" = 12 bytes
}

#[test]
fn test_find_multiple_duplicate_groups() {
    let dir = TempDir::new().unwrap();
    create_file(dir.path(), "a1.txt", b"content A");
    create_file(dir.path(), "a2.txt", b"content A");
    create_file(dir.path(), "b1.txt", b"content B");
    create_file(dir.path(), "b2.txt", b"content B");
    create_file(dir.path(), "unique.txt", b"unique");

    let result = find_duplicates(dir.path(), &test_options()).unwrap();

    assert!(result.has_duplicates());
    assert_eq!(result.groups.len(), 2);
    assert_eq!(result.duplicate_count, 2);
}

#[test]
fn test_keeper_shortest_path() {
    let dir = TempDir::new().unwrap();
    create_file(dir.path(), "a.txt", b"content");
    create_file(dir.path(), "subdir/longer/path/a.txt", b"content");

    let result = find_duplicates(dir.path(), &test_options()).unwrap();

    assert_eq!(result.groups.len(), 1);
    let keeper = result.groups[0].keeper().unwrap();
    // Shorter path should be keeper
    assert!(!keeper.to_string_lossy().contains("subdir"));
}

#[test]
fn test_delete_duplicates() {
    let dir = TempDir::new().unwrap();
    let keep = create_file(dir.path(), "a.txt", b"content");
    let dup = create_file(dir.path(), "subdir/duplicate.txt", b"content");

    let result = find_duplicates(dir.path(), &test_options()).unwrap();
    assert_eq!(result.duplicate_count, 1);

    let delete_result = delete_duplicates(&result);

    assert_eq!(delete_result.files_deleted, 1);
    assert_eq!(delete_result.bytes_reclaimed, 7); // "content" = 7 bytes
    assert!(keep.exists());
    assert!(!dup.exists());
}

#[test]
fn test_min_size_filter() {
    let dir = TempDir::new().unwrap();
    create_file(dir.path(), "tiny1.txt", b"a");
    create_file(dir.path(), "tiny2.txt", b"a");
    create_file(dir.path(), "big1.txt", b"bigger content here");
    create_file(dir.path(), "big2.txt", b"bigger content here");

    let options = DedupOptions {
        min_size: 10, // Skip files < 10 bytes
        ..Default::default()
    };
    let result = find_duplicates(dir.path(), &options).unwrap();

    // Should only find the "big" duplicates, not the tiny ones
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.groups[0].size, 19); // "bigger content here"
}

#[test]
fn test_wasted_bytes() {
    let group = DuplicateGroup {
        hash: "abc".to_string(),
        size: 100,
        files: vec![
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            PathBuf::from("/c"),
        ],
    };

    // 3 files of 100 bytes, 2 are duplicates = 200 wasted
    assert_eq!(group.wasted_bytes(), 200);
}

#[test]
fn test_verify_mode() {
    let dir = TempDir::new().unwrap();
    create_file(dir.path(), "a.txt", b"content");
    create_file(dir.path(), "b.txt", b"content");

    let options = DedupOptions {
        min_size: 1,
        verify: true,
        ..Default::default()
    };
    let result = find_duplicates(dir.path(), &options).unwrap();

    assert!(result.has_duplicates());
    assert_eq!(result.groups.len(), 1);
}

#[test]
fn test_duplicates_to_delete() {
    let group = DuplicateGroup {
        hash: "abc".to_string(),
        size: 100,
        files: vec![
            PathBuf::from("/short"),
            PathBuf::from("/much/longer/path"),
            PathBuf::from("/another/long/path"),
        ],
    };

    let to_delete = group.duplicates_to_delete();
    assert_eq!(to_delete.len(), 2);
    // Keeper should be /short (shortest path)
    assert!(to_delete.iter().all(|p| p.as_path() != Path::new("/short")));
}

#[test]
fn test_wasted_bytes_single_file() {
    let group = DuplicateGroup {
        hash: "abc".to_string(),
        size: 100,
        files: vec![PathBuf::from("/only_one")],
    };

    // Single file = no wasted bytes
    assert_eq!(group.wasted_bytes(), 0);
}

#[test]
fn test_dedup_result_has_duplicates() {
    let empty_result = DedupResult {
        groups: vec![],
        files_scanned: 10,
        bytes_scanned: 1000,
        duplicate_count: 0,
        wasted_bytes: 0,
    };
    assert!(!empty_result.has_duplicates());

    let with_dups = DedupResult {
        groups: vec![DuplicateGroup {
            hash: "abc".to_string(),
            size: 100,
            files: vec![PathBuf::from("/a"), PathBuf::from("/b")],
        }],
        files_scanned: 10,
        bytes_scanned: 1000,
        duplicate_count: 1,
        wasted_bytes: 100,
    };
    assert!(with_dups.has_duplicates());
}
