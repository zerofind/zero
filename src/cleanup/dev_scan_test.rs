//! Tests for dev garbage scanner

use std::fs;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use tempfile::tempdir;

use super::dev_scan::*;

/// Helper: create a file with some content to give it size
fn create_file(path: &std::path::Path, size: usize) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, vec![0u8; size]).unwrap();
}

#[test]
fn test_match_with_sibling() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Create project with Cargo.toml + target/
    create_file(&root.join("myproject/Cargo.toml"), 100);
    create_file(&root.join("myproject/target/debug/binary"), 1000);
    create_file(&root.join("myproject/target/release/binary"), 2000);

    let result = scan_dev_garbage(root, Some("rust_target"), None, None).unwrap();

    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].category_id, "rust_target");
    assert_eq!(result.items[0].dir_name, "target");
    assert!(result.items[0].total_bytes >= 3000);
    assert_eq!(result.items[0].file_count, 2);
}

#[test]
fn test_no_match_without_sibling() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Create target/ without Cargo.toml
    create_file(&root.join("other/target/debug/binary"), 1000);

    let result = scan_dev_garbage(root, Some("rust_target"), None, None).unwrap();

    assert!(result.items.is_empty());
}

#[test]
fn test_disambiguation_flutter_vs_cmake() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Flutter project: build/ + pubspec.yaml
    create_file(&root.join("flutter_app/pubspec.yaml"), 50);
    create_file(&root.join("flutter_app/build/output.apk"), 5000);

    // CMake project: build/ + CMakeLists.txt
    create_file(&root.join("cmake_proj/CMakeLists.txt"), 50);
    create_file(&root.join("cmake_proj/build/Makefile"), 200);

    let result = scan_dev_garbage(root, None, None, None).unwrap();

    // Find flutter match
    let flutter: Vec<_> = result
        .items
        .iter()
        .filter(|i| i.category_id == "flutter_build")
        .collect();
    assert_eq!(flutter.len(), 1);
    assert!(flutter[0].path.to_string_lossy().contains("flutter_app"));

    // Find cmake match
    let cmake: Vec<_> = result
        .items
        .iter()
        .filter(|i| i.category_id == "cmake_build")
        .collect();
    assert_eq!(cmake.len(), 1);
    assert!(cmake[0].path.to_string_lossy().contains("cmake_proj"));
}

#[test]
fn test_node_modules_match() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    create_file(&root.join("webapp/package.json"), 200);
    create_file(&root.join("webapp/node_modules/.package-lock.json"), 100);
    create_file(&root.join("webapp/node_modules/react/index.js"), 500);

    let result = scan_dev_garbage(root, Some("node_modules"), None, None).unwrap();

    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].category_id, "node_modules");
    assert!(result.items[0].total_bytes >= 600);
}

#[test]
fn test_size_calculation() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    create_file(&root.join("proj/Cargo.toml"), 50);
    create_file(&root.join("proj/target/a"), 1000);
    create_file(&root.join("proj/target/b"), 2000);
    create_file(&root.join("proj/target/sub/c"), 3000);

    let result = scan_dev_garbage(root, Some("rust_target"), None, None).unwrap();

    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].total_bytes, 6000);
    assert_eq!(result.items[0].file_count, 3);
}

#[test]
fn test_skip_descent_into_matched() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Nested: project/target/ and project/target/nested_project/Cargo.toml/target/
    // The inner one should NOT be found because we skip descent into matched dirs
    create_file(&root.join("proj/Cargo.toml"), 50);
    create_file(&root.join("proj/target/debug/bin"), 1000);
    create_file(&root.join("proj/target/nested/Cargo.toml"), 50);
    create_file(&root.join("proj/target/nested/target/debug/bin"), 500);

    let result = scan_dev_garbage(root, Some("rust_target"), None, None).unwrap();

    // Should only find the outer target/, not the nested one
    assert_eq!(result.items.len(), 1);
    assert_eq!(
        result.items[0].path,
        root.join("proj/target").canonicalize().unwrap()
    );
}

#[test]
fn test_cancellation() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Create several projects
    for i in 0..5 {
        create_file(&root.join(format!("proj{i}/Cargo.toml")), 50);
        create_file(&root.join(format!("proj{i}/target/bin")), 100);
    }

    let progress = Arc::new(DevScanProgress::new());
    // Cancel immediately
    progress.cancelled.store(true, Ordering::Relaxed);

    let result = scan_dev_garbage(root, Some("rust_target"), None, Some(progress)).unwrap();

    // Should find fewer items than available (cancelled early)
    assert!(result.items.len() < 5);
}

#[test]
fn test_empty_dir_no_matches() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Empty directory
    let result = scan_dev_garbage(root, None, None, None).unwrap();

    assert!(result.items.is_empty());
    assert_eq!(result.total_bytes, 0);
}

#[test]
fn test_root_not_found() {
    let result = scan_dev_garbage(
        std::path::Path::new("/nonexistent/path/that/does/not/exist"),
        None,
        None,
        None,
    );
    assert!(result.is_err());
}

#[test]
fn test_category_filter() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // Create both node_modules and target
    create_file(&root.join("rs_proj/Cargo.toml"), 50);
    create_file(&root.join("rs_proj/target/bin"), 100);
    create_file(&root.join("js_proj/package.json"), 50);
    create_file(&root.join("js_proj/node_modules/lib.js"), 100);

    // Filter to just rust_target
    let result = scan_dev_garbage(root, Some("rust_target"), None, None).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].category_id, "rust_target");

    // Filter to just node_modules
    let result = scan_dev_garbage(root, Some("node_modules"), None, None).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].category_id, "node_modules");
}

#[test]
fn test_results_sorted_by_size() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    create_file(&root.join("small/Cargo.toml"), 50);
    create_file(&root.join("small/target/bin"), 100);

    create_file(&root.join("large/Cargo.toml"), 50);
    create_file(&root.join("large/target/bin"), 10000);

    create_file(&root.join("medium/Cargo.toml"), 50);
    create_file(&root.join("medium/target/bin"), 5000);

    let result = scan_dev_garbage(root, Some("rust_target"), None, None).unwrap();

    assert_eq!(result.items.len(), 3);
    assert!(result.items[0].total_bytes >= result.items[1].total_bytes);
    assert!(result.items[1].total_bytes >= result.items[2].total_bytes);
}

#[test]
fn test_multiple_categories_same_scan() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    create_file(&root.join("rs/Cargo.toml"), 50);
    create_file(&root.join("rs/target/bin"), 1000);
    create_file(&root.join("js/package.json"), 50);
    create_file(&root.join("js/node_modules/a.js"), 500);
    create_file(&root.join("py/pyproject.toml"), 50);
    create_file(&root.join("py/.venv/lib/pkg.py"), 300);

    let result = scan_dev_garbage(root, None, None, None).unwrap();

    let ids: Vec<&str> = result
        .items
        .iter()
        .map(|i| i.category_id.as_str())
        .collect();
    assert!(ids.contains(&"rust_target"));
    assert!(ids.contains(&"node_modules"));
    assert!(ids.contains(&"python_venv"));
}
