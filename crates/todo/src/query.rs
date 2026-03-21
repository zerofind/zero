//! Query and filter operations for tasks
//!
//! Provides `TaskFilter` for building complex queries across todo lists.

use super::task::{Task, TaskStatus};

/// Filter criteria for querying tasks
#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    /// Filter by specific list names (None = all lists)
    pub lists: Option<Vec<String>>,
    /// Filter by tags (task must have at least one of these tags)
    pub tags: Option<Vec<String>>,
    /// Filter by assignee
    pub assigned: Option<String>,
    /// Filter by status (task must match one of these statuses)
    pub status: Option<Vec<TaskStatus>>,
    /// Filter tasks due before this timestamp
    pub due_before: Option<i64>,
    /// Filter tasks due after this timestamp
    pub due_after: Option<i64>,
    /// Filter by parent task ID
    pub parent: Option<u64>,
    /// Only include overdue tasks
    pub overdue_only: bool,
    /// Only include tasks due today
    pub due_today_only: bool,
    /// Text search in task description (case-insensitive)
    pub text_search: Option<String>,
    /// Only include top-level tasks (no parent)
    pub top_level_only: bool,
}

impl TaskFilter {
    /// Create a new empty filter (matches all tasks)
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: filter by list names
    #[must_use]
    pub fn with_lists(mut self, lists: Vec<String>) -> Self {
        self.lists = Some(lists);
        self
    }

    /// Builder: filter by a single list
    #[must_use]
    pub fn with_list(mut self, list: impl Into<String>) -> Self {
        self.lists = Some(vec![list.into()]);
        self
    }

    /// Builder: filter by tags
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    /// Builder: filter by a single tag
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags = Some(vec![tag.into()]);
        self
    }

    /// Builder: filter by assignee
    #[must_use]
    pub fn with_assigned(mut self, assigned: impl Into<String>) -> Self {
        self.assigned = Some(assigned.into());
        self
    }

    /// Builder: filter by status
    #[must_use]
    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = Some(vec![status]);
        self
    }

    /// Builder: filter by multiple statuses
    #[must_use]
    pub fn with_statuses(mut self, statuses: Vec<TaskStatus>) -> Self {
        self.status = Some(statuses);
        self
    }

    /// Builder: filter open tasks only
    #[must_use]
    pub fn open_only(mut self) -> Self {
        self.status = Some(vec![TaskStatus::Open, TaskStatus::InProgress]);
        self
    }

    /// Builder: filter closed tasks only
    #[must_use]
    pub fn closed_only(mut self) -> Self {
        self.status = Some(vec![TaskStatus::Done, TaskStatus::Cancelled]);
        self
    }

    /// Builder: filter by due date range
    #[must_use]
    pub fn with_due_before(mut self, timestamp: i64) -> Self {
        self.due_before = Some(timestamp);
        self
    }

    /// Builder: filter by due date range
    #[must_use]
    pub fn with_due_after(mut self, timestamp: i64) -> Self {
        self.due_after = Some(timestamp);
        self
    }

    /// Builder: filter by parent task
    #[must_use]
    pub fn with_parent(mut self, parent_id: u64) -> Self {
        self.parent = Some(parent_id);
        self
    }

    /// Builder: only overdue tasks
    #[must_use]
    pub fn overdue(mut self) -> Self {
        self.overdue_only = true;
        self
    }

    /// Builder: only tasks due today
    #[must_use]
    pub fn due_today(mut self) -> Self {
        self.due_today_only = true;
        self
    }

    /// Builder: search by text
    #[must_use]
    pub fn with_text(mut self, search: impl Into<String>) -> Self {
        self.text_search = Some(search.into());
        self
    }

    /// Builder: only top-level tasks
    #[must_use]
    pub fn top_level(mut self) -> Self {
        self.top_level_only = true;
        self
    }

    /// Check if a task matches this filter
    pub fn matches(&self, task: &Task) -> bool {
        // Check status filter
        if let Some(ref statuses) = self.status
            && !statuses.contains(&task.status)
        {
            return false;
        }

        // Check tags filter (task must have at least one matching tag)
        if let Some(ref tags) = self.tags {
            let has_matching_tag = tags.iter().any(|t| task.has_tag(t));
            if !has_matching_tag {
                return false;
            }
        }

        // Check assignee filter
        if let Some(ref assigned) = self.assigned {
            let matches = task
                .assigned
                .as_ref()
                .is_some_and(|a| a.to_lowercase() == assigned.to_lowercase());
            if !matches {
                return false;
            }
        }

        // Check due_before filter
        if let Some(due_before) = self.due_before {
            match task.due {
                Some(due) if due >= due_before => return false,
                None => return false, // No due date doesn't match "before" filter
                _ => {}
            }
        }

        // Check due_after filter
        if let Some(due_after) = self.due_after {
            match task.due {
                Some(due) if due <= due_after => return false,
                None => return false, // No due date doesn't match "after" filter
                _ => {}
            }
        }

        // Check parent filter
        if let Some(parent_id) = self.parent
            && task.parent != Some(parent_id)
        {
            return false;
        }

        // Check overdue filter
        if self.overdue_only && !task.is_overdue() {
            return false;
        }

        // Check due today filter
        if self.due_today_only && !task.is_due_today() {
            return false;
        }

        // Check text search
        if let Some(ref search) = self.text_search {
            let search_lower = search.to_lowercase();
            if !task.text.to_lowercase().contains(&search_lower) {
                return false;
            }
        }

        // Check top-level filter
        if self.top_level_only && task.parent.is_some() {
            return false;
        }

        true
    }

    /// Filter a slice of tasks
    pub fn filter<'a>(&self, tasks: &'a [Task]) -> Vec<&'a Task> {
        tasks.iter().filter(|t| self.matches(t)).collect()
    }

    /// Filter and sort tasks by various criteria
    pub fn filter_sorted<'a>(&self, tasks: &'a [Task], sort: TaskSort) -> Vec<&'a Task> {
        let mut filtered: Vec<&Task> = tasks.iter().filter(|t| self.matches(t)).collect();
        sort.apply(&mut filtered);
        filtered
    }
}

