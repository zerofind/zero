//! TodoFile - a single todo file containing tasks across multiple lists
//!
//! A TodoFile is the unit of storage (e.g., `project.todo`, `SECURITY_AUDIT.todo`).
//! Tasks within the file have a `list` field for grouping (e.g., "inbox", "bugs", "features").

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::util::now_timestamp;

use super::task::{DEFAULT_LIST, Task, TaskStatus};

/// Persistent format version for TodoFile
/// Increment when the serialization format changes
pub const FILE_VERSION: u32 = 1;

/// A todo file containing tasks organized by lists
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoFile {
    /// Format version for compatibility
    pub version: u32,
    /// All tasks in the file
    pub tasks: Vec<Task>,
    /// Optional notes/description for the file
    pub notes: Option<String>,
    /// Creation timestamp (unix seconds)
    pub created: i64,
    /// Last modification timestamp (unix seconds)
    pub modified: i64,
    /// Next task ID to assign
    next_id: u64,
}

impl Default for TodoFile {
    fn default() -> Self {
        Self::new()
    }
}

impl TodoFile {
    /// Create a new empty todo file
    pub fn new() -> Self {
        let now = now_timestamp();
        Self {
            version: FILE_VERSION,
            tasks: Vec::new(),
            notes: None,
            created: now,
            modified: now,
            next_id: 1,
        }
    }

    /// Get the next task ID that will be assigned
    #[inline]
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    /// Get the number of tasks in the file
    #[inline]
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Check if the file is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Add a task to the file
    ///
    /// The task's ID will be assigned automatically.
    /// Returns the assigned task ID.
    pub fn add_task(&mut self, mut task: Task) -> u64 {
        task.id = self.next_id;
        self.next_id += 1;
        self.tasks.push(task);
        self.touch();
        self.next_id - 1
    }

    /// Get a task by ID
    pub fn get_task(&self, id: u64) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// Get a mutable reference to a task by ID
    pub fn get_task_mut(&mut self, id: u64) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    /// Remove a task by ID
    ///
    /// Returns the removed task if found
    pub fn remove_task(&mut self, id: u64) -> Option<Task> {
        if let Some(pos) = self.tasks.iter().position(|t| t.id == id) {
            self.touch();
            Some(self.tasks.remove(pos))
        } else {
            None
        }
    }

    /// Complete a task by ID
    ///
    /// Returns true if the task was found and completed
    pub fn complete_task(&mut self, id: u64) -> bool {
        if let Some(task) = self.get_task_mut(id) {
            task.complete();
            self.touch();
            true
        } else {
            false
        }
    }

    /// Reopen a task by ID
    ///
    /// Returns true if the task was found and reopened
    pub fn reopen_task(&mut self, id: u64) -> bool {
        if let Some(task) = self.get_task_mut(id) {
            task.reopen();
            self.touch();
            true
        } else {
            false
        }
    }

    /// Move a task within or between lists
    ///
    /// # Arguments
    /// * `id` - Task ID to move
    /// * `to_list` - Destination list (None = stay in current list)
    /// * `after_id` - Place after this task ID (None = put at top of list)
    ///
    /// # Returns
    /// true if the task was moved successfully
    pub fn move_task(&mut self, id: u64, to_list: Option<&str>, after_id: Option<u64>) -> bool {
        // Find and remove the task
        let task_pos = match self.tasks.iter().position(|t| t.id == id) {
            Some(pos) => pos,
            None => return false,
        };

        let mut task = self.tasks.remove(task_pos);

        // Update list if specified
        let target_list = match to_list {
            Some(list) => {
                task.list = list.to_string();
                task.touch();
                list
            }
            None => task.list.as_str(),
        };

        // Find insertion position
        let insert_pos = match after_id {
            Some(aid) => {
                // Find the task to insert after
                match self.tasks.iter().position(|t| t.id == aid) {
                    Some(pos) => pos + 1,
                    None => {
                        // after_id not found, put at end of target list
                        self.find_list_end(target_list)
                    }
                }
            }
            None => {
                // Put at top of target list
                self.find_list_start(target_list)
            }
        };

        self.tasks.insert(insert_pos, task);
        self.touch();
        true
    }

