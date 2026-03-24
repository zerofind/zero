//! Tests for scan filter

use super::*;
use std::path::Path;

// ============================================================================
// Default filter tests
// ============================================================================

#[test]
fn test_default_ignores_ds_store() {
    let filter = ScanFilter::new();
    assert!(filter.should_ignore(".DS_Store"));
    assert!(filter.should_ignore_path(Path::new("/some/path/.DS_Store")));
}

#[test]
fn test_default_ignores_spotlight() {
    let filter = ScanFilter::new();
    assert!(filter.should_ignore(".Spotlight-V100"));
}

#[test]
fn test_default_ignores_trashes() {
    let filter = ScanFilter::new();
    assert!(filter.should_ignore(".Trashes"));
}

#[test]
fn test_default_ignores_fseventsd() {
    let filter = ScanFilter::new();
    assert!(filter.should_ignore(".fseventsd"));
}

#[test]
fn test_default_ignores_temporary_items() {
    let filter = ScanFilter::new();
    assert!(filter.should_ignore(".TemporaryItems"));
}

#[test]
fn test_default_ignores_apple_double() {
    let filter = ScanFilter::new();
    assert!(filter.should_ignore("._somefile"));
    assert!(filter.should_ignore("._document.pdf"));
    assert!(filter.should_ignore_path(Path::new("/path/._document.pdf")));
}

#[test]
fn test_default_ignores_zero() {
    let filter = ScanFilter::new();
    assert!(filter.should_ignore(".zero"));
}

#[test]
fn test_does_not_ignore_regular_files() {
    let filter = ScanFilter::new();
    assert!(!filter.should_ignore("document.pdf"));
    assert!(!filter.should_ignore("photo.jpg"));
    assert!(!filter.should_ignore("video.mp4"));
    assert!(!filter.should_ignore("archive.zip"));
}

#[test]
fn test_does_not_ignore_dotfiles() {
    let filter = ScanFilter::new();
    assert!(!filter.should_ignore(".gitignore"));
    assert!(!filter.should_ignore(".bashrc"));
    assert!(!filter.should_ignore(".zshrc"));
    assert!(!filter.should_ignore(".env"));
}

// ============================================================================
// Empty filter tests
// ============================================================================

#[test]
fn test_empty_filter_ignores_nothing() {
    let filter = ScanFilter::empty();
    assert!(!filter.should_ignore(".DS_Store"));
    assert!(!filter.should_ignore(".Spotlight-V100"));
    assert!(!filter.should_ignore("._somefile"));
}

#[test]
fn test_empty_filter_patterns_empty() {
    let filter = ScanFilter::empty();
    // Even system files should pass through
    assert!(!filter.should_ignore(".Trashes"));
    assert!(!filter.should_ignore(".fseventsd"));
}

// ============================================================================
// Custom pattern tests
// ============================================================================

#[test]
fn test_custom_pattern_exact() {
    let mut filter = ScanFilter::empty();
    filter.add_pattern("specific_file.txt");
    assert!(filter.should_ignore("specific_file.txt"));
    assert!(!filter.should_ignore("other_file.txt"));
}

#[test]
fn test_custom_pattern_suffix_wildcard() {
    let mut filter = ScanFilter::empty();
    filter.add_pattern("*.tmp");
    assert!(filter.should_ignore("file.tmp"));
    assert!(filter.should_ignore("another.tmp"));
    assert!(!filter.should_ignore("file.txt"));
    assert!(!filter.should_ignore("tmp"));
}

#[test]
fn test_custom_pattern_prefix_wildcard() {
    let mut filter = ScanFilter::empty();
    filter.add_pattern("temp*");
    assert!(filter.should_ignore("temporary"));
    assert!(filter.should_ignore("temp"));
    assert!(filter.should_ignore("temp_file.txt"));
    assert!(!filter.should_ignore("atemp"));
}

#[test]
fn test_contains_pattern() {
    let filter = ScanFilter::empty().with_patterns(["*.sqlite*"]);
    assert!(filter.should_ignore("database.sqlite"));
    assert!(filter.should_ignore("database.sqlite-wal"));
    assert!(filter.should_ignore("database.sqlite-shm"));
    assert!(filter.should_ignore("my.sqlite.backup"));
    assert!(!filter.should_ignore("database.db"));
    assert!(!filter.should_ignore("sqlite_readme.txt")); // doesn't have the dot
}

