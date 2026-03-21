//! Integration tests for CodeIndex (full roundtrip: create files → index → search → overview)

use super::*;
use std::fs;

fn create_rust_project(dir: &std::path::Path) {
    // Create a .git dir so discover_projects finds it
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("src/lib.rs"),
        r#"
/// The main search engine
pub struct SearchEngine {
    pub index_path: String,
    pub max_results: usize,
}

impl SearchEngine {
    /// Create a new search engine
    pub fn new(path: &str) -> Self {
        Self { index_path: path.to_string(), max_results: 100 }
    }

    /// Run a search query
    pub fn search(&self, query: &str) -> Vec<String> {
        vec![]
    }
}

pub const VERSION: &str = "1.0.0";

pub enum IndexError {
    NotFound,
    Corrupted(String),
}

fn internal_helper() {}
"#,
    )
    .unwrap();

    fs::write(
        dir.join("src/utils.rs"),
        r#"
/// Utility for formatting results
pub fn format_result(item: &str) -> String {
    item.to_uppercase()
}

pub trait Formatter {
    fn format(&self) -> String;
}
"#,
    )
    .unwrap();
}

fn create_go_project(dir: &std::path::Path) {
    fs::create_dir_all(dir.join(".git")).unwrap();

    fs::write(
        dir.join("main.go"),
        r#"package main

// Server handles HTTP requests
type Server struct {
    Port int
    Host string
}

func NewServer(port int) *Server {
    return &Server{Port: port}
}

func (s *Server) Start() error {
    return nil
}

type Handler interface {
    Handle(req Request) Response
}
"#,
    )
    .unwrap();
}

#[test]
fn test_index_and_search_rust_project() {
    let tmp = tempfile::tempdir().unwrap();
    let index_dir = tmp.path().join("code_index");
    let project_dir = tmp.path().join("myproject");

    create_rust_project(&project_dir);

    let mut ci = CodeIndex::with_dir(index_dir).unwrap();
    let project = ci.index_project(&project_dir).unwrap();

    assert!(project.symbol_count > 0);
    assert!(project.file_count >= 2);
    assert!(project.languages.contains(&Language::Rust));

    // Search by name
    let results = ci
        .search("SearchEngine", &CodeSearchOpts::default())
        .unwrap();
    assert!(!results.is_empty(), "should find SearchEngine");
    assert!(results.iter().any(|r| r.element.name == "SearchEngine"));

    // Search by kind filter
    let results = ci
        .search(
            "search",
            &CodeSearchOpts {
                kind: Some(ElementKind::Function),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        results.iter().any(|r| r.element.name == "search"),
        "should find search method"
    );

    // Search with no results
    let results = ci
        .search("nonexistent_xyz", &CodeSearchOpts::default())
        .unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_index_persist_and_reload() {
    let tmp = tempfile::tempdir().unwrap();
    let index_dir = tmp.path().join("code_index");
    let project_dir = tmp.path().join("myproject");

    create_rust_project(&project_dir);

    // Index with one CodeIndex instance
    {
        let mut ci = CodeIndex::with_dir(index_dir.clone()).unwrap();
        ci.index_project(&project_dir).unwrap();
    }

    // Create a new CodeIndex instance — should reload from disk
    let mut ci2 = CodeIndex::with_dir(index_dir).unwrap();
    let projects = ci2.indexed_projects();
    assert_eq!(projects.len(), 1);

    // Search should work after reload (loads .cidx from disk)
    let results = ci2
        .search("SearchEngine", &CodeSearchOpts::default())
        .unwrap();
    assert!(!results.is_empty());
}

#[test]
fn test_overview() {
    let tmp = tempfile::tempdir().unwrap();
    let index_dir = tmp.path().join("code_index");
    let project_dir = tmp.path().join("myproject");

    create_rust_project(&project_dir);

    let mut ci = CodeIndex::with_dir(index_dir).unwrap();
    ci.index_project(&project_dir).unwrap();

    let overview = ci.overview(&project_dir).unwrap();
    assert!(overview.is_some());

    let ov = overview.unwrap();
    assert!(ov.file_count >= 2);
    assert!(ov.symbol_count > 0);
    assert!(!ov.key_types.is_empty(), "should have public types");
    assert!(
        ov.key_types.iter().any(|t| t == "SearchEngine"),
        "key_types should include SearchEngine"
    );
}

#[test]
fn test_remove_project() {
    let tmp = tempfile::tempdir().unwrap();
    let index_dir = tmp.path().join("code_index");
    let project_dir = tmp.path().join("myproject");

    create_rust_project(&project_dir);

    let mut ci = CodeIndex::with_dir(index_dir).unwrap();
    ci.index_project(&project_dir).unwrap();
    assert_eq!(ci.indexed_projects().len(), 1);

    ci.remove_project(&project_dir).unwrap();
    assert_eq!(ci.indexed_projects().len(), 0);

    // Search should return empty after removal
    let results = ci
        .search("SearchEngine", &CodeSearchOpts::default())
        .unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_index_go_project() {
    let tmp = tempfile::tempdir().unwrap();
    let index_dir = tmp.path().join("code_index");
    let project_dir = tmp.path().join("goproject");

    create_go_project(&project_dir);

    let mut ci = CodeIndex::with_dir(index_dir).unwrap();
    let project = ci.index_project(&project_dir).unwrap();

    assert!(project.symbol_count > 0);
    assert!(project.languages.contains(&Language::Go));

    let results = ci.search("Server", &CodeSearchOpts::default()).unwrap();
    assert!(
        !results.is_empty(),
        "should find Server struct in Go project"
    );
}

#[test]
fn test_project_symbols_public_only() {
    let tmp = tempfile::tempdir().unwrap();
    let index_dir = tmp.path().join("code_index");
    let project_dir = tmp.path().join("myproject");

    create_rust_project(&project_dir);

    let mut ci = CodeIndex::with_dir(index_dir).unwrap();
    ci.index_project(&project_dir).unwrap();

    let symbols = ci
        .project_symbols(&project_dir, &CodeSearchOpts::default())
        .unwrap();

    // internal_helper is private — should NOT appear
    assert!(
        !symbols.iter().any(|r| r.element.name == "internal_helper"),
        "private symbols should not appear in project_symbols"
    );

    // Public symbols should appear
    assert!(
        symbols.iter().any(|r| r.element.name == "SearchEngine"),
        "public struct should appear"
    );
}
