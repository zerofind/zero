use std::time::{SystemTime, UNIX_EPOCH};

use gpui::*;
use gpui_component::input::InputState;

use todo::{TaskStatus, TodoManager};

use super::render::TodoView;

impl TodoView {
    /// Open a specific .todo file and reload tasks.
    pub fn open_file(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        tracing::debug!(path = %path.display(), "todo: open file");
        match TodoManager::open_file(&path) {
            Ok(manager) => {
                self.lists = manager.list_names();
                self.tasks = manager.all_tasks().to_vec();
                self.manager = Some(manager);
                self.error = None;
            }
            Err(e) => {
                self.tasks = Vec::new();
                self.lists = Vec::new();
                self.error = Some(format!("Failed to open {}: {e}", path.display()));
            }
        }
        cx.notify();
    }

    pub(super) fn load_tasks(&mut self) {
        let todo_path = std::env::current_dir()
            .unwrap_or_default()
            .join("project.todo");

        match TodoManager::open_file(&todo_path) {
            Ok(manager) => {
                self.lists = manager.list_names();
                self.tasks = manager.all_tasks().to_vec();
                self.manager = Some(manager);
                self.error = None;
            }
            Err(e) => {
                self.tasks = Vec::new();
                self.lists = Vec::new();
                self.error = Some(format!("No todo file found: {e}"));
            }
        }
    }

    pub(super) fn add_task(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!(text, "todo: add task");
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let Some(manager) = &mut self.manager else {
            return;
        };

        let mut task = todo::Task::new(text);
        if self.current_list != "All" {
            task.list = self.current_list.clone();
        }
        let _ = manager.add_task(task);
        self.refresh_from_manager();

        self.input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        cx.notify();
    }

    pub(super) fn delete_task(&mut self, task_id: u64, cx: &mut Context<Self>) {
        tracing::debug!(task_id, "todo: delete task");
        let Some(manager) = &mut self.manager else {
            return;
        };
        let _ = manager.remove(task_id);
        self.refresh_from_manager();
        self.pending_delete = None;
        cx.notify();
    }

    pub(super) fn toggle_task(&mut self, task_id: u64, cx: &mut Context<Self>) {
        tracing::debug!(task_id, "todo: toggle task");
        let Some(manager) = &mut self.manager else {
            return;
        };

        // Check current status to decide: reopen or complete
        let is_closed = manager
            .get_task(task_id)
            .map(|t| t.status.is_closed())
            .unwrap_or(false);

        if is_closed {
            let _ = manager.reopen(task_id);
        } else {
            let _ = manager.complete(task_id);
        }
        self.refresh_from_manager();
        cx.notify();
    }

    pub(super) fn start_editing(
        &mut self,
        task_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(task_id, "todo: start editing");
        let Some(manager) = &self.manager else {
            return;
        };
        let Some(task) = manager.get_task(task_id).ok() else {
            return;
        };

        let edit_input = cx
            .new(|cx| gpui_component::input::InputState::new(window, cx).default_value(&task.text));
        self.editing_task = Some(task_id);
        self.edit_input = Some(edit_input);
        cx.notify();
    }

    pub(super) fn confirm_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!("todo: confirm edit");
        let Some(task_id) = self.editing_task else {
            return;
        };
        let Some(input) = &self.edit_input else {
            return;
        };

        let new_text = input.read(cx).value().to_string();
        let new_text = new_text.trim();

        if !new_text.is_empty()
            && let Some(manager) = &mut self.manager
        {
            let _ = manager.update_text(task_id, new_text);
            self.refresh_from_manager();
        }

