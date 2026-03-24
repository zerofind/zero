//! Tests for parallel directory walker

use super::*;
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;

fn create_test_tree() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create some files
    File::create(root.join("file1.txt"))
        .unwrap()
        .write_all(b"hello")
        .unwrap();
    File::create(root.join("file2.txt"))
        .unwrap()
        .write_all(b"world")
        .unwrap();

    // Create subdirectory with files
    fs::create_dir(root.join("subdir")).unwrap();
    File::create(root.join("subdir/file3.txt"))
        .unwrap()
        .write_all(b"nested")
        .unwrap();

    // Create a .DS_Store file (should be filtered)
    File::create(root.join(".DS_Store"))
        .unwrap()
        .write_all(b"junk")
        .unwrap();

    dir
}

// ============================================================================
// Basic scanning tests
// ============================================================================

#[test]
fn test_scan_basic() {
    let dir = create_test_tree();

    let entries: Vec<_> = scan(dir.path(), &ScanOptions::default()).unwrap().collect();

    // Should have 3 files (excluding .DS_Store)
    let ok_entries: Vec<_> = entries
        .into_iter()
        .filter_map(std::result::Result::ok)
        .collect();
    assert_eq!(ok_entries.len(), 3);
}

#[test]
fn test_scan_collect() {
    let dir = create_test_tree();

    let entries = scan_collect(dir.path(), &ScanOptions::default()).unwrap();
    assert_eq!(entries.len(), 3);
}

#[test]
fn test_scan_empty_directory() {
    let dir = TempDir::new().unwrap();
    let entries = scan_collect(dir.path(), &ScanOptions::default()).unwrap();
    assert!(entries.is_empty());
}

// ============================================================================
// Error handling tests
// ============================================================================

#[test]
fn test_scan_nonexistent() {
    let result = scan(Path::new("/nonexistent/path/xyz"), &ScanOptions::default());
    assert!(matches!(result, Err(ScanError::RootNotFound(_))));
}

#[test]
fn test_scan_file_not_directory() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("file.txt");
    File::create(&file_path)
        .unwrap()
        .write_all(b"test")
        .unwrap();

    let result = scan(&file_path, &ScanOptions::default());
    assert!(matches!(result, Err(ScanError::NotADirectory(_))));
}

// ============================================================================
// Path handling tests
// ============================================================================

#[test]
fn test_relative_paths() {
    let dir = create_test_tree();

    let entries = scan_collect(dir.path(), &ScanOptions::default()).unwrap();

    for entry in entries {
        // All relative paths should not be absolute
        assert!(entry.path.is_relative());
        // All absolute paths should be absolute
        assert!(entry.absolute_path.is_absolute());
    }
}

#[test]
fn test_file_sizes() {
    let dir = create_test_tree();

    let entries = scan_collect(dir.path(), &ScanOptions::default()).unwrap();

    let file1 = entries.iter().find(|e| e.path.ends_with("file1.txt"));
    assert!(file1.is_some());
    assert_eq!(file1.unwrap().size, 5); // "hello" = 5 bytes
}

#[test]
fn test_file_mtime() {
    let dir = create_test_tree();

    let entries = scan_collect(dir.path(), &ScanOptions::default()).unwrap();

    for entry in entries {
        // mtime should be set to something reasonable (not 0)
        assert!(entry.mtime > 0);
    }
}

// ============================================================================
// Hidden files/directories tests
// ============================================================================

#[test]
fn test_hidden_directories_included() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create a hidden directory with files (like .fingerprint)
    fs::create_dir(root.join(".hidden_dir")).unwrap();
    File::create(root.join(".hidden_dir/file.txt"))
        .unwrap()
        .write_all(b"hidden")
        .unwrap();

    // Create a normal file too
    File::create(root.join("normal.txt"))
        .unwrap()
        .write_all(b"normal")
        .unwrap();

    // With empty filter, should find both files
    let options = ScanOptions {
        filter: ScanFilter::empty(),
        ..Default::default()
    };
    let entries = scan_collect(dir.path(), &options).unwrap();

    // Should find file in hidden directory
    let has_hidden = entries
        .iter()
        .any(|e| e.path.to_string_lossy().contains(".hidden_dir"));
    assert!(has_hidden, "Should include files in hidden directories");
    assert_eq!(entries.len(), 2);
}

#[test]
fn test_hidden_files_included_by_default() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    File::create(root.join(".hidden_file"))
        .unwrap()
        .write_all(b"hidden")
        .unwrap();
    File::create(root.join("normal.txt"))
        .unwrap()
        .write_all(b"normal")
        .unwrap();

    let options = ScanOptions {
        filter: ScanFilter::empty(),
        ..Default::default()
    };
    let entries = scan_collect(dir.path(), &options).unwrap();

    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .any(|e| e.path.to_string_lossy() == ".hidden_file")
    );
}

