//! Storage layer for todo files
//!
//! Handles persistence via etchdb (WAL + snapshots).
//! Also manages the global context (which todo file is currently open).

use std::fs;
use std::path::{Path, PathBuf};

use etchdb::{Store, WalBackend};

use super::manager::TodoError;
use super::state::TodoState;

/// Default filename for todo directories
pub const DEFAULT_FILENAME: &str = "project.todo";

/// Open (or create) an etch-backed todo store.
///
/// `dir` is the etch directory (e.g., `project.todo/`).
/// Creates the directory if it doesn't exist.
pub fn open_store(dir: &Path) -> Result<Store<TodoState, WalBackend<TodoState>>, TodoError> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    super::state::open_store(dir)
}

/// Get the path to the context file (`~/.zero/todo-context`)
fn context_file_path() -> Option<PathBuf> {
    foundation::dirs::todo_context_path()
}

/// Context stored globally - which todo file is currently open
#[derive(Debug, Clone)]
pub struct TodoContext {
    /// Absolute path to the open todo directory
    pub path: PathBuf,
}

/// Save the current context (which file is open)
pub fn save_context(ctx: &TodoContext) -> Result<(), TodoError> {
    let path = context_file_path().ok_or_else(|| {
        TodoError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not determine config directory",
        ))
    })?;

    // Create config dir if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write path as string
    fs::write(&path, ctx.path.to_string_lossy().as_bytes())?;

    Ok(())
}

/// Load the current context
pub fn load_context() -> Option<TodoContext> {
    let path = context_file_path()?;

    if !path.exists() {
        return None;
    }

    let content = fs::read_to_string(&path).ok()?;
    let todo_path = PathBuf::from(content.trim());

    // Verify the directory still exists
    if todo_path.exists() {
        Some(TodoContext { path: todo_path })
    } else {
        // Directory was deleted, clear context
        let _ = clear_context();
        None
    }
}

/// Clear the current context
pub fn clear_context() -> Result<(), TodoError> {
    if let Some(path) = context_file_path()
        && path.exists()
    {
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// Resolve which todo directory to use
///
/// Priority:
/// 1. Saved context (from `open` command)
/// 2. `project.todo` in current directory
///
/// If nothing exists, returns path to `project.todo` in cwd (will be created on first write)
pub fn resolve_todo_path() -> Result<PathBuf, TodoError> {
    // Check saved context first
    if let Some(ctx) = load_context() {
        return Ok(ctx.path);
    }

    // Default to project.todo in current directory
    let cwd = std::env::current_dir()?;
    Ok(cwd.join(DEFAULT_FILENAME))
}

/// Normalize a path (make absolute if relative)
pub fn normalize_path(path: &Path) -> Result<PathBuf, TodoError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let cwd = std::env::current_dir()?;
        Ok(cwd.join(path))
    }
}