        self.editing_task = None;
        self.edit_input = None;
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub(super) fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("todo: cancel edit");
        self.editing_task = None;
        self.edit_input = None;
        cx.notify();
    }

    pub(super) fn set_current_list(&mut self, list: String, cx: &mut Context<Self>) {
        tracing::debug!(list = %list, "todo: set list");
        self.current_list = list;
        cx.notify();
    }

    pub(super) fn toggle_show_completed(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("todo: toggle show completed");
        self.show_completed = !self.show_completed;
        cx.notify();
    }

    pub(super) fn filtered_tasks(&self) -> Vec<&todo::Task> {
        self.tasks
            .iter()
            .filter(|t| {
                // List filter
                if self.current_list != "All" && t.list != self.current_list {
                    return false;
                }
                // Show completed filter
                if !self.show_completed
                    && (t.status == TaskStatus::Done || t.status == TaskStatus::Cancelled)
                {
                    return false;
                }
                true
            })
            .collect()
    }

    fn refresh_from_manager(&mut self) {
        if let Some(manager) = &self.manager {
            self.tasks = manager.all_tasks().to_vec();
            self.lists = manager.list_names();
        }
    }

    // -- Detail panel actions --

    pub(super) fn toggle_detail(
        &mut self,
        task_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.detail_task == Some(task_id) {
            self.close_detail();
        } else {
            self.open_detail(task_id, window, cx);
        }
        cx.notify();
    }

    fn open_detail(&mut self, task_id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let task = self.tasks.iter().find(|t| t.id == task_id);

        let due_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("today, tomorrow, YYYY-MM-DD");
            if let Some(t) = &task
                && let Some(ts) = t.due
            {
                input = input.default_value(format_timestamp_date(ts));
            }
            input
        });

        let tag_input = cx.new(|cx| InputState::new(window, cx).placeholder("new tag"));

        let assignee_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("assignee name");
            if let Some(t) = &task
                && let Some(ref a) = t.assigned
            {
                input = input.default_value(a);
            }
            input
        });

        self.detail_task = Some(task_id);
        self.detail_due_input = Some(due_input);
        self.detail_tag_input = Some(tag_input);
        self.detail_assignee_input = Some(assignee_input);
    }

    fn close_detail(&mut self) {
        self.detail_task = None;
        self.detail_due_input = None;
        self.detail_tag_input = None;
        self.detail_assignee_input = None;
    }

    pub(super) fn set_due(&mut self, task_id: u64, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!(task_id, "todo: set due");
        let Some(input) = &self.detail_due_input else {
            return;
        };
        let raw = input.read(cx).value().trim().to_string();
        if raw.is_empty() {
            return;
        }

        let timestamp = parse_due_date(&raw);
        let Some(manager) = &mut self.manager else {
            return;
        };
        let _ = manager.set_due(task_id, timestamp);
        self.refresh_from_manager();
        // Re-open detail to refresh inputs
        self.open_detail(task_id, window, cx);
        cx.notify();
    }

    pub(super) fn clear_due(&mut self, task_id: u64, cx: &mut Context<Self>) {
        tracing::debug!(task_id, "todo: clear due");
        let Some(manager) = &mut self.manager else {
            return;
        };
        let _ = manager.set_due(task_id, None);
        self.refresh_from_manager();
        cx.notify();
    }

    pub(super) fn add_tag_from_input(
        &mut self,
        task_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(task_id, "todo: add tag");
        let Some(input) = &self.detail_tag_input else {
            return;
        };
        let tag = input.read(cx).value().trim().to_string();
        if tag.is_empty() {
            return;
        }

        let Some(manager) = &mut self.manager else {
            return;
        };
        let _ = manager.add_tag(task_id, &tag);
        self.refresh_from_manager();

        // Clear tag input
        if let Some(input) = &self.detail_tag_input {
            input.update(cx, |state, cx| {
                state.set_value("", window, cx);
            });
        }
        cx.notify();
    }

    pub(super) fn remove_tag(&mut self, task_id: u64, tag: &str, cx: &mut Context<Self>) {
        tracing::debug!(task_id, tag, "todo: remove tag");
        let Some(manager) = &mut self.manager else {
            return;
        };
        let _ = manager.remove_tag(task_id, tag);
        self.refresh_from_manager();
        cx.notify();
    }

    pub(super) fn set_assignee(
        &mut self,
        task_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = &self.detail_assignee_input else {
            return;
        };
        let assignee = input.read(cx).value().trim().to_string();
        if assignee.is_empty() {
            return;
        }

        let Some(manager) = &mut self.manager else {
            return;
        };
        let _ = manager.assign(task_id, &assignee);
        self.refresh_from_manager();
        self.open_detail(task_id, window, cx);
        cx.notify();
    }

    pub(super) fn clear_assignee(&mut self, task_id: u64, cx: &mut Context<Self>) {
        let Some(manager) = &mut self.manager else {
            return;
        };
        let _ = manager.unassign(task_id);
        self.refresh_from_manager();
        cx.notify();
    }

    pub(super) fn move_to_list(&mut self, task_id: u64, list: &str, cx: &mut Context<Self>) {
        tracing::debug!(task_id, list, "todo: move to list");
        let Some(manager) = &mut self.manager else {
            return;
        };
        let _ = manager.move_to_list(task_id, list);
        self.refresh_from_manager();
        cx.notify();
    }

    pub(super) fn move_task_up(&mut self, task_id: u64, cx: &mut Context<Self>) {
        tracing::debug!(task_id, direction = "up", "todo: move task");
        let Some(manager) = &mut self.manager else {
            return;
        };
        // Find the task before this one in the same list
        let task = self.tasks.iter().find(|t| t.id == task_id);
        let list = task.map(|t| t.list.as_str()).unwrap_or("inbox");
        let list_tasks: Vec<u64> = self
            .tasks
            .iter()
            .filter(|t| t.list == list)
            .map(|t| t.id)
            .collect();
        if let Some(pos) = list_tasks.iter().position(|&id| id == task_id)
            && pos > 0
        {
            // Move after the task that's two positions before (or to top)
            let after_id = if pos >= 2 {
                Some(list_tasks[pos - 2])
            } else {
                None // Move to top
            };
            let _ = manager.move_task(task_id, Some(list), after_id);
            self.refresh_from_manager();
            cx.notify();
        }
    }

    pub(super) fn move_task_down(&mut self, task_id: u64, cx: &mut Context<Self>) {
        tracing::debug!(task_id, direction = "down", "todo: move task");
        let Some(manager) = &mut self.manager else {
            return;
        };
        let task = self.tasks.iter().find(|t| t.id == task_id);
        let list = task.map(|t| t.list.as_str()).unwrap_or("inbox");
        let list_tasks: Vec<u64> = self
            .tasks
            .iter()
            .filter(|t| t.list == list)
            .map(|t| t.id)
            .collect();
        if let Some(pos) = list_tasks.iter().position(|&id| id == task_id)
            && pos + 1 < list_tasks.len()
        {
            let after_id = Some(list_tasks[pos + 1]);
            let _ = manager.move_task(task_id, Some(list), after_id);
            self.refresh_from_manager();
            cx.notify();
        }
    }
}

