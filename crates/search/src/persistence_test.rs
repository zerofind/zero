//! Tests for flat compressed snapshot persistence

use tempfile::TempDir;

use super::node::FileNode;
use super::persistence;
use super::search::SearchIndex;

#[test]
fn roundtrip_empty() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("empty.zidx");

    let index = SearchIndex::new();
    persistence::save_index(&index, &path).unwrap();

    let loaded = persistence::load_index(&path).unwrap();
    assert_eq!(loaded.file_count(), 0);
    assert_eq!(loaded.dir_count(), 0);
}

#[test]
fn roundtrip_with_nodes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.zidx");

    let mut index = SearchIndex::new();
    index.insert(FileNode::file(
        "Documents/report.pdf".into(),
        1024,
        1700000000,
    ));
    index.insert(FileNode::file("src/main.rs".into(), 500, 1700000001));
    index.insert(FileNode::directory("src".into(), 1700000000));

    persistence::save_index(&index, &path).unwrap();

    let loaded = persistence::load_index(&path).unwrap();
    assert_eq!(loaded.file_count(), 2);
    assert_eq!(loaded.dir_count(), 1);
    assert_eq!(loaded.node_count(), 3);

    // Search should work after load
    let results = loaded.search("report", 10);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].node.name(), "report.pdf");

    // Type index should be rebuilt
    let stats = loaded.type_stats();
    assert!(stats.code > 0); // .rs file
}

#[test]
fn invalid_magic_rejected() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bad.zidx");

    std::fs::write(&path, b"BAAD some data here").unwrap();

    let err = persistence::load_index(&path).unwrap_err();
    assert!(err.to_string().contains("magic"));
}

#[test]
fn roots_preserved() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("roots.zidx");

    let mut index = SearchIndex::new();
    index.set_roots(vec!["/Users/me/docs".into(), "/Volumes/ext".into()]);
    index.insert(FileNode::file("/Users/me/docs/a.txt".into(), 10, 0));

    persistence::save_index(&index, &path).unwrap();

    let loaded = persistence::load_index(&path).unwrap();
    assert_eq!(loaded.roots(), &["/Users/me/docs", "/Volumes/ext"]);
}