/// Sorting options for task queries
#[derive(Debug, Clone, Copy, Default)]
pub enum TaskSort {
    /// Sort by creation date (newest first)
    #[default]
    CreatedDesc,
    /// Sort by creation date (oldest first)
    CreatedAsc,
    /// Sort by modification date (newest first)
    ModifiedDesc,
    /// Sort by modification date (oldest first)
    ModifiedAsc,
    /// Sort by due date (earliest first, no due date last)
    DueAsc,
    /// Sort by due date (latest first, no due date last)
    DueDesc,
    /// Sort by status (open first, then in progress, then done, then cancelled)
    StatusAsc,
}

impl TaskSort {
    /// Apply sorting to a mutable slice of task references
    pub fn apply(self, tasks: &mut [&Task]) {
        match self {
            TaskSort::CreatedDesc => tasks.sort_by_key(|a| std::cmp::Reverse(a.created)),
            TaskSort::CreatedAsc => tasks.sort_by_key(|a| a.created),
            TaskSort::ModifiedDesc => tasks.sort_by_key(|a| std::cmp::Reverse(a.modified)),
            TaskSort::ModifiedAsc => tasks.sort_by_key(|a| a.modified),
            TaskSort::DueAsc => tasks.sort_by(|a, b| match (a.due, b.due) {
                (Some(da), Some(db)) => da.cmp(&db),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }),
            TaskSort::DueDesc => tasks.sort_by(|a, b| match (a.due, b.due) {
                (Some(da), Some(db)) => db.cmp(&da),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }),
            TaskSort::StatusAsc => tasks.sort_by(|a, b| {
                let status_order = |s: &TaskStatus| match s {
                    TaskStatus::Open => 0,
                    TaskStatus::InProgress => 1,
                    TaskStatus::Done => 2,
                    TaskStatus::Cancelled => 3,
                };
                status_order(&a.status).cmp(&status_order(&b.status))
            }),
        }
    }
}
