use tempfile::TempDir;

use super::super::element::{CodeElement, ElementKind, Language, ProjectSummary, Visibility};
use super::*;

#[test]
fn test_round_trip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.cidx");

    let mut summary = ProjectSummary::new("/test/project".to_string());
    summary.files_processed = 3;
    summary.lines_of_code = 150;
    summary.elements.push(CodeElement {
        language: Language::Rust,
        kind: ElementKind::Function,
        name: "hello".to_string(),
        signature: "pub fn hello() -> String".to_string(),
        file_path: "src/lib.rs".to_string(),
        line_number: 10,
        doc: Some("Says hello".to_string()),
        visibility: Visibility::Public,
    });
    summary.elements.push(CodeElement {
        language: Language::Go,
        kind: ElementKind::Struct,
        name: "Config".to_string(),
        signature: "type Config struct { Name string }".to_string(),
        file_path: "config.go".to_string(),
        line_number: 5,
        doc: None,
        visibility: Visibility::Public,
    });

    save_summary(&summary, &path).unwrap();
    assert!(path.exists());

    let loaded = load_summary(&path).unwrap();
    assert_eq!(loaded.root, summary.root);
    assert_eq!(loaded.files_processed, 3);
    assert_eq!(loaded.lines_of_code, 150);
    assert_eq!(loaded.elements.len(), 2);
    assert_eq!(loaded.elements[0].name, "hello");
    assert_eq!(loaded.elements[1].name, "Config");
}

#[test]
fn test_invalid_magic() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bad.cidx");
    std::fs::write(&path, b"BAD!data").unwrap();

    let result = load_summary(&path);
    assert!(result.is_err());
}
