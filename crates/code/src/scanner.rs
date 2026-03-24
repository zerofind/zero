//! Project directory scanning
//!
//! Walks a project directory and finds all source files for parsing.
//! Supports Rust (.rs) and Go (.go) files.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use super::element::ProjectSummary;
use super::parsers::{detect_language, parser_for_extension};

/// Scan a project directory and extract all code elements
pub fn scan_project(root: &Path) -> Result<ProjectSummary, ScanError> {
    let root = root
        .canonicalize()
        .map_err(|e| ScanError::InvalidRoot(root.display().to_string(), e))?;

    let mut summary = ProjectSummary::new(root.display().to_string());

    for entry in WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_hidden(e) && !is_excluded_dir(e))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                summary
                    .parse_errors
                    .push((String::new(), format!("Walk error: {e}")));
                continue;
            }
        };

        let path = entry.path();

        if entry.file_type().is_dir() {
            continue;
        }

        let Some(language) = detect_language(path) else {
            continue;
        };

        if is_test_file(path, language) {
            continue;
        }

        let relative_path = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();

        let Some(parser) = parser_for_extension(ext) else {
            continue;
        };

        // Count lines of code
        let loc = std::fs::read_to_string(path).map_or(0, |s| s.lines().count());

        match parser.parse_file(path, &relative_path) {
            Ok(elements) => {
                summary.elements.extend(elements);
                summary.files_processed += 1;
                summary.lines_of_code += loc;
                summary.file_lines.insert(relative_path, loc);
            }
            Err(e) => {
                summary.parse_errors.push((relative_path, e));
            }
        }
    }

    Ok(summary)
}

/// Scan only specific files (useful for incremental updates)
pub fn scan_files(root: &Path, files: &[PathBuf]) -> Result<ProjectSummary, ScanError> {
    let root = root
        .canonicalize()
        .map_err(|e| ScanError::InvalidRoot(root.display().to_string(), e))?;

    let mut summary = ProjectSummary::new(root.display().to_string());

    for path in files {
        let full_path = if path.is_absolute() {
            path.clone()
        } else {
            root.join(path)
        };

        if !full_path.exists() {
            summary
                .parse_errors
                .push((path.display().to_string(), "File not found".to_string()));
            continue;
        }

        let ext = full_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();

        let Some(parser) = parser_for_extension(ext) else {
            continue;
        };

        let relative_path = full_path
            .strip_prefix(&root)
            .unwrap_or(&full_path)
            .display()
            .to_string();

        let loc = std::fs::read_to_string(&full_path).map_or(0, |s| s.lines().count());

        match parser.parse_file(&full_path, &relative_path) {
            Ok(elements) => {
                summary.elements.extend(elements);
                summary.files_processed += 1;
                summary.lines_of_code += loc;
                summary.file_lines.insert(relative_path, loc);
            }
            Err(e) => {
                summary.parse_errors.push((relative_path, e));
            }
        }
    }

    Ok(summary)
}

/// Walk a directory tree and find code projects (directories with .git)
pub fn discover_projects(root: &Path, git_only: bool) -> Vec<PathBuf> {
    let mut projects = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_hidden(e) && !is_excluded_dir(e))
    {
        let Ok(entry) = entry else {
            continue;
        };

        if !entry.file_type().is_dir() {
            continue;
        }

        let path = entry.path();

        if git_only {
            if path.join(".git").is_dir() {
                projects.push(path.to_path_buf());
            }
        } else {
            // Check if directory contains source files
            let has_source = has_source_files(path);
            if has_source {
                projects.push(path.to_path_buf());
            }
        }
    }

    // Remove nested projects (keep outermost only unless git_only)
    if !git_only {
        dedup_nested(&mut projects);
    }

    projects
}

// ============================================================================
// Helpers
// ============================================================================

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 {
        return false;
    }
    entry
        .file_name()
        .to_str()
        .is_some_and(|s| s.starts_with('.'))
}

fn is_excluded_dir(entry: &walkdir::DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    let name = entry.file_name().to_string_lossy();
    matches!(
        name.as_ref(),
        "target" | "vendor" | "node_modules" | "testdata" | ".git"
    )
}

use super::element::Language;

fn is_test_file(path: &Path, _language: Language) -> bool {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    stem.ends_with("_test")
}

/// Quick check if a directory contains any supported source files (non-recursive)
fn has_source_files(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        if let Some(ext) = entry.path().extension().and_then(|e| e.to_str())
            && matches!(ext, "rs" | "go")
        {
            return true;
        }
        // Check for Cargo.toml or go.mod as project indicators
        let name = entry.file_name();
        if name == "Cargo.toml" || name == "go.mod" {
            return true;
        }
    }
    false
}

/// Remove nested paths — if /a/b and /a/b/c are both in the list, remove /a/b/c
fn dedup_nested(paths: &mut Vec<PathBuf>) {
    paths.sort();
    paths.dedup();
    let snapshot = paths.clone();
    paths.retain(|p| {
        !snapshot
            .iter()
            .any(|parent| p != parent && p.starts_with(parent))
    });
}

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("Invalid project root '{0}': {1}")]
    InvalidRoot(String, std::io::Error),

    #[error("Failed to walk directory: {0}")]
    WalkError(String),
}

#[cfg(test)]
#[path = "scanner_test.rs"]
mod scanner_test;
