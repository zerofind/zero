//! `TodoManager` - main orchestration for todo operations
//!
//! The manager provides a high-level API for working with todo files.
//! It handles context (which file is open) and persistence via etchdb.

use std::path::{Path, PathBuf};

use etchdb::{Store, WalBackend};
use thiserror::Error;

use super::state::TodoState;
use super::storage::{
    TodoContext, clear_context, load_context, normalize_path, open_store, resolve_todo_path,
    save_context,
};
use super::task::{Task, TaskStatus};

/// Errors that can occur during todo operations
#[derive(Error, Debug)]
pub enum TodoError {
    /// Task not found
    #[error("Task not found: {0}")]
    TaskNotFound(u64),

    /// No todo file is open
    #[error("No todo file open. Use 'zero todo open <file>' first.")]
    NoFileOpen,

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// Task validation failed
    #[error("Invalid task: {0}")]
    Validation(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Result type for todo operations
pub type TodoResult<T> = Result<T, TodoError>;

/// Convert `TodoError` -> `etchdb::Error` for use inside store.write closures
fn to_etch(e: TodoError) -> etchdb::Error {
    etchdb::Error::Invalid {
        field: "todo",
        message: e.to_string(),
    }
}

/// Convert `etchdb::Error` -> `TodoError`
fn from_etch(e: etchdb::Error) -> TodoError {
    TodoError::Io(std::io::Error::other(e.to_string()))
}

/// Manager for todo files
///
/// Holds an etch store for crash-safe persistence.
pub struct TodoManager {
    /// Etch store wrapping `TodoState`
    store: Store<TodoState, WalBackend<TodoState>>,
    /// Path to the current etch directory
    path: PathBuf,
}

impl TodoManager {
    /// Open the todo manager with the current context or default file
    pub fn open_current() -> TodoResult<Self> {
        let path = resolve_todo_path()?;
        let store = open_store(&path)?;
        Ok(Self { store, path })
    }

    /// Open a specific todo directory
    pub fn open_file(path: impl AsRef<Path>) -> TodoResult<Self> {
        let path = normalize_path(path.as_ref())?;
        let store = open_store(&path)?;
        save_context(&TodoContext { path: path.clone() })?;
        Ok(Self { store, path })
    }

    /// Close the current context (revert to default project.todo)
    pub fn close() -> TodoResult<()> {
        clear_context()
    }

    /// Get the current context (which file is open)
    pub fn current_context() -> Option<TodoContext> {
        load_context()
    }

    /// Get the path to the current directory
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the filename (without directory)
    pub fn filename(&self) -> &str {
        self.path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
    }

    /// Get a reference to the current state (snapshot read)
    pub fn file(&self) -> etchdb::Ref<'_, TodoState> {
        self.store.read()
    }

    // =========================================================================
    // Task operations
    // =========================================================================

    /// Add a task. Returns the ID of the newly created task.
    pub fn add_task(&mut self, task: Task) -> TodoResult<u64> {
        if let Some(err) = task.validate() {
            return Err(TodoError::Validation(err.to_string()));
        }

        self.store
            .write(|tx| {
                let next_id = tx.state.file.next_id();
                let mut task = task;
                task.id = next_id;
                tx.put_task(&task);
                Ok(next_id)
            })
            .map_err(from_etch)
    }

    /// Add a task with text and optional list
    pub fn add(&mut self, text: &str, list: Option<&str>) -> TodoResult<u64> {
        let mut task = Task::new(text);
        if let Some(l) = list {
            task = task.with_list(l);
        }
        self.add_task(task)
    }

    /// Get a task by ID (cloned)
    pub fn get_task(&self, id: u64) -> TodoResult<Task> {
        let state = self.store.read();
        state
            .file
            .get_task(id)
            .cloned()
            .ok_or(TodoError::TaskNotFound(id))
    }

    /// Update a task's text
    pub fn update_text(&mut self, id: u64, text: &str) -> TodoResult<()> {
        let text = text.to_string();
        self.store
            .write(|tx| {
                let mut task = tx
                    .get_task(id)
                    .cloned()
                    .ok_or_else(|| to_etch(TodoError::TaskNotFound(id)))?;
                task.text = text;
                task.touch();
                tx.put_task(&task);
                Ok(())
            })
            .map_err(from_etch)
    }

    /// Set task status
    pub fn set_status(&mut self, id: u64, status: TaskStatus) -> TodoResult<()> {
        self.store
            .write(|tx| {
                let mut task = tx
                    .get_task(id)
                    .cloned()
                    .ok_or_else(|| to_etch(TodoError::TaskNotFound(id)))?;

                match status {
                    TaskStatus::Done => task.complete(),
                    TaskStatus::Cancelled => task.cancel(),
                    TaskStatus::Open | TaskStatus::InProgress => {
                        task.status = status;
                        task.completed = None;
                        task.touch();
                    }
                }

                tx.put_task(&task);
                Ok(())
            })
            .map_err(from_etch)
    }

    /// Complete a task
    pub fn complete(&mut self, id: u64) -> TodoResult<()> {
        self.set_status(id, TaskStatus::Done)
    }

    /// Complete multiple tasks
    pub fn complete_many(&mut self, ids: &[u64]) -> TodoResult<Vec<u64>> {
        let ids = ids.to_vec();
        self.store
            .write(|tx| {
                let mut completed = Vec::new();
                for id in &ids {
                    if let Some(mut task) = tx.get_task(*id).cloned() {
                        task.complete();
                        tx.put_task(&task);
                        completed.push(*id);
                    }
                }
                Ok(completed)
            })
            .map_err(from_etch)
    }

    /// Reopen a task
    pub fn reopen(&mut self, id: u64) -> TodoResult<()> {
        self.store
            .write(|tx| {
                let mut task = tx
                    .get_task(id)
                    .cloned()
                    .ok_or_else(|| to_etch(TodoError::TaskNotFound(id)))?;
                task.reopen();
                tx.put_task(&task);
                Ok(())
            })
            .map_err(from_etch)
    }

    /// Remove a task
    pub fn remove(&mut self, id: u64) -> TodoResult<Task> {
        self.store
            .write(|tx| {
                let task = tx
                    .get_task(id)
                    .cloned()
                    .ok_or_else(|| to_etch(TodoError::TaskNotFound(id)))?;
                tx.delete_task(id);
                Ok(task)
            })
            .map_err(from_etch)
    }

    /// Assign a task
    pub fn assign(&mut self, id: u64, assignee: &str) -> TodoResult<()> {
        let assignee = assignee.to_string();
        self.store
            .write(|tx| {
                let mut task = tx
                    .get_task(id)
                    .cloned()
                    .ok_or_else(|| to_etch(TodoError::TaskNotFound(id)))?;
                task.assigned = Some(assignee);
                task.touch();
                tx.put_task(&task);
                Ok(())
            })
            .map_err(from_etch)
    }

    /// Unassign a task
    pub fn unassign(&mut self, id: u64) -> TodoResult<()> {
        self.store
            .write(|tx| {
                let mut task = tx
                    .get_task(id)
                    .cloned()
                    .ok_or_else(|| to_etch(TodoError::TaskNotFound(id)))?;
                task.assigned = None;
                task.touch();
                tx.put_task(&task);
                Ok(())
            })
            .map_err(from_etch)
    }

    /// Set task due date
    pub fn set_due(&mut self, id: u64, due: Option<i64>) -> TodoResult<()> {
        self.store
            .write(|tx| {
                let mut task = tx
                    .get_task(id)
                    .cloned()
                    .ok_or_else(|| to_etch(TodoError::TaskNotFound(id)))?;
                task.due = due;
                task.touch();
                tx.put_task(&task);
                Ok(())
            })
            .map_err(from_etch)
    }

    /// Add a tag to a task
    pub fn add_tag(&mut self, id: u64, tag: &str) -> TodoResult<()> {
        let tag = tag.to_string();
        self.store
            .write(|tx| {
                let mut task = tx
                    .get_task(id)
                    .cloned()
                    .ok_or_else(|| to_etch(TodoError::TaskNotFound(id)))?;
                if !task.has_tag(&tag) {
                    task.tags.push(tag);
                    task.touch();
                    tx.put_task(&task);
                }
                Ok(())
            })
            .map_err(from_etch)
    }

    /// Remove a tag from a task
    pub fn remove_tag(&mut self, id: u64, tag: &str) -> TodoResult<()> {
        let tag_lower = tag.to_lowercase();
        self.store
            .write(|tx| {
                let mut task = tx
                    .get_task(id)
                    .cloned()
                    .ok_or_else(|| to_etch(TodoError::TaskNotFound(id)))?;
                let original_len = task.tags.len();
                task.tags.retain(|t| t.to_lowercase() != tag_lower);
                if task.tags.len() != original_len {
                    task.touch();
                    tx.put_task(&task);
                }
                Ok(())
            })
            .map_err(from_etch)
    }

    /// Move task to a different list (legacy - use `move_task` for reordering)
    pub fn move_to_list(&mut self, id: u64, list: &str) -> TodoResult<()> {
        let list = list.to_string();
        self.store
            .write(|tx| {
                let mut task = tx
                    .get_task(id)
                    .cloned()
                    .ok_or_else(|| to_etch(TodoError::TaskNotFound(id)))?;
                task.list = list;
                task.touch();
                tx.put_task(&task);
                Ok(())
            })
            .map_err(from_etch)
    }

    /// Move a task within or between lists
    ///
    /// Task list changes are persisted via WAL.
    /// Ordering within the Vec is captured by the next snapshot.
    pub fn move_task(
        &mut self,
        id: u64,
        to_list: Option<&str>,
        _after_id: Option<u64>,
    ) -> TodoResult<()> {
        let to_list_owned = to_list.map(String::from);
        self.store
            .write(|tx| {
                // Verify task exists
                let _task = tx
                    .get_task(id)
                    .ok_or_else(|| to_etch(TodoError::TaskNotFound(id)))?;

                // If changing list, emit a Put for the updated task
                if let Some(ref list) = to_list_owned {
                    let mut task = tx.get_task(id).cloned().unwrap();
                    task.list = list.clone();
                    task.touch();
                    tx.put_task(&task);
                }
                Ok(())
            })
            .map_err(from_etch)
    }

    // =========================================================================
    // Query operations
    // =========================================================================

    /// Get all list names in the file
    pub fn list_names(&self) -> Vec<String> {
        self.store.read().file.list_names()
    }

    /// Get tasks in a specific list (cloned)
    pub fn tasks_in_list(&self, list: &str) -> Vec<Task> {
        let state = self.store.read();
        state
            .file
            .tasks_in_list(list)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get open tasks in a specific list (cloned)
    pub fn open_tasks_in_list(&self, list: &str) -> Vec<Task> {
        let state = self.store.read();
        state
            .file
            .open_tasks_in_list(list)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get all open tasks (cloned)
    pub fn open_tasks(&self) -> Vec<Task> {
        let state = self.store.read();
        state.file.open_tasks().cloned().collect()
    }

    /// Get all tasks (cloned)
    pub fn all_tasks(&self) -> Vec<Task> {
        self.store.read().file.tasks.clone()
    }
}
