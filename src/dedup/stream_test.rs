//! Tests for the streaming dedup API.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::*;
use crate::dedup::types::{DedupOptions, DedupProgress};

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
        fs::create_dir_all(parent).ok();
    }
    let mut file = File::create(&path).unwrap();
    file.write_all(content).unwrap();
    path
}

#[test]
fn test_streaming_events_order() {
    let dir = tempfile::TempDir::new().unwrap();
    create_file(dir.path(), "a.txt", b"same content here!");
    create_file(dir.path(), "b.txt", b"same content here!");
    create_file(dir.path(), "c.txt", b"unique stuff");

    let progress = Arc::new(DedupProgress::new());
    let (tx, rx) = std::sync::mpsc::channel();

    find_duplicates_streaming(dir.path(), test_options(), progress, tx, None).unwrap();

    let events: Vec<DedupStreamEvent> = rx.into_iter().collect();

    // Must end with Complete
    assert!(matches!(
        events.last(),
        Some(DedupStreamEvent::Complete { .. })
    ));

    // Must have at least one GroupVerified (the a/b pair)
    let group_count = events
        .iter()
        .filter(|e| matches!(e, DedupStreamEvent::GroupVerified(_)))
        .count();
    assert_eq!(group_count, 1);

    // SizeGroupsReady must appear before any GroupVerified
    let sgr_pos = events
        .iter()
        .position(|e| matches!(e, DedupStreamEvent::SizeGroupsReady { .. }));
    let gv_pos = events
        .iter()
        .position(|e| matches!(e, DedupStreamEvent::GroupVerified(_)));
    assert!(sgr_pos.is_some());
    assert!(gv_pos.is_some());
    assert!(sgr_pos.unwrap() < gv_pos.unwrap());
}

#[test]
fn test_streaming_no_duplicates() {
    let dir = tempfile::TempDir::new().unwrap();
    create_file(dir.path(), "a.txt", b"content a");
    create_file(dir.path(), "b.txt", b"content b");

    let progress = Arc::new(DedupProgress::new());
    let (tx, rx) = std::sync::mpsc::channel();

    find_duplicates_streaming(dir.path(), test_options(), progress, tx, None).unwrap();

    let events: Vec<DedupStreamEvent> = rx.into_iter().collect();
    let group_count = events
        .iter()
        .filter(|e| matches!(e, DedupStreamEvent::GroupVerified(_)))
        .count();
    assert_eq!(group_count, 0);
}

#[test]
fn test_streaming_cancellation() {
    let dir = tempfile::TempDir::new().unwrap();
    create_file(dir.path(), "a.txt", b"content");
    create_file(dir.path(), "b.txt", b"content");

    let progress = Arc::new(DedupProgress::new());
    progress.cancel();

    let (tx, rx) = std::sync::mpsc::channel();
    find_duplicates_streaming(dir.path(), test_options(), progress, tx, None).unwrap();

    let events: Vec<DedupStreamEvent> = rx.into_iter().collect();
    // Should complete immediately with no groups
    assert!(matches!(
        events.last(),
        Some(DedupStreamEvent::Complete { .. })
    ));
    let group_count = events
        .iter()
        .filter(|e| matches!(e, DedupStreamEvent::GroupVerified(_)))
        .count();
    assert_eq!(group_count, 0);
}

#[test]
fn test_streaming_largest_groups_first() {
    let dir = tempfile::TempDir::new().unwrap();
    // Small duplicates (10 bytes)
    create_file(dir.path(), "small1.txt", b"small file");
    create_file(dir.path(), "small2.txt", b"small file");
    // Large duplicates (47 bytes)
    create_file(
        dir.path(),
        "big1.txt",
        b"this is a much larger file content for testing!",
    );
    create_file(
        dir.path(),
        "big2.txt",
        b"this is a much larger file content for testing!",
    );

    let progress = Arc::new(DedupProgress::new());
    let (tx, rx) = std::sync::mpsc::channel();

    find_duplicates_streaming(dir.path(), test_options(), progress, tx, None).unwrap();

    let groups: Vec<DuplicateGroup> = rx
        .into_iter()
        .filter_map(|e| match e {
            DedupStreamEvent::GroupVerified(g) => Some(g),
            _ => None,
        })
        .collect();

    assert_eq!(groups.len(), 2);
    // Largest group should come first
    assert!(groups[0].size > groups[1].size);
}
