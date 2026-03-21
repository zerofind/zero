//! Core data structures for representing code elements (Rust, Go, etc.)

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Programming language of a code element
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    Go,
}

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Go => "go",
        }
    }

    pub fn file_extension(&self) -> &'static str {
        match self {
            Self::Rust => "rs",
            Self::Go => "go",
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A code element extracted from source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeElement {
    pub language: Language,
    pub kind: ElementKind,
    pub name: String,
    /// Full signature (e.g., `pub fn scan(path: &Path) -> Result<Vec<Entry>>`)
    pub signature: String,
    /// File path relative to project root
    pub file_path: String,
    pub line_number: usize,
    pub doc: Option<String>,
    pub visibility: Visibility,
}

/// Kind of code element
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Interface,
    Impl,
    Const,
    Static,
    TypeAlias,
    Macro,
    Module,
    Package,
}

impl ElementKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "fn",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Interface => "interface",
            Self::Impl => "impl",
            Self::Const => "const",
            Self::Static => "static",
            Self::TypeAlias => "type",
            Self::Macro => "macro",
            Self::Module => "mod",
            Self::Package => "package",
        }
    }

    /// Parse from a string (for CLI/MCP filter parameter)
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "fn" | "function" => Some(Self::Function),
            "method" => Some(Self::Method),
            "struct" => Some(Self::Struct),
            "enum" => Some(Self::Enum),
            "trait" => Some(Self::Trait),
            "interface" => Some(Self::Interface),
            "impl" => Some(Self::Impl),
            "const" => Some(Self::Const),
            "static" => Some(Self::Static),
            "type" | "type_alias" | "typealias" => Some(Self::TypeAlias),
            "macro" => Some(Self::Macro),
            "mod" | "module" => Some(Self::Module),
            "package" => Some(Self::Package),
            _ => None,
        }
    }
}

impl std::fmt::Display for ElementKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::cmp::Ord for ElementKind {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

impl std::cmp::PartialOrd for ElementKind {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Visibility of a code element
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    PublicCrate,
    PublicSuper,
    Private,
}

impl Visibility {
    pub fn is_public(&self) -> bool {
        matches!(self, Self::Public)
    }
}

/// Project structure with all extracted elements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub root: String,
    pub elements: Vec<CodeElement>,
    pub files_processed: usize,
    pub lines_of_code: usize,
    /// Lines of code per file (relative path -> line count)
    pub file_lines: HashMap<String, usize>,
    pub parse_errors: Vec<(String, String)>,
}

impl ProjectSummary {
    pub fn new(root: String) -> Self {
        Self {
            root,
            elements: Vec::new(),
            files_processed: 0,
            lines_of_code: 0,
            file_lines: HashMap::new(),
            parse_errors: Vec::new(),
        }
    }

    /// Filter to only public elements
    #[must_use]
    pub fn public_only(&self) -> Vec<&CodeElement> {
        self.elements
            .iter()
            .filter(|e| e.visibility.is_public())
            .collect()
    }

    /// Group elements by file
    #[must_use]
    pub fn by_file(&self) -> std::collections::BTreeMap<&str, Vec<&CodeElement>> {
        let mut map = std::collections::BTreeMap::new();
        for element in &self.elements {
            map.entry(element.file_path.as_str())
                .or_insert_with(Vec::new)
                .push(element);
        }
        map
    }

    /// Group elements by kind
    #[must_use]
    pub fn by_kind(&self) -> std::collections::BTreeMap<ElementKind, Vec<&CodeElement>> {
        let mut map = std::collections::BTreeMap::new();
        for element in &self.elements {
            map.entry(element.kind)
                .or_insert_with(Vec::new)
                .push(element);
        }
        map
    }

    /// Remove all elements from a specific file
    pub fn remove_file(&mut self, file_path: &str) {
        self.elements.retain(|e| e.file_path != file_path);
    }

    /// Merge elements from another summary (used for incremental updates)
    pub fn merge(&mut self, other: ProjectSummary) {
        let files: std::collections::HashSet<String> =
            other.elements.iter().map(|e| e.file_path.clone()).collect();

        let error_files: std::collections::HashSet<String> = other
            .parse_errors
            .iter()
            .map(|(path, _)| path.clone())
            .collect();

        for file in &files {
            self.remove_file(file);
        }

        self.elements.extend(other.elements);

        self.elements.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then(a.line_number.cmp(&b.line_number))
        });

        self.parse_errors
            .retain(|(path, _)| !files.contains(path) && !error_files.contains(path));
        self.parse_errors.extend(other.parse_errors);
    }

    /// Detected languages in this project
    #[must_use]
    pub fn languages(&self) -> Vec<Language> {
        let mut langs: Vec<Language> = self
            .elements
            .iter()
            .map(|e| e.language)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        langs.sort_by_key(|l| *l as u8);
        langs
    }
}

#[cfg(test)]
#[path = "element_test.rs"]
mod element_test;
