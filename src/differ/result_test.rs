//! Tests for diff result types

use super::*;
use std::path::PathBuf;

#[test]
fn test_diff_status_needs_action() {
    assert!(DiffStatus::Added.needs_action());
    assert!(DiffStatus::Removed.needs_action());
    assert!(DiffStatus::Modified.needs_action());
    assert!(!DiffStatus::Identical.needs_action());
}

#[test]
fn test_diff_status_needs_transfer() {
    assert!(DiffStatus::Added.needs_transfer());
    assert!(DiffStatus::Modified.needs_transfer());
    assert!(!DiffStatus::Removed.needs_transfer());
    assert!(!DiffStatus::Identical.needs_transfer());
}

#[test]
fn test_diff_status_symbol() {
    assert_eq!(DiffStatus::Added.symbol(), "+");
    assert_eq!(DiffStatus::Removed.symbol(), "-");
    assert_eq!(DiffStatus::Modified.symbol(), "~");
    assert_eq!(DiffStatus::Identical.symbol(), "=");
}

#[test]
fn test_diff_status_name() {
    assert_eq!(DiffStatus::Added.name(), "added");
    assert_eq!(DiffStatus::Removed.name(), "removed");
    assert_eq!(DiffStatus::Modified.name(), "modified");
    assert_eq!(DiffStatus::Identical.name(), "identical");
}

#[test]
fn test_diff_entry_added() {
    let entry = DiffEntry::added(PathBuf::from("test.txt"), 100, 1000);
    assert_eq!(entry.status, DiffStatus::Added);
    assert_eq!(entry.source_size, Some(100));
    assert_eq!(entry.dest_size, None);
    assert_eq!(entry.source_mtime, Some(1000));
    assert_eq!(entry.dest_mtime, None);
}

#[test]
fn test_diff_entry_removed() {
    let entry = DiffEntry::removed(PathBuf::from("test.txt"), 100, 1000);
    assert_eq!(entry.status, DiffStatus::Removed);
    assert_eq!(entry.source_size, None);
    assert_eq!(entry.dest_size, Some(100));
    assert_eq!(entry.source_mtime, None);
    assert_eq!(entry.dest_mtime, Some(1000));
}

#[test]
fn test_diff_entry_modified() {
    let entry = DiffEntry::modified(PathBuf::from("test.txt"), 100, 1000, 200, 2000);
    assert_eq!(entry.status, DiffStatus::Modified);
    assert_eq!(entry.source_size, Some(100));
    assert_eq!(entry.dest_size, Some(200));
    assert_eq!(entry.source_mtime, Some(1000));
    assert_eq!(entry.dest_mtime, Some(2000));
}

#[test]
fn test_diff_entry_identical() {
    let entry = DiffEntry::identical(PathBuf::from("test.txt"), 100, 1000);
    assert_eq!(entry.status, DiffStatus::Identical);
    assert_eq!(entry.source_size, Some(100));
    assert_eq!(entry.dest_size, Some(100));
    assert_eq!(entry.source_mtime, Some(1000));
    assert_eq!(entry.dest_mtime, Some(1000));
}

#[test]
fn test_diff_result_counts() {
    let mut result = DiffResult::new();

    result.push(DiffEntry::added(PathBuf::from("a.txt"), 100, 0));
    result.push(DiffEntry::added(PathBuf::from("b.txt"), 200, 0));
    result.push(DiffEntry::removed(PathBuf::from("c.txt"), 50, 0));
    result.push(DiffEntry::modified(PathBuf::from("d.txt"), 300, 0, 250, 0));
    result.push(DiffEntry::identical(PathBuf::from("e.txt"), 500, 0));

    assert_eq!(result.added_count, 2);
    assert_eq!(result.removed_count, 1);
    assert_eq!(result.modified_count, 1);
    assert_eq!(result.identical_count, 1);
    assert_eq!(result.total_count(), 5);
    assert_eq!(result.changes_count(), 4);
    assert_eq!(result.bytes_to_transfer, 600); // 100 + 200 + 300
    assert!(!result.is_identical());
}

#[test]
fn test_identical_result() {
    let mut result = DiffResult::new();
    result.push(DiffEntry::identical(PathBuf::from("a.txt"), 100, 0));
    result.push(DiffEntry::identical(PathBuf::from("b.txt"), 200, 0));

    assert!(result.is_identical());
    assert_eq!(result.changes_count(), 0);
}

#[test]
fn test_entries_by_status() {
    let mut result = DiffResult::new();
    result.push(DiffEntry::added(PathBuf::from("a.txt"), 100, 0));
    result.push(DiffEntry::removed(PathBuf::from("b.txt"), 100, 0));
    result.push(DiffEntry::added(PathBuf::from("c.txt"), 100, 0));

    let added: Vec<_> = result.entries_by_status(DiffStatus::Added).collect();
    assert_eq!(added.len(), 2);
}

#[test]
fn test_diff_result_with_capacity() {
    let result = DiffResult::with_capacity(100);
    assert!(result.entries.capacity() >= 100);
    assert_eq!(result.total_count(), 0);
}

#[test]
fn test_diff_result_changes() {
    let mut result = DiffResult::new();
    result.push(DiffEntry::added(PathBuf::from("a.txt"), 100, 0));
    result.push(DiffEntry::identical(PathBuf::from("b.txt"), 100, 0));
    result.push(DiffEntry::modified(PathBuf::from("c.txt"), 100, 0, 200, 0));

    let changes: Vec<_> = result.changes().collect();
    assert_eq!(changes.len(), 2);
    assert!(changes.iter().all(|e| e.status.needs_action()));
}

#[test]
fn test_diff_result_default() {
    let result = DiffResult::default();
    assert_eq!(result.total_count(), 0);
    assert!(result.is_identical());
}
