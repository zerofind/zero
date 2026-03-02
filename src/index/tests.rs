//! Integration tests for the search index module

use std::fs::{self, File};
use std::io::Write;
use std::sync::Arc;
use tempfile::TempDir;

use super::*;
use crate::scanner::CrawlProgress;

/// Create a test directory structure with files
fn create_test_directory() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create some files
    File::create(root.join("readme.md"))
        .unwrap()
        .write_all(b"# Test Project")
        .unwrap();

    File::create(root.join("config.json"))
        .unwrap()
        .write_all(b"{}")
        .unwrap();

    // Create subdirectories with files
    fs::create_dir(root.join("src")).unwrap();
    File::create(root.join("src/main.rs"))
        .unwrap()
        .write_all(b"fn main() {}")
        .unwrap();
    File::create(root.join("src/lib.rs"))
        .unwrap()
        .write_all(b"pub mod test;")
        .unwrap();
    File::create(root.join("src/config.rs"))
        .unwrap()
        .write_all(b"pub struct Config;")
        .unwrap();

    fs::create_dir(root.join("docs")).unwrap();
    File::create(root.join("docs/readme.md"))
        .unwrap()
        .write_all(b"# Documentation")
        .unwrap();
    File::create(root.join("docs/api.md"))
        .unwrap()
        .write_all(b"# API Reference")
        .unwrap();

    dir
}

// ============================================================================
// Build from path tests
// ============================================================================

#[test]
fn test_build_from_path() {
    let dir = create_test_directory();
    let mut index = SearchIndex::new();

    index.build_from_path(dir.path()).unwrap();

    // Should have indexed all files
    assert_eq!(index.file_count(), 7);
    assert!(index.total_bytes() > 0);
}

#[test]
fn test_build_from_path_with_progress() {
    let dir = create_test_directory();
    let progress = Arc::new(CrawlProgress::new());
    let mut index = SearchIndex::new();

    index
        .build_from_path_with_progress(dir.path(), Some(Arc::clone(&progress)))
        .unwrap();

    // Progress should reflect what was found
    assert!(progress.files() > 0);
    assert!(progress.bytes() > 0);

    // Index should match progress
    assert_eq!(index.file_count(), progress.files());
}

#[test]
fn test_build_nonexistent_path() {
    let mut index = SearchIndex::new();
    // jwalk doesn't error on nonexistent paths, it just returns empty results
    let result = index.build_from_path(std::path::Path::new(
        "/nonexistent/path/that/does/not/exist",
    ));
    // Either it errors or returns empty index
    if result.is_ok() {
        assert_eq!(index.file_count(), 0);
        assert_eq!(index.dir_count(), 0);
    }
}

// ============================================================================
// Search integration tests
// ============================================================================

#[test]
fn test_search_after_build() {
    let dir = create_test_directory();
    let mut index = SearchIndex::new();
    index.build_from_path(dir.path()).unwrap();

    // Search for readme files
    let results = index.search("readme", 10);
    assert_eq!(results.len(), 2); // readme.md and docs/readme.md

    // Search for config files
    let results = index.search("config", 10);
    assert_eq!(results.len(), 2); // config.json and src/config.rs

    // Search for .rs files by extension (empty query with filter = list all matching)
    let options = SearchOptions::default().with_extension("rs");
    let results = index.search_with_options("", options);
    // Empty query with extension filter lists all files with that extension
    assert_eq!(results.len(), 3); // main.rs, lib.rs, config.rs

    // Search for rust files
    let results = index.search(".rs", 10);
    assert_eq!(results.len(), 3); // main.rs, lib.rs, config.rs
}

#[test]
fn test_search_case_insensitive() {
    let dir = create_test_directory();
    let mut index = SearchIndex::new();
    index.build_from_path(dir.path()).unwrap();

    let results_lower = index.search("readme", 10);
    let results_upper = index.search("README", 10);
    let results_mixed = index.search("ReadMe", 10);

    assert_eq!(results_lower.len(), results_upper.len());
    assert_eq!(results_lower.len(), results_mixed.len());
}

// ============================================================================
// Persistence integration tests
// ============================================================================

#[test]
fn test_save_load_roundtrip() {
    let test_dir = create_test_directory();
    let cache_dir = TempDir::new().unwrap();
    let index_dir = cache_dir.path().join("index_etch");

    // Build index
    let mut index = SearchIndex::new();
    index.build_from_path(test_dir.path()).unwrap();

    let original_count = index.file_count();
    let original_bytes = index.total_bytes();

    // Save via etch
    save_index_via_etch(&index, &index_dir).unwrap();

    // Load via etch
    let store = open_index_store(&index_dir).unwrap();
    let loaded = store.read().clone();

    // Verify counts match
    assert_eq!(loaded.file_count(), original_count);
    assert_eq!(loaded.total_bytes(), original_bytes);

    // Verify search still works
    let results = loaded.search("config", 10);
    assert_eq!(results.len(), 2);
}

