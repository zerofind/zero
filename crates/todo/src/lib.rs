//! Todo module - task management with etch-backed storage
//!
//! Provides a simple, privacy-first todo system that stores tasks in local
//! `.todo` directories using etchdb (WAL + snapshot persistence).
//!
//! ## Concepts
//!
//! - **`TodoFile`**: A collection of tasks organized by lists
//! - **List**: A grouping within a file (e.g., "inbox", "bugs", "features")
//! - **Task**: An individual todo item with status, tags, assignee, etc.
//!
//! ## Usage
//!
//! ```ignore
//! use todo::{TodoManager, Task};
//!
//! // Open current context (or project.todo by default)
//! let mut manager = TodoManager::open_current()?;
//!
//! // Add tasks to different lists
//! manager.add("Fix crash on startup", Some("bugs"))?;
//! manager.add("Add dark mode", Some("features"))?;
//! manager.add("Review PR", None)?; // goes to "inbox"
//!
//! // Complete tasks
//! manager.complete(1)?;
//! ```
//!
//! ## CLI
//!
//! ```bash
//! zero todo                          # Show status + all tasks
//! zero todo open SECURITY.todo       # Switch to file
//! zero todo add "Fix bug" bugs       # Add to bugs list
//! zero todo done 1 2 3               # Complete tasks
//! zero todo bugs                     # Show bugs list only
//! ```
//!
//! ## Suggested Lists
//!
//! | List | Use for |
//! |------|---------|
//! | `inbox` | Unsorted tasks (default) |
//! | `bugs` | Defects, issues to fix |
//! | `features` | New functionality |
//! | `refactor` | Code improvements |
//! | `security` | Security concerns |
//! | `docs` | Documentation tasks |
//!
//! ## Storage
//!
//! Each `.todo` path is an etch directory containing `snapshot.postcard` + `wal.bin`.
//! WAL provides crash recovery; snapshots provide fast cold starts.

mod file;
mod manager;
mod query;
mod search;
pub mod state;
mod storage;
mod task;

pub use file::{ListCounts, StatusCounts, TodoFile};
pub use manager::{TodoError, TodoManager, TodoResult};
pub use query::{TaskFilter, TaskSort};
pub use search::{
    SearchError, TaskRef, TaskSearchResult, TodoSearchOptions, find_todo_files, search_todos,
};
pub use state::TodoState;
pub use storage::{DEFAULT_FILENAME, resolve_todo_path};
pub use task::{DEFAULT_LIST, Task, TaskStatus};

#[cfg(test)]
mod file_test;
#[cfg(test)]
mod manager_test;
#[cfg(test)]
mod search_test;
#[cfg(test)]
mod state_test;
#[cfg(test)]
mod storage_test;
#[cfg(test)]
mod task_test;