fn format_timestamp_date(ts: i64) -> String {
    let days = ts / SECS_PER_DAY;
    let mut year = 1970i64;
    let mut remaining = days;
    loop {
        let dy = if is_leap(year) { 366 } else { 365 };
        if remaining < dy {
            break;
        }
        remaining -= dy;
        year += 1;
    }
    let month_days = [
        31,
        28 + i64::from(is_leap(year)),
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for &md in &month_days {
        if remaining < md {
            break;
        }
        remaining -= md;
        month += 1;
    }
    let day = remaining + 1;
    format!("{year:04}-{month:02}-{day:02}")
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

const SECS_PER_DAY: i64 = 86_400;

/// Parse a relative or absolute date string into a Unix timestamp.
fn parse_due_date(input: &str) -> Option<i64> {
    let input = input.trim().to_lowercase();
    let now = now_timestamp();

    // Round to end of today (UTC)
    let end_of_today = (now / SECS_PER_DAY + 1) * SECS_PER_DAY - 1;

    match input.as_str() {
        "today" => Some(end_of_today),
        "tomorrow" => Some(end_of_today + SECS_PER_DAY),
        "next week" => Some(end_of_today + 7 * SECS_PER_DAY),
        _ => {
            // Try YYYY-MM-DD: count days since epoch
            parse_date_str(&input)
        }
    }
}

/// Parse "YYYY-MM-DD" into a Unix timestamp (end of that day UTC).
fn parse_date_str(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: i64 = parts[0].parse().ok()?;
    let month: i64 = parts[1].parse().ok()?;
    let day: i64 = parts[2].parse().ok()?;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || year < 1970 {
        return None;
    }

    // Simple days-since-epoch calculation
    let mut days: i64 = 0;
    for y in 1970..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    let month_days = [
        31,
        28 + i64::from(is_leap(year)),
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    for d in &month_days[..(month - 1) as usize] {
        days += d;
    }
    days += day - 1;
    Some(days * SECS_PER_DAY + SECS_PER_DAY - 1) // end of day
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
