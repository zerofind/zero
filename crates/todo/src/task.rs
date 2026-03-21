//! Task model and operations
//!
//! Defines the core `Task` struct and `TaskStatus` enum used throughout
//! the todo module.

use serde::{Deserialize, Serialize};

use foundation::util::now_timestamp;

/// Status of a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskStatus {
    /// Task is open and not started
    #[default]
    Open,
    /// Task is in progress
    InProgress,
    /// Task is completed
    Done,
    /// Task was cancelled
    Cancelled,
}

impl TaskStatus {
    /// Check if the task is considered "closed" (done or cancelled)
    #[inline]
    pub fn is_closed(&self) -> bool {
        matches!(self, TaskStatus::Done | TaskStatus::Cancelled)
    }

    /// Check if the task is open (not closed)
    #[inline]
    pub fn is_open(&self) -> bool {
        !self.is_closed()
    }

    /// Parse status from string
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "open" | "o" => Some(TaskStatus::Open),
            "inprogress" | "in_progress" | "in-progress" | "ip" | "wip" => {
                Some(TaskStatus::InProgress)
            }
            "done" | "d" | "completed" | "complete" => Some(TaskStatus::Done),
            "cancelled" | "canceled" | "c" => Some(TaskStatus::Cancelled),
            _ => None,
        }
    }

    /// Get display name
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Open => "open",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Done => "done",
            TaskStatus::Cancelled => "cancelled",
        }
    }
}

/// A task in a todo list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier within the file
    pub id: u64,
    /// List name (e.g., "inbox", "bugs", "features")
    pub list: String,
    /// Current status
    pub status: TaskStatus,
    /// Task description text
    pub text: String,
    /// Creation timestamp (unix seconds)
    pub created: i64,
    /// Last modification timestamp (unix seconds)
    pub modified: i64,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Assignee (user or agent identifier)
    pub assigned: Option<String>,
    /// Due date (unix timestamp, None if no due date)
    pub due: Option<i64>,
    /// Completion timestamp (set when status becomes Done/Cancelled)
    pub completed: Option<i64>,
    /// Parent task ID for subtasks
    pub parent: Option<u64>,
    /// Indentation level for display (0 = top-level)
    pub indent_level: u8,
}

/// Default list name for new tasks
pub const DEFAULT_LIST: &str = "inbox";

impl Task {
    /// Create a new task with the given text
    ///
    /// The task is created with:
    /// - ID of 0 (should be assigned by `TodoFile`)
    /// - List of "inbox" (default)
    /// - Status of Open
    /// - Current timestamp for created/modified
    pub fn new(text: impl Into<String>) -> Self {
        let now = now_timestamp();
        Self {
            id: 0,
            list: DEFAULT_LIST.to_string(),
            status: TaskStatus::Open,
            text: text.into(),
            created: now,
            modified: now,
            tags: Vec::new(),
            assigned: None,
            due: None,
            completed: None,
            parent: None,
            indent_level: 0,
        }
    }

    /// Builder: set the list
    #[must_use]
    pub fn with_list(mut self, list: impl Into<String>) -> Self {
        self.list = list.into();
        self
    }

    /// Builder: add a tag
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Builder: add multiple tags
    #[must_use]
    pub fn with_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags.extend(tags.into_iter().map(Into::into));
        self
    }

    /// Builder: set assignee
    #[must_use]
    pub fn with_assigned(mut self, assigned: impl Into<String>) -> Self {
        self.assigned = Some(assigned.into());
        self
    }

    /// Builder: set due date
    #[must_use]
    pub fn with_due(mut self, due: i64) -> Self {
        self.due = Some(due);
        self
    }

    /// Builder: set parent task
    #[must_use]
    pub fn with_parent(mut self, parent_id: u64, indent: u8) -> Self {
        self.parent = Some(parent_id);
        self.indent_level = indent;
        self
    }

    /// Builder: set indent level
    #[must_use]
    pub fn with_indent(mut self, level: u8) -> Self {
        self.indent_level = level;
        self
    }

    /// Check if the task is overdue
    ///
    /// Returns true if there's a due date and it's in the past
    pub fn is_overdue(&self) -> bool {
        if self.status.is_closed() {
            return false;
        }
        match self.due {
            Some(due) => due < now_timestamp(),
            None => false,
        }
    }

    /// Check if the task is due today
    ///
    /// Returns true if due date is within the current day (UTC)
    pub fn is_due_today(&self) -> bool {
        if self.status.is_closed() {
            return false;
        }
        match self.due {
            Some(due) => {
                let now = now_timestamp();
                let day_start = now - (now % 86400);
                let day_end = day_start + 86400;
                due >= day_start && due < day_end
            }
            None => false,
        }
    }

    /// Update the modified timestamp to now
    pub fn touch(&mut self) {
        self.modified = now_timestamp();
    }

    /// Mark the task as complete
    pub fn complete(&mut self) {
        self.status = TaskStatus::Done;
        self.completed = Some(now_timestamp());
        self.touch();
    }

    /// Mark the task as cancelled
    pub fn cancel(&mut self) {
        self.status = TaskStatus::Cancelled;
        self.completed = Some(now_timestamp());
        self.touch();
    }

    /// Reopen a closed task
    pub fn reopen(&mut self) {
        self.status = TaskStatus::Open;
        self.completed = None;
        self.touch();
    }

    /// Check if the task has a specific tag (case-insensitive)
    pub fn has_tag(&self, tag: &str) -> bool {
        let tag_lower = tag.to_lowercase();
        self.tags.iter().any(|t| t.to_lowercase() == tag_lower)
    }

    /// Validate the task
    ///
    /// Returns an error message if the task is invalid
    pub fn validate(&self) -> Option<&'static str> {
        if self.text.trim().is_empty() {
            return Some("Task text cannot be empty");
        }
        if self.text.len() > 10000 {
            return Some("Task text too long (max 10000 chars)");
        }
        if self.tags.len() > 100 {
            return Some("Too many tags (max 100)");
        }
        if self.indent_level > 10 {
            return Some("Indent level too deep (max 10)");
        }
        None
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