#[test]
fn test_persistence_search_results_match() {
    let test_dir = create_test_directory();
    let cache_dir = TempDir::new().unwrap();
    let index_dir = cache_dir.path().join("index_etch");

    // Build and search
    let mut index = SearchIndex::new();
    index.build_from_path(test_dir.path()).unwrap();
    let original_results = index.search("main", 10);

    // Save and load via etch
    save_index_via_etch(&index, &index_dir).unwrap();
    let store = open_index_store(&index_dir).unwrap();
    let loaded = store.read().clone();
    let loaded_results = loaded.search("main", 10);

    // Results should be identical
    assert_eq!(original_results.len(), loaded_results.len());
    for (orig, load) in original_results.iter().zip(loaded_results.iter()) {
        assert_eq!(orig.node.name, load.node.name);
        assert_eq!(orig.node.path, load.node.path);
        assert_eq!(orig.score, load.score);
    }
}

// ============================================================================
// Large index tests
// ============================================================================

#[test]
fn test_large_index() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create many files
    for i in 0..1000 {
        let subdir = root.join(format!("dir{}", i % 10));
        fs::create_dir_all(&subdir).ok();
        File::create(subdir.join(format!("file{}.txt", i)))
            .unwrap()
            .write_all(format!("content {}", i).as_bytes())
            .unwrap();
    }

    let mut index = SearchIndex::new();
    index.build_from_path(root).unwrap();

    assert_eq!(index.file_count(), 1000);

    // Search should still be fast
    let start = std::time::Instant::now();
    let results = index.search("file", 100);
    let elapsed = start.elapsed();

    assert_eq!(results.len(), 100);
    assert!(
        elapsed.as_millis() < 100,
        "Search took too long: {:?}",
        elapsed
    );
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_empty_directory() {
    let dir = TempDir::new().unwrap();
    let mut index = SearchIndex::new();

    index.build_from_path(dir.path()).unwrap();

    // Empty directory has no files
    assert_eq!(index.file_count(), 0);
    // May have 0 or 1 dirs depending on whether root is counted
    // The important thing is no files

    let results = index.search("anything", 10);
    assert!(results.is_empty());
}

#[test]
fn test_special_characters_in_filename() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create files with special characters
    File::create(root.join("file with spaces.txt")).unwrap();
    File::create(root.join("file-with-dashes.txt")).unwrap();
    File::create(root.join("file_with_underscores.txt")).unwrap();
    File::create(root.join("file.multiple.dots.txt")).unwrap();

    let mut index = SearchIndex::new();
    index.build_from_path(root).unwrap();

    assert_eq!(index.file_count(), 4);

    // Search should handle special chars
    let results = index.search("spaces", 10);
    assert_eq!(results.len(), 1);

    let results = index.search("dashes", 10);
    assert_eq!(results.len(), 1);
}

#[test]
fn test_unicode_filenames() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create files with unicode names
    File::create(root.join("日本語.txt")).unwrap();
    File::create(root.join("émoji_🎉.txt")).unwrap();
    File::create(root.join("中文文件.md")).unwrap();

    let mut index = SearchIndex::new();
    index.build_from_path(root).unwrap();

    assert_eq!(index.file_count(), 3);

    // Search should work with unicode
    let results = index.search("日本語", 10);
    assert_eq!(results.len(), 1);

    let results = index.search("🎉", 10);
    assert_eq!(results.len(), 1);
}

#[test]
fn test_deeply_nested() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create deeply nested structure
    let deep_path = root.join("a/b/c/d/e/f/g/h/i/j");
    fs::create_dir_all(&deep_path).unwrap();
    File::create(deep_path.join("deep_file.txt")).unwrap();

    let mut index = SearchIndex::new();
    index.build_from_path(root).unwrap();

    let results = index.search("deep_file", 10);
    assert_eq!(results.len(), 1);
    assert!(results[0].node.path.contains("a/b/c/d/e/f/g/h/i/j"));
}

// ============================================================================
// insert_index_memory_only tests
// ============================================================================