    /// Find the index where a list starts (first task in that list)
    fn find_list_start(&self, list: &str) -> usize {
        self.tasks
            .iter()
            .position(|t| t.list == list)
            .unwrap_or(self.tasks.len())
    }

    /// Find the index after the last task in a list
    fn find_list_end(&self, list: &str) -> usize {
        self.tasks
            .iter()
            .enumerate()
            .rfind(|(_, t)| t.list == list)
            .map(|(i, _)| i + 1)
            .unwrap_or(self.tasks.len())
    }

    /// Get all unique list names in the file
    pub fn list_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tasks.iter().map(|t| t.list.clone()).collect();
        names.sort();
        names.dedup();

        // Ensure inbox is first if present
        if let Some(pos) = names.iter().position(|n| n == DEFAULT_LIST) {
            names.remove(pos);
            names.insert(0, DEFAULT_LIST.to_string());
        }

        names
    }

    /// Get tasks in a specific list
    pub fn tasks_in_list(&self, list: &str) -> Vec<&Task> {
        self.tasks.iter().filter(|t| t.list == list).collect()
    }

    /// Get open tasks in a specific list
    pub fn open_tasks_in_list(&self, list: &str) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|t| t.list == list && t.status.is_open())
            .collect()
    }

    /// Get all open tasks
    pub fn open_tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter().filter(|t| t.status.is_open())
    }

    /// Get all closed tasks
    pub fn closed_tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter().filter(|t| t.status.is_closed())
    }

    /// Count tasks grouped by list
    pub fn count_by_list(&self) -> HashMap<String, ListCounts> {
        let mut counts: HashMap<String, ListCounts> = HashMap::new();

        for task in &self.tasks {
            let entry = counts.entry(task.list.clone()).or_default();
            entry.total += 1;
            if task.status.is_open() {
                entry.open += 1;
            } else {
                entry.closed += 1;
            }
        }

        counts
    }

    /// Count tasks by status (across all lists)
    pub fn count_by_status(&self) -> StatusCounts {
        let mut counts = StatusCounts::default();
        for task in &self.tasks {
            match task.status {
                TaskStatus::Open => counts.open += 1,
                TaskStatus::InProgress => counts.in_progress += 1,
                TaskStatus::Done => counts.done += 1,
                TaskStatus::Cancelled => counts.cancelled += 1,
            }
        }
        counts
    }

    /// Insert or replace a task by ID.
    ///
    /// If a task with the same ID exists, it is replaced in-place.
    /// Otherwise the task is appended and `next_id` is bumped if needed.
    pub fn upsert_task(&mut self, id: u64, task: Task) {
        if let Some(pos) = self.tasks.iter().position(|t| t.id == id) {
            self.tasks[pos] = task;
        } else {
            self.tasks.push(task);
            if id >= self.next_id {
                self.next_id = id + 1;
            }
        }
        self.touch();
    }

    /// Update the modified timestamp
    fn touch(&mut self) {
        self.modified = now_timestamp();
    }

    /// Get all unique tags in the file
    pub fn all_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .tasks
            .iter()
            .flat_map(|t| t.tags.iter().cloned())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }

    /// Get all unique assignees in the file
    pub fn all_assignees(&self) -> Vec<String> {
        let mut assignees: Vec<String> = self
            .tasks
            .iter()
            .filter_map(|t| t.assigned.clone())
            .collect();
        assignees.sort();
        assignees.dedup();
        assignees
    }
}

/// Counts for a single list
#[derive(Debug, Default, Clone, Copy)]
pub struct ListCounts {
    pub total: usize,
    pub open: usize,
    pub closed: usize,
}

/// Counts of tasks by status
#[derive(Debug, Default, Clone, Copy)]
pub struct StatusCounts {
    pub open: usize,
    pub in_progress: usize,
    pub done: usize,
    pub cancelled: usize,
}

impl StatusCounts {
    /// Total number of tasks
    pub fn total(&self) -> usize {
        self.open + self.in_progress + self.done + self.cancelled
    }

    /// Number of active (non-closed) tasks
    pub fn active(&self) -> usize {
        self.open + self.in_progress
    }

    /// Number of closed tasks
    pub fn closed(&self) -> usize {
        self.done + self.cancelled
    }
}