// ============================================================================
// Filter tests
// ============================================================================

#[test]
fn test_custom_filter() {
    let dir = create_test_tree();

    // Create a filter that excludes .txt files
    let options = ScanOptions {
        filter: ScanFilter::empty().with_patterns(["*.txt"]),
        ..Default::default()
    };

    let entries = scan_collect(dir.path(), &options).unwrap();

    // Should have 1 file (.DS_Store) - all .txt files filtered out
    assert_eq!(
        entries.len(),
        1,
        "Filter should exclude all .txt files, leaving .DS_Store"
    );
    assert!(entries[0].path.to_string_lossy().contains(".DS_Store"));
}

#[test]
fn test_filter_partial_match() {
    let dir = create_test_tree();

    // Filter that only excludes file1.txt
    let options = ScanOptions {
        filter: ScanFilter::empty().with_patterns(["file1.txt"]),
        ..Default::default()
    };

    let entries = scan_collect(dir.path(), &options).unwrap();

    // Should have 3 files (file2.txt, subdir/file3.txt, .DS_Store)
    assert_eq!(entries.len(), 3);
    assert!(!entries.iter().any(|e| e.path.ends_with("file1.txt")));
}

#[test]
fn test_default_filter_excludes_ds_store() {
    let dir = create_test_tree();

    let entries = scan_collect(dir.path(), &ScanOptions::default()).unwrap();

    // .DS_Store should be excluded
    assert!(
        !entries
            .iter()
            .any(|e| e.path.to_string_lossy() == ".DS_Store")
    );
}

// ============================================================================
// ScanOptions tests
// ============================================================================

#[test]
fn test_scan_options_default() {
    let options = ScanOptions::default();
    assert!(!options.follow_symlinks);
    assert!(!options.skip_hidden);
    assert_eq!(options.max_depth, usize::MAX);
}

#[test]
fn test_scan_options_max_depth() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create nested structure
    fs::create_dir_all(root.join("a/b/c")).unwrap();
    File::create(root.join("level0.txt"))
        .unwrap()
        .write_all(b"0")
        .unwrap();
    File::create(root.join("a/level1.txt"))
        .unwrap()
        .write_all(b"1")
        .unwrap();
    File::create(root.join("a/b/level2.txt"))
        .unwrap()
        .write_all(b"2")
        .unwrap();
    File::create(root.join("a/b/c/level3.txt"))
        .unwrap()
        .write_all(b"3")
        .unwrap();

    // Scan with max_depth = 2 (root = 0, a = 1, a/b = 2)
    let options = ScanOptions {
        max_depth: 2,
        filter: ScanFilter::empty(),
        ..Default::default()
    };
    let entries = scan_collect(dir.path(), &options).unwrap();

    // Should find level0.txt and level1.txt, but not level2.txt or level3.txt
    assert!(
        entries
            .iter()
            .any(|e| e.path.to_string_lossy() == "level0.txt")
    );
    assert!(
        entries
            .iter()
            .any(|e| e.path.to_string_lossy().contains("level1.txt"))
    );
    assert!(
        !entries
            .iter()
            .any(|e| e.path.to_string_lossy().contains("level2.txt"))
    );
    assert!(
        !entries
            .iter()
            .any(|e| e.path.to_string_lossy().contains("level3.txt"))
    );
}

// ============================================================================
// Nested directory tests
// ============================================================================

#[test]
fn test_deeply_nested_structure() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create deeply nested structure
    let deep_path = root.join("a/b/c/d/e/f");
    fs::create_dir_all(&deep_path).unwrap();
    File::create(deep_path.join("deep.txt"))
        .unwrap()
        .write_all(b"deep")
        .unwrap();

    let options = ScanOptions {
        filter: ScanFilter::empty(),
        ..Default::default()
    };
    let entries = scan_collect(dir.path(), &options).unwrap();

    assert_eq!(entries.len(), 1);
    assert!(entries[0].path.to_string_lossy().contains("deep.txt"));
}

#[test]
fn test_multiple_subdirectories() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create multiple subdirectories with files
    for subdir in ["dir1", "dir2", "dir3"] {
        fs::create_dir(root.join(subdir)).unwrap();
        File::create(root.join(format!("{subdir}/file.txt")))
            .unwrap()
            .write_all(b"content")
            .unwrap();
    }

    let options = ScanOptions {
        filter: ScanFilter::empty(),
        ..Default::default()
    };
    let entries = scan_collect(dir.path(), &options).unwrap();

    assert_eq!(entries.len(), 3);
}

// ============================================================================
// Error type tests
// ============================================================================

#[test]
fn test_scan_error_display() {
    let err = ScanError::RootNotFound(PathBuf::from("/test/path"));
    let display = format!("{err}");
    assert!(display.contains("/test/path"));

    let err2 = ScanError::NotADirectory(PathBuf::from("/test/file.txt"));
    let display2 = format!("{err2}");
    assert!(display2.contains("/test/file.txt"));
}

