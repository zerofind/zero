use std::fs;
use tempfile::TempDir;

use super::*;

#[test]
fn test_scan_rust_project() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("lib.rs"),
        "pub fn hello() -> &'static str { \"hello\" }\n\npub struct Config { pub name: String }\n",
    )
    .unwrap();

    let summary = scan_project(dir.path()).unwrap();
    assert!(summary.files_processed >= 1);
    assert!(!summary.elements.is_empty());

    let names: Vec<&str> = summary.elements.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"hello"));
    assert!(names.contains(&"Config"));
}

#[test]
fn test_scan_files_incremental() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.rs"), "pub fn alpha() {}\n").unwrap();
    fs::write(src.join("b.rs"), "pub fn beta() {}\n").unwrap();

    let files = vec![src.join("a.rs")];
    let summary = scan_files(dir.path(), &files).unwrap();
    assert_eq!(summary.files_processed, 1);
    let names: Vec<&str> = summary.elements.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"alpha"));
    assert!(!names.contains(&"beta"));
}

#[test]
fn test_discover_projects_git_only() {
    let dir = TempDir::new().unwrap();

    // Create a git project
    let proj = dir.path().join("myproject");
    fs::create_dir_all(proj.join(".git")).unwrap();
    fs::write(proj.join("main.rs"), "fn main() {}").unwrap();

    // Create a non-git directory
    let other = dir.path().join("other");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("code.rs"), "fn code() {}").unwrap();

    let projects = discover_projects(dir.path(), true);
    assert_eq!(projects.len(), 1);
    assert!(projects[0].ends_with("myproject"));
}

#[test]
fn test_skips_test_files() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("lib.rs"), "pub fn real() {}\n").unwrap();
    fs::write(dir.path().join("lib_test.rs"), "fn test_real() {}\n").unwrap();

    let summary = scan_project(dir.path()).unwrap();
    let names: Vec<&str> = summary.elements.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"real"));
    assert!(!names.contains(&"test_real"));
}