#[test]
fn test_insert_index_memory_only() {
    let dir = TempDir::new().unwrap();
    let indexes_dir = dir.path().join("indexes");
    let test_root = dir.path().join("test_root");

    // Create test files
    fs::create_dir_all(&test_root).unwrap();
    File::create(test_root.join("alpha.txt"))
        .unwrap()
        .write_all(b"hello")
        .unwrap();
    File::create(test_root.join("beta.rs"))
        .unwrap()
        .write_all(b"fn main() {}")
        .unwrap();

    let root_str = test_root.to_str().unwrap();

    // Build index and save to disk manually (simulating background thread work)
    let mut index = SearchIndex::new();
    index.build_from_path(&test_root).unwrap();
    let count = index.file_count();
    assert_eq!(count, 2);

    let hash = manager::hash_path(root_str);
    let etch_dir = indexes_dir.join(&hash);
    save_index_via_etch(&index, &etch_dir).unwrap();

    // Create manager and insert via memory-only path (simulating main thread)
    let mut manager = IndexManager::with_dir(indexes_dir).unwrap();
    assert!(!manager.has_root(root_str));

    manager.insert_index_memory_only(root_str, index, count);

    // Verify root is registered
    assert!(manager.has_root(root_str));
    assert_eq!(manager.total_file_count(), 2);

    // Verify search works
    let results = manager.search("alpha", 10);
    assert_eq!(results.len(), 1);
    assert!(results[0].node.name.contains("alpha"));

    let results = manager.search("beta", 10);
    assert_eq!(results.len(), 1);
}

#[test]
fn test_insert_index_memory_only_stats() {
    let dir = TempDir::new().unwrap();
    let indexes_dir = dir.path().join("indexes");
    let test_root = dir.path().join("root");

    fs::create_dir_all(test_root.join("sub")).unwrap();
    File::create(test_root.join("file.txt"))
        .unwrap()
        .write_all(b"some content here")
        .unwrap();
    File::create(test_root.join("sub/nested.md"))
        .unwrap()
        .write_all(b"# Title")
        .unwrap();

    let root_str = test_root.to_str().unwrap();

    let mut index = SearchIndex::new();
    index.build_from_path(&test_root).unwrap();
    let count = index.file_count();

    let mut manager = IndexManager::with_dir(indexes_dir).unwrap();
    manager.insert_index_memory_only(root_str, index, count);

    // Check stats are populated
    let stats = manager.root_stats(root_str).unwrap();
    assert_eq!(stats.file_count, 2);
    assert!(stats.dir_count >= 1); // at least "sub"
    assert!(stats.total_bytes > 0);
    assert!(stats.last_indexed > 0);
}

// ============================================================================
// IndexManager Clone tests
// ============================================================================

#[test]
fn test_manager_clone_preserves_indexes() {
    let dir = TempDir::new().unwrap();
    let indexes_dir = dir.path().join("indexes");
    let test_root = dir.path().join("root");

    fs::create_dir_all(&test_root).unwrap();
    File::create(test_root.join("report.pdf"))
        .unwrap()
        .write_all(b"pdf data")
        .unwrap();
    File::create(test_root.join("notes.txt"))
        .unwrap()
        .write_all(b"my notes")
        .unwrap();
    File::create(test_root.join("photo.jpg"))
        .unwrap()
        .write_all(b"jpg data")
        .unwrap();

    let root_str = test_root.to_str().unwrap();

    // Build manager with one root
    let mut original = IndexManager::with_dir(indexes_dir).unwrap();
    original.add_root(root_str).unwrap();

    assert_eq!(original.total_file_count(), 3);

    // Clone it
    let cloned = original.clone();

    // Clone should have the same data
    assert_eq!(cloned.total_file_count(), 3);
    assert!(cloned.has_root(root_str));
    assert_eq!(cloned.roots().len(), 1);

    // Search should work identically on both
    let orig_results = original.search("report", 10);
    let clone_results = cloned.search("report", 10);
    assert_eq!(orig_results.len(), clone_results.len());
    assert_eq!(orig_results.len(), 1);

    // Stats should match
    let orig_stats = original.root_stats(root_str).unwrap();
    let clone_stats = cloned.root_stats(root_str).unwrap();
    assert_eq!(orig_stats.file_count, clone_stats.file_count);
    assert_eq!(orig_stats.total_bytes, clone_stats.total_bytes);
}

#[test]
fn test_manager_clone_is_independent() {
    let dir = TempDir::new().unwrap();
    let indexes_dir = dir.path().join("indexes");
    let root1 = dir.path().join("root1");
    let root2 = dir.path().join("root2");

    fs::create_dir_all(&root1).unwrap();
    fs::create_dir_all(&root2).unwrap();
    File::create(root1.join("a.txt")).unwrap();
    File::create(root2.join("b.txt")).unwrap();

    let root1_str = root1.to_str().unwrap();
    let root2_str = root2.to_str().unwrap();

    // Build manager with root1
    let mut original = IndexManager::with_dir(indexes_dir).unwrap();
    original.add_root(root1_str).unwrap();

    // Clone before adding root2
    let cloned = original.clone();

    // Add root2 only to original
    original.add_root(root2_str).unwrap();

    // Original has both roots, clone only has root1
    assert_eq!(original.roots().len(), 2);
    assert_eq!(cloned.roots().len(), 1);
    assert!(cloned.has_root(root1_str));
    assert!(!cloned.has_root(root2_str));
}