#[test]
fn test_contains_pattern_map() {
    let filter = ScanFilter::empty().with_patterns(["*.map.*"]);
    assert!(filter.should_ignore("dbStr-1.map.buckets"));
    assert!(filter.should_ignore("dbStr-2.map.data"));
    assert!(filter.should_ignore("file.map.offsets"));
    assert!(!filter.should_ignore("map.txt"));
    assert!(!filter.should_ignore("sitemap.xml"));
}

#[test]
fn test_with_patterns_builder() {
    let filter = ScanFilter::empty().with_patterns(["*.log", "temp*"]);
    assert!(filter.should_ignore("debug.log"));
    assert!(filter.should_ignore("error.log"));
    assert!(filter.should_ignore("temporary"));
    assert!(filter.should_ignore("temp_data"));
    assert!(!filter.should_ignore("file.txt"));
}

#[test]
fn test_with_patterns_multiple_calls() {
    let filter = ScanFilter::empty()
        .with_patterns(["*.log"])
        .with_patterns(["*.tmp"]);
    assert!(filter.should_ignore("debug.log"));
    assert!(filter.should_ignore("file.tmp"));
}

#[test]
fn test_add_pattern_after_creation() {
    let mut filter = ScanFilter::new();
    filter.add_pattern("*.custom");

    // Should still have default patterns
    assert!(filter.should_ignore(".DS_Store"));
    // Plus custom pattern
    assert!(filter.should_ignore("file.custom"));
}

// ============================================================================
// Path-based filtering tests
// ============================================================================

#[test]
fn test_should_ignore_path_extracts_filename() {
    let filter = ScanFilter::new();
    assert!(filter.should_ignore_path(Path::new("/very/long/path/to/.DS_Store")));
    assert!(filter.should_ignore_path(Path::new("relative/path/.DS_Store")));
}

#[test]
fn test_should_ignore_path_with_no_filename() {
    let filter = ScanFilter::new();
    // Root path has no filename component
    assert!(!filter.should_ignore_path(Path::new("/")));
}

#[test]
fn test_should_ignore_path_regular_file() {
    let filter = ScanFilter::new();
    assert!(!filter.should_ignore_path(Path::new("/path/to/document.pdf")));
    assert!(!filter.should_ignore_path(Path::new("/path/to/image.png")));
}

// ============================================================================
// Default trait tests
// ============================================================================

#[test]
fn test_default_trait() {
    let filter = ScanFilter::default();
    // Default should be same as new()
    assert!(filter.should_ignore(".DS_Store"));
    assert!(filter.should_ignore(".Spotlight-V100"));
}

// ============================================================================
// Clone and Debug tests
// ============================================================================

#[test]
fn test_clone() {
    let filter = ScanFilter::empty().with_patterns(["*.test"]);
    let cloned = filter.clone();

    assert!(cloned.should_ignore("file.test"));
}

#[test]
fn test_debug() {
    let filter = ScanFilter::new();
    let debug_str = format!("{filter:?}");
    assert!(debug_str.contains("ScanFilter"));
    assert!(debug_str.contains("patterns"));
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_empty_filename() {
    let filter = ScanFilter::new();
    assert!(!filter.should_ignore(""));
}

#[test]
fn test_pattern_with_spaces() {
    let filter = ScanFilter::empty().with_patterns(["file with spaces.txt"]);
    assert!(filter.should_ignore("file with spaces.txt"));
    assert!(!filter.should_ignore("filewithspaces.txt"));
}

#[test]
fn test_case_sensitive() {
    let filter = ScanFilter::new();
    // Patterns are case-sensitive
    assert!(filter.should_ignore(".DS_Store"));
    assert!(!filter.should_ignore(".ds_store"));
    assert!(!filter.should_ignore(".DS_STORE"));
}

#[test]
fn test_multiple_wildcards_not_supported() {
    // Only prefix/suffix wildcards are supported, not glob patterns
    let filter = ScanFilter::empty().with_patterns(["*.test.*"]);
    // This should work as a contains pattern
    assert!(filter.should_ignore("file.test.txt"));
    assert!(filter.should_ignore("a.test.b"));
}
