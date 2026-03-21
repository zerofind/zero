use super::*;
use crate::DiffStatus;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_test_entry(path: &str, size: u64, mtime: u64) -> FileEntry {
    FileEntry {
        path: PathBuf::from(path),
        absolute_path: PathBuf::from(format!("/tmp/{}", path)),
        size,
        mtime,
        hash: None,
        hash_xxh3: None,
    }
}

#[test]
fn test_diff_identical_dirs() {
    let source = vec![
        create_test_entry("a.txt", 100, 1000),
        create_test_entry("b.txt", 200, 2000),
    ];
    let dest = vec![
        create_test_entry("a.txt", 100, 1000),
        create_test_entry("b.txt", 200, 2000),
    ];

    let result = diff(&source, &dest);

    assert_eq!(result.identical_count, 2);
    assert_eq!(result.added_count, 0);
    assert_eq!(result.removed_count, 0);
    assert_eq!(result.modified_count, 0);
    assert!(result.is_identical());
}

#[test]
fn test_diff_added_files() {
    let source = vec![
        create_test_entry("a.txt", 100, 1000),
        create_test_entry("b.txt", 200, 2000),
        create_test_entry("c.txt", 300, 3000), // new file
    ];
    let dest = vec![
        create_test_entry("a.txt", 100, 1000),
        create_test_entry("b.txt", 200, 2000),
    ];

    let result = diff(&source, &dest);

    assert_eq!(result.added_count, 1);
    assert_eq!(result.identical_count, 2);

    let added: Vec<_> = result.entries_by_status(DiffStatus::Added).collect();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].path, PathBuf::from("c.txt"));
}

#[test]
fn test_diff_removed_files() {
    let source = vec![create_test_entry("a.txt", 100, 1000)];
    let dest = vec![
        create_test_entry("a.txt", 100, 1000),
        create_test_entry("orphan.txt", 500, 5000), // only in dest
    ];

    let result = diff(&source, &dest);

    assert_eq!(result.removed_count, 1);
    assert_eq!(result.identical_count, 1);

    let removed: Vec<_> = result.entries_by_status(DiffStatus::Removed).collect();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].path, PathBuf::from("orphan.txt"));
}

#[test]
fn test_diff_modified_files() {
    let source = vec![
        create_test_entry("a.txt", 100, 1000),
        create_test_entry("b.txt", 250, 2500), // different size and mtime
    ];
    let dest = vec![
        create_test_entry("a.txt", 100, 1000),
        create_test_entry("b.txt", 200, 2000),
    ];

    let result = diff(&source, &dest);

    assert_eq!(result.modified_count, 1);
    assert_eq!(result.identical_count, 1);
    assert_eq!(result.bytes_to_transfer, 250);

    let modified: Vec<_> = result.entries_by_status(DiffStatus::Modified).collect();
    assert_eq!(modified.len(), 1);
    assert_eq!(modified[0].path, PathBuf::from("b.txt"));
}

#[test]
fn test_diff_include_identical() {
    let source = vec![
        create_test_entry("a.txt", 100, 1000),
        create_test_entry("b.txt", 200, 2000),
    ];
    let dest = vec![
        create_test_entry("a.txt", 100, 1000),
        create_test_entry("b.txt", 200, 2000),
    ];

    let options = DiffOptions {
        include_identical: true,
        ..Default::default()
    };
    let result = diff_with_options(&source, &dest, &options);

    assert_eq!(result.entries.len(), 2);
    assert_eq!(result.identical_count, 2);
}

#[test]
fn test_diff_empty_source() {
    let source: Vec<FileEntry> = vec![];
    let dest = vec![
        create_test_entry("a.txt", 100, 1000),
        create_test_entry("b.txt", 200, 2000),
    ];

    let result = diff(&source, &dest);

    assert_eq!(result.removed_count, 2);
    assert_eq!(result.added_count, 0);
}

#[test]
fn test_diff_empty_dest() {
    let source = vec![
        create_test_entry("a.txt", 100, 1000),
        create_test_entry("b.txt", 200, 2000),
    ];
    let dest: Vec<FileEntry> = vec![];

    let result = diff(&source, &dest);

    assert_eq!(result.added_count, 2);
    assert_eq!(result.removed_count, 0);
    assert_eq!(result.bytes_to_transfer, 300);
}

#[test]
fn test_diff_with_checksum() {
    // Create actual files for checksum testing
    let dir = TempDir::new().unwrap();
    let source_dir = dir.path().join("source");
    let dest_dir = dir.path().join("dest");
    fs::create_dir(&source_dir).unwrap();
    fs::create_dir(&dest_dir).unwrap();

    // Same content
    File::create(source_dir.join("same.txt"))
        .unwrap()
        .write_all(b"hello")
        .unwrap();
    File::create(dest_dir.join("same.txt"))
        .unwrap()
        .write_all(b"hello")
        .unwrap();

    // Different content, same size
    File::create(source_dir.join("diff.txt"))
        .unwrap()
        .write_all(b"aaaaa")
        .unwrap();
    File::create(dest_dir.join("diff.txt"))
        .unwrap()
        .write_all(b"bbbbb")
        .unwrap();

    let source = vec![
        FileEntry {
            path: PathBuf::from("same.txt"),
            absolute_path: source_dir.join("same.txt"),
            size: 5,
            mtime: 1000,
            hash: None,
            hash_xxh3: None,
        },
        FileEntry {
            path: PathBuf::from("diff.txt"),
            absolute_path: source_dir.join("diff.txt"),
            size: 5,
            mtime: 1000, // Same mtime to force checksum comparison
            hash: None,
            hash_xxh3: None,
        },
    ];

    let dest = vec![
        FileEntry {
            path: PathBuf::from("same.txt"),
            absolute_path: dest_dir.join("same.txt"),
            size: 5,
            mtime: 1000,
            hash: None,
            hash_xxh3: None,
        },
        FileEntry {
            path: PathBuf::from("diff.txt"),
            absolute_path: dest_dir.join("diff.txt"),
            size: 5,
            mtime: 1000,
            hash: None,
            hash_xxh3: None,
        },
    ];

    // Without checksum - both appear identical (same size + mtime)
    let result_metadata = diff(&source, &dest);
    assert_eq!(result_metadata.identical_count, 2);

    // With checksum - detects the difference
    let options = DiffOptions {
        use_checksum: true,
        ..Default::default()
    };
    let result_checksum = diff_with_options(&source, &dest, &options);
    assert_eq!(result_checksum.identical_count, 1);
    assert_eq!(result_checksum.modified_count, 1);
}

#[test]
fn test_diff_bytes_to_transfer() {
    let source = vec![
        create_test_entry("new.txt", 1000, 1000),      // added
        create_test_entry("changed.txt", 2000, 2000),  // modified
        create_test_entry("unchanged.txt", 500, 1000), // identical
    ];
    let dest = vec![
        create_test_entry("changed.txt", 1500, 1500), // will be modified
        create_test_entry("unchanged.txt", 500, 1000), // identical
        create_test_entry("orphan.txt", 3000, 3000),  // removed
    ];

    let result = diff(&source, &dest);

    // bytes_to_transfer = added (1000) + modified (2000)
    assert_eq!(result.bytes_to_transfer, 3000);
}
