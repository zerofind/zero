//! Language-specific parsers
//!
//! Each language has its own submodule. The `LanguageParser` trait provides
//! a uniform interface; `parser_for_extension()` selects the right parser.

pub mod go;
pub mod rust;
pub mod rust_format;

use std::path::Path;

use super::element::{CodeElement, Language};

/// Trait for language-specific code parsers
pub trait LanguageParser: Send + Sync {
    fn language(&self) -> Language;
    fn extensions(&self) -> &[&str];
    fn parse_file(&self, path: &Path, relative_path: &str) -> Result<Vec<CodeElement>, String>;
}

struct RustParser;
struct GoParser;

impl LanguageParser for RustParser {
    fn language(&self) -> Language {
        Language::Rust
    }
    fn extensions(&self) -> &[&str] {
        &["rs"]
    }
    fn parse_file(&self, path: &Path, relative_path: &str) -> Result<Vec<CodeElement>, String> {
        rust::parse_file(path, relative_path).map_err(|e| e.to_string())
    }
}

impl LanguageParser for GoParser {
    fn language(&self) -> Language {
        Language::Go
    }
    fn extensions(&self) -> &[&str] {
        &["go"]
    }
    fn parse_file(&self, path: &Path, relative_path: &str) -> Result<Vec<CodeElement>, String> {
        go::parse_file(path, relative_path).map_err(|e| e.to_string())
    }
}

static RUST_PARSER: RustParser = RustParser;
static GO_PARSER: GoParser = GoParser;

/// Get the appropriate parser for a file extension
pub fn parser_for_extension(ext: &str) -> Option<&'static dyn LanguageParser> {
    match ext {
        "rs" => Some(&RUST_PARSER),
        "go" => Some(&GO_PARSER),
        _ => None,
    }
}

/// Detect language from file extension
pub fn detect_language(path: &Path) -> Option<Language> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "rs" => Some(Language::Rust),
        "go" => Some(Language::Go),
        _ => None,
    }
}
