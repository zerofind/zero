//! Search across all .todo files
//!
//! Provides fast search across all indexed `.todo` files by:
//! 1. Using `TypeIndex` bitmap to find all `.todo` files instantly
//! 2. Loading files in parallel with rayon
//! 3. Filtering tasks by query, tags, status, etc.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::storage::open_store;
use super::task::{Task, TaskStatus};

/// Errors that can occur during todo search
#[derive(Error, Debug)]
pub enum SearchError {
    /// No index available
    #[error("No search index available. Run 'zero search --index' first.")]
    NoIndex,

    /// IO error loading todo file
    #[error("Failed to load todo file: {0}")]
    LoadError(String),
}

/// A task found during search, with its source file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSearchResult {
    /// The task
    pub task: TaskRef,
    /// Path to the .todo file
    pub file_path: String,
    /// Filename only (e.g., "project.todo")
    pub file_name: String,
    /// Relevance score (higher = better match)
    pub score: u32,
}

/// Task reference for search results (avoids cloning full Task)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRef {
    pub id: u64,
    pub list: String,
    pub text: String,
    pub status: String,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due: Option<i64>,
    pub created: i64,
    pub modified: i64,
}

impl From<&Task> for TaskRef {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id,
            list: task.list.clone(),
            text: task.text.clone(),
            status: task.status.as_str().to_string(),
            tags: task.tags.clone(),
            assigned: task.assigned.clone(),
            due: task.due,
            created: task.created,
            modified: task.modified,
        }
    }
}

/// Options for searching todos
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoSearchOptions {
    /// Text query (searches task text, case-insensitive)
    #[serde(default)]
    pub query: Option<String>,

    /// Filter by tag (task must have this tag)
    #[serde(default)]
    pub tag: Option<String>,

    /// Filter by status ("open", "done", "`in_progress`", "cancelled")
    #[serde(default)]
    pub status: Option<String>,

    /// Filter by list name
    #[serde(default)]
    pub list: Option<String>,

    /// Filter by assignee
    #[serde(default)]
    pub assigned: Option<String>,

    /// Only include overdue tasks
    #[serde(default)]
    pub overdue_only: bool,

    /// Maximum number of results
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

impl Default for TodoSearchOptions {
    fn default() -> Self {
        Self {
            query: None,
            tag: None,
            status: None,
            list: None,
            assigned: None,
            overdue_only: false,
            limit: 100,
        }
    }
}

impl TodoSearchOptions {
    /// Create new search options with a query
    pub fn with_query(query: impl Into<String>) -> Self {
        Self {
            query: Some(query.into()),
            ..Default::default()
        }
    }

    /// Builder: filter by tag
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Builder: filter by status
    #[must_use]
    pub fn with_status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }

    /// Builder: filter by list
    #[must_use]
    pub fn with_list(mut self, list: impl Into<String>) -> Self {
        self.list = Some(list.into());
        self
    }

    /// Builder: filter by assignee
    #[must_use]
    pub fn with_assigned(mut self, assigned: impl Into<String>) -> Self {
        self.assigned = Some(assigned.into());
        self
    }

    /// Builder: only overdue tasks
    #[must_use]
    pub fn overdue(mut self) -> Self {
        self.overdue_only = true;
        self
    }

    /// Builder: set result limit
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

/// Search for tasks across multiple .todo files
///
/// # Arguments
/// * `todo_paths` - Paths to .todo files to search
/// * `options` - Search options (query, filters, limit)
///
/// # Returns
/// Vector of matching tasks with their source files
pub fn search_todos(todo_paths: &[PathBuf], options: &TodoSearchOptions) -> Vec<TaskSearchResult> {
    if todo_paths.is_empty() {
        return Vec::new();
    }

    // Load and search files in parallel
    let mut results: Vec<TaskSearchResult> = todo_paths
        .par_iter()
        .filter_map(|path| load_and_search_file(path, options).ok())
        .flatten()
        .collect();

    // Sort by score (descending), then by modified (descending)
    results.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.task.modified.cmp(&a.task.modified))
    });

    // Apply limit
    results.truncate(options.limit);

    results
}

/// Load a single .todo directory and search its tasks
fn load_and_search_file(
    path: &Path,
    options: &TodoSearchOptions,
) -> Result<Vec<TaskSearchResult>, SearchError> {
    let store = open_store(path).map_err(|e| SearchError::LoadError(e.to_string()))?;
    let state = store.read();

    let file_path = path.to_string_lossy().to_string();
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let results: Vec<TaskSearchResult> = state
        .file
        .tasks
        .iter()
        .filter_map(|task| {
            let score = match_task(task, options)?;
            Some(TaskSearchResult {
                task: TaskRef::from(task),
                file_path: file_path.clone(),
                file_name: file_name.clone(),
                score,
            })
        })
        .collect();

    Ok(results)
}

/// Check if a task matches the search options
///
/// Returns Some(score) if matches, None if doesn't match
fn match_task(task: &Task, options: &TodoSearchOptions) -> Option<u32> {
    let mut score = 100u32;

    // Text query match
    if let Some(ref query) = options.query {
        let query_lower = query.to_lowercase();
        let text_lower = task.text.to_lowercase();

        if text_lower.contains(&query_lower) {
            // Boost score for exact word match or prefix match
            if text_lower.starts_with(&query_lower) {
                score += 50;
            } else if text_lower
                .split_whitespace()
                .any(|w| w.starts_with(&query_lower))
            {
                score += 25;
            }
        } else {
            return None;
        }
    }

    // Tag filter
    if let Some(ref tag) = options.tag {
        if !task.has_tag(tag) {
            return None;
        }
        score += 10;
    }

    // Status filter
    if let Some(ref status) = options.status {
        let task_status = task.status.as_str();
        if !task_status.eq_ignore_ascii_case(status) {
            // Also check common aliases
            let matches = match status.to_lowercase().as_str() {
                "open" => task.status == TaskStatus::Open,
                "done" | "completed" | "complete" => task.status == TaskStatus::Done,
                "in_progress" | "inprogress" | "wip" => task.status == TaskStatus::InProgress,
                "cancelled" | "canceled" => task.status == TaskStatus::Cancelled,
                _ => false,
            };
            if !matches {
                return None;
            }
        }
    }

    // List filter
    if let Some(ref list) = options.list {
        if !task.list.eq_ignore_ascii_case(list) {
            return None;
        }
        score += 5;
    }

    // Assignee filter
    if let Some(ref assigned) = options.assigned {
        match &task.assigned {
            Some(a) if a.eq_ignore_ascii_case(assigned) => {
                score += 10;
            }
            _ => return None,
        }
    }

    // Overdue filter
    if options.overdue_only && !task.is_overdue() {
        return None;
    }

    Some(score)
}

/// Get all .todo file paths from a list of directories
///
/// This is a simple recursive scan. For indexed directories,
/// use the `TypeIndex` bitmap instead (much faster).
pub fn find_todo_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    dirs.par_iter()
        .flat_map(|dir| find_todo_files_in_dir(dir))
        .collect()
}

/// Find .todo directories (etch stores) in a single directory (recursive)
fn find_todo_files_in_dir(dir: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                // Skip hidden directories and common excludes
                if name.starts_with('.') || name == "node_modules" || name == "target" {
                    continue;
                }

                if path.is_dir() {
                    if let Some(ext) = path.extension()
                        && ext == "todo"
                    {
                        // .todo directory = etch store
                        results.push(path);
                    } else {
                        // Recurse into regular directories
                        results.extend(find_todo_files_in_dir(&path));
                    }
                }
            }
        }
    }

    results
}