// ============================================================================
// Large file tests
// ============================================================================

#[test]
fn test_various_file_sizes() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Empty file
    File::create(root.join("empty.txt")).unwrap();

    // Small file
    File::create(root.join("small.txt"))
        .unwrap()
        .write_all(b"small")
        .unwrap();

    // Larger file (1KB)
    let large_content = vec![b'x'; 1024];
    File::create(root.join("large.txt"))
        .unwrap()
        .write_all(&large_content)
        .unwrap();

    let options = ScanOptions {
        filter: ScanFilter::empty(),
        ..Default::default()
    };
    let entries = scan_collect(dir.path(), &options).unwrap();

    assert_eq!(entries.len(), 3);

    let empty = entries
        .iter()
        .find(|e| e.path.to_string_lossy() == "empty.txt")
        .unwrap();
    assert_eq!(empty.size, 0);

    let small = entries
        .iter()
        .find(|e| e.path.to_string_lossy() == "small.txt")
        .unwrap();
    assert_eq!(small.size, 5);

    let large = entries
        .iter()
        .find(|e| e.path.to_string_lossy() == "large.txt")
        .unwrap();
    assert_eq!(large.size, 1024);
}

// ============================================================================
// CrawlProgress tests
// ============================================================================

#[test]
fn test_crawl_progress_new() {
    let progress = CrawlProgress::new();
    assert_eq!(progress.files(), 0);
    assert_eq!(progress.dirs(), 0);
    assert_eq!(progress.bytes(), 0);
    assert!(!progress.is_cancelled());
}

#[test]
fn test_crawl_progress_default() {
    let progress = CrawlProgress::default();
    assert_eq!(progress.files(), 0);
    assert_eq!(progress.dirs(), 0);
}

#[test]
fn test_crawl_progress_cancel() {
    let progress = CrawlProgress::new();
    assert!(!progress.is_cancelled());
    progress.cancel();
    assert!(progress.is_cancelled());
}

#[test]
fn test_scan_with_progress_counts() {
    let dir = create_test_tree();
    let progress = Arc::new(CrawlProgress::new());

    let entries = scan_with_progress(
        dir.path(),
        &ScanOptions::default(),
        Some(Arc::clone(&progress)),
    )
    .unwrap()
    .collect::<Vec<_>>();

    // Should have found 3 files (excluding .DS_Store which is filtered)
    let ok_count = entries.iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok_count, 3);

    // Progress should reflect what we found
    assert_eq!(progress.files(), 3);
    // At least 1 directory (subdir), possibly more depending on how root is counted
    assert!(progress.dirs() >= 1);
    // Bytes should be non-zero (we wrote some content)
    assert!(progress.bytes() > 0);
}

#[test]
fn test_scan_with_progress_bytes() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create files with known sizes
    File::create(root.join("a.txt"))
        .unwrap()
        .write_all(b"12345") // 5 bytes
        .unwrap();
    File::create(root.join("b.txt"))
        .unwrap()
        .write_all(b"1234567890") // 10 bytes
        .unwrap();

    let progress = Arc::new(CrawlProgress::new());
    let entries = scan_collect_with_progress(
        root,
        &ScanOptions {
            filter: ScanFilter::empty(),
            ..Default::default()
        },
        Some(Arc::clone(&progress)),
    )
    .unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(progress.files(), 2);
    assert_eq!(progress.bytes(), 15); // 5 + 10
}

#[test]
fn test_scan_with_progress_cancellation() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create many files
    for i in 0..100 {
        File::create(root.join(format!("file{i}.txt")))
            .unwrap()
            .write_all(b"x")
            .unwrap();
    }

    let progress = Arc::new(CrawlProgress::new());

    // Cancel immediately
    progress.cancel();

    let entries = scan_collect_with_progress(
        root,
        &ScanOptions {
            filter: ScanFilter::empty(),
            ..Default::default()
        },
        Some(Arc::clone(&progress)),
    )
    .unwrap();

    // Should have very few or no entries due to cancellation
    assert!(
        entries.len() < 100,
        "Cancellation should stop the scan early"
    );
}

#[test]
fn test_scan_with_progress_none() {
    // Ensure scan works fine without progress tracking
    let dir = create_test_tree();

    let entries = scan_with_progress(dir.path(), &ScanOptions::default(), None)
        .unwrap()
        .collect::<Vec<_>>();

    let ok_count = entries.iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok_count, 3);
}

#[test]
fn test_scan_collect_with_progress() {
    let dir = create_test_tree();
    let progress = Arc::new(CrawlProgress::new());

    let entries = scan_collect_with_progress(
        dir.path(),
        &ScanOptions::default(),
        Some(Arc::clone(&progress)),
    )
    .unwrap();

    assert_eq!(entries.len(), 3);
    assert_eq!(progress.files(), 3);
}
