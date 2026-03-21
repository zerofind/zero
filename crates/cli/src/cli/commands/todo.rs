//! Todo command handler
//!
//! Commands for managing todos from the CLI.
//! Uses context-based workflow: `open` a file, then work within it.

use foundation::cmd_success;
use foundation::output::{Outputter, Response};
use todo::{Task, TaskStatus, TodoManager};

/// Execute todo subcommand
pub fn cmd_todo(out: &Outputter, cmd: Option<&TodoCommands>) -> anyhow::Result<()> {
    match cmd {
        None => cmd_status(out),
        Some(TodoCommands::Open { file }) => cmd_open(out, file),
        Some(TodoCommands::Close) => cmd_close(out),
        Some(TodoCommands::Add { text, list }) => cmd_add(out, text, list.as_deref()),
        Some(TodoCommands::Toggle { ids }) => cmd_toggle(out, ids),
        Some(TodoCommands::Remove { ids }) => cmd_remove(out, ids),
        Some(TodoCommands::Show { id }) => cmd_show(out, *id),
        Some(TodoCommands::Move { id, after, list }) => cmd_move(out, *id, after, list.as_deref()),
        Some(TodoCommands::Update {
            id,
            text,
            assigned,
            unassign,
            due,
            clear_due,
            status,
            list,
            add_tag,
            remove_tag,
        }) => cmd_update(
            out,
            *id,
            text.as_deref(),
            assigned.as_deref(),
            *unassign,
            due.as_deref(),
            *clear_due,
            status.as_deref(),
            list.as_deref(),
            add_tag,
            remove_tag,
        ),
        Some(TodoCommands::View(args)) => {
            cmd_view(out, args.first().map(std::string::String::as_str))
        }
    }
}

/// Todo subcommands
#[derive(Debug, clap::Subcommand)]
pub enum TodoCommands {
    /// Open a todo file (creates if needed)
    Open {
        /// Path to .todo file
        file: String,
    },

    /// Close current file (revert to default project.todo)
    Close,

    /// Add a task
    Add {
        /// Task description
        text: String,

        /// List name (e.g., bugs, features). Defaults to inbox.
        list: Option<String>,
    },

    /// Toggle task status (open ↔ done)
    Toggle {
        /// Task IDs to toggle
        ids: Vec<u64>,
    },

    /// Remove tasks
    Remove {
        /// Task IDs to remove
        ids: Vec<u64>,
    },

    /// Show task details
    Show {
        /// Task ID
        id: u64,
    },

    /// Move/reorder a task
    ///
    /// Examples:
    ///   zero todo move 3 1           # move #3 after #1
    ///   zero todo move 3 top         # move #3 to top of list
    ///   zero todo move 3 last        # move #3 to bottom
    ///   zero todo move 3 top --list inbox  # move to top of inbox
    Move {
        /// Task ID to move
        id: u64,

        /// Position: task ID to place after, "top", or "last"
        after: String,

        /// Move to different list (optional)
        #[arg(long)]
        list: Option<String>,
    },

    /// Update a task
    Update {
        /// Task ID
        id: u64,

        /// New text
        #[arg(long)]
        text: Option<String>,

        /// Assign to user or agent
        #[arg(long)]
        assigned: Option<String>,

        /// Remove assignee
        #[arg(long)]
        unassign: bool,

        /// Due date (unix timestamp or relative like "tomorrow")
        #[arg(long)]
        due: Option<String>,

        /// Clear due date
        #[arg(long)]
        clear_due: bool,

        /// Set status (open, `in_progress`, done, cancelled)
        #[arg(long)]
        status: Option<String>,

        /// Move to different list
        #[arg(long)]
        list: Option<String>,

        /// Add tag (can be specified multiple times)
        #[arg(long = "add-tag")]
        add_tag: Vec<String>,

        /// Remove tag (can be specified multiple times)
        #[arg(long = "remove-tag")]
        remove_tag: Vec<String>,
    },

    /// View tasks in a list (e.g., `zero todo bugs`)
    #[command(external_subcommand)]
    View(Vec<String>),
}

fn cmd_open(out: &Outputter, file: &str) -> anyhow::Result<()> {
    // Ensure .todo extension
    let file = if file.ends_with(".todo") {
        file.to_string()
    } else {
        format!("{file}.todo")
    };

    let manager = TodoManager::open_file(&file)?;
    let counts = manager.file().file.count_by_status();

    let data = serde_json::json!({
        "file": manager.filename(),
        "path": manager.path(),
        "tasks": counts.total(),
        "open": counts.active(),
    });
    cmd_success!(out, "todo.open", 0u64, data, {
        out.success(&format!(
            "Opened {} ({} tasks, {} open)",
            manager.filename(),
            counts.total(),
            counts.active()
        ));
    });
    Ok(())
}

fn cmd_close(out: &Outputter) -> anyhow::Result<()> {
    TodoManager::close()?;

    cmd_success!(out, "todo.close", 0u64, (), {
        out.success("Closed todo context (using project.todo)");
    });
    Ok(())
}

fn cmd_add(out: &Outputter, text: &str, list: Option<&str>) -> anyhow::Result<()> {
    let mut manager = TodoManager::open_current()?;
    let id = manager.add(text, list)?;
    let list_name = list.unwrap_or("inbox");

    let data = serde_json::json!({ "id": id, "list": list_name });
    cmd_success!(out, "todo.add", 0u64, data, {
        out.success(&format!("[{}] Added #{}: {}", manager.filename(), id, text));
    });
    Ok(())
}

fn cmd_toggle(out: &Outputter, ids: &[u64]) -> anyhow::Result<()> {
    let mut manager = TodoManager::open_current()?;
    let mut toggled = Vec::new();

    for &id in ids {
        if let Ok(task) = manager.get_task(id) {
            let was_open = task.status.is_open();
            let new_status = if was_open {
                TaskStatus::Done
            } else {
                TaskStatus::Open
            };

            if manager.set_status(id, new_status).is_ok() {
                toggled.push((id, was_open));
            }
        }
    }

    let data: Vec<_> = toggled
        .iter()
        .map(|(id, was_open)| {
            serde_json::json!({
                "id": id,
                "status": if *was_open { "done" } else { "open" }
            })
        })
        .collect();
    cmd_success!(out, "todo.toggle", 0u64, data, {
        for (id, was_open) in &toggled {
            if *was_open {
                out.success(&format!("#{id} → done"));
            } else {
                out.success(&format!("#{id} → open"));
            }
        }
        if toggled.len() < ids.len() {
            out.warn(&format!("{} task(s) not found", ids.len() - toggled.len()));
        }
    });
    Ok(())
}

fn cmd_remove(out: &Outputter, ids: &[u64]) -> anyhow::Result<()> {
    let mut manager = TodoManager::open_current()?;
    let mut removed = Vec::new();

    for &id in ids {
        if manager.remove(id).is_ok() {
            removed.push(id);
        }
    }

    cmd_success!(out, "todo.remove", 0u64, removed, {
        for id in &removed {
            out.success(&format!("Removed #{id}"));
        }
    });
    Ok(())
}

fn cmd_move(out: &Outputter, id: u64, after: &str, list: Option<&str>) -> anyhow::Result<()> {
    let mut manager = TodoManager::open_current()?;

    // Get current task's list for "last" handling
    let current_list = manager
        .get_task(id)
        .map_or_else(|_| "inbox".to_string(), |t| t.list.clone());

    // Parse "after" argument: task ID, "top", or "last"
    let after_id = match after.to_lowercase().as_str() {
        "top" => None,
        "last" => {
            // Find last task in target list
            let target_list = list.unwrap_or(current_list.as_str());
            let tasks = manager.tasks_in_list(target_list);
            tasks.last().map(|t| t.id)
        }
        s => Some(s.parse::<u64>().map_err(|_| {
            anyhow::anyhow!("Invalid position '{s}'. Use task ID, 'top', or 'last'")
        })?),
    };

    manager.move_task(id, list, after_id)?;

    let data = serde_json::json!({ "id": id, "after": after, "list": list });
    cmd_success!(out, "todo.move", 0u64, data, {
        match (list, after_id) {
            (Some(l), None) => out.success(&format!("Moved #{id} to top of {l}")),
            (Some(l), Some(aid)) => out.success(&format!("Moved #{id} after #{aid} in {l}")),
            (None, None) => out.success(&format!("Moved #{id} to top")),
            (None, Some(aid)) => out.success(&format!("Moved #{id} after #{aid}")),
        }
    });
    Ok(())
}

fn cmd_show(out: &Outputter, id: u64) -> anyhow::Result<()> {
    let manager = TodoManager::open_current()?;
    let task = manager.get_task(id)?;

    cmd_success!(out, "todo.show", 0u64, task_to_json(&task), {
        out.header(&format!("Task #{id}"));
        out.indented(&format!("Text: {}", task.text));
        out.indented(&format!("Status: {}", task.status));
        out.indented(&format!("List: {}", task.list));
        if !task.tags.is_empty() {
            out.indented(&format!("Tags: {}", task.tags.join(", ")));
        }
        if let Some(ref assigned) = task.assigned {
            out.indented(&format!("Assigned: {assigned}"));
        }
        if let Some(due) = task.due {
            out.indented(&format!("Due: {}", format_timestamp(due)));
        }
        out.indented(&format!("Created: {}", format_timestamp(task.created)));
        out.indented(&format!("Modified: {}", format_timestamp(task.modified)));
        if let Some(completed) = task.completed {
            out.indented(&format!("Completed: {}", format_timestamp(completed)));
        }
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_update(
    out: &Outputter,
    id: u64,
    text: Option<&str>,
    assigned: Option<&str>,
    unassign: bool,
    due: Option<&str>,
    clear_due: bool,
    status: Option<&str>,
    list: Option<&str>,
    add_tags: &[String],
    remove_tags: &[String],
) -> anyhow::Result<()> {
    let mut manager = TodoManager::open_current()?;
    let mut changed = false;

    if let Some(t) = text {
        manager.update_text(id, t)?;
        changed = true;
    }

    if unassign {
        manager.unassign(id)?;
        changed = true;
    } else if let Some(a) = assigned {
        manager.assign(id, a)?;
        changed = true;
    }

    if clear_due {
        manager.set_due(id, None)?;
        changed = true;
    } else if let Some(d) = due
        && let Some(ts) = parse_due_date(d)
    {
        manager.set_due(id, Some(ts))?;
        changed = true;
    }

    if let Some(s) = status
        && let Some(status) = TaskStatus::parse_str(s)
    {
        manager.set_status(id, status)?;
        changed = true;
    }

    if let Some(l) = list {
        manager.move_to_list(id, l)?;
        changed = true;
    }

    for tag in add_tags {
        manager.add_tag(id, tag)?;
        changed = true;
    }

    for tag in remove_tags {
        manager.remove_tag(id, tag)?;
        changed = true;
    }

    if out.is_json() {
        let response: Response<()> = Response::success("todo.update", 0, ());
        out.result(&response);
    } else if changed {
        out.success(&format!("Updated task #{id}"));
    } else {
        out.info("No changes specified");
    }

    Ok(())
}

fn cmd_status(out: &Outputter) -> anyhow::Result<()> {
    let manager = TodoManager::open_current()?;
    let state = manager.file();
    let counts_by_list = state.file.count_by_list();
    let list_names = state.file.list_names();

    let all_tasks = manager.all_tasks();
    let data = serde_json::json!({
        "file": manager.filename(),
        "path": manager.path(),
        "tasks": all_tasks.iter().map(task_to_json).collect::<Vec<_>>(),
    });
    cmd_success!(out, "todo.status", 0u64, data, {
        out.header(manager.filename());
        out.newline();
        if list_names.is_empty() {
            out.info("No tasks");
            return Ok(());
        }
        for list_name in &list_names {
            let counts = counts_by_list.get(list_name).copied().unwrap_or_default();
            out.info(&format!("{} ({})", list_name, counts.open));
            for task in &manager.open_tasks_in_list(list_name) {
                print_task(out, task);
            }
            out.newline();
        }
    });
    Ok(())
}

fn cmd_view(out: &Outputter, list: Option<&str>) -> anyhow::Result<()> {
    let manager = TodoManager::open_current()?;

    let tasks: Vec<_> = if let Some(l) = list {
        manager.tasks_in_list(l).iter().map(task_to_json).collect()
    } else {
        manager.all_tasks().iter().map(task_to_json).collect()
    };
    cmd_success!(out, "todo.view", 0u64, tasks, {
        out.header(manager.filename());
        out.newline();
        if let Some(l) = list {
            let open_tasks = manager.open_tasks_in_list(l);
            if open_tasks.is_empty() {
                out.info(&format!("No open tasks in '{l}'"));
            } else {
                out.info(&format!("{} ({})", l, open_tasks.len()));
                for task in &open_tasks {
                    print_task(out, task);
                }
            }
        } else {
            let list_names = manager.list_names();
            let state = manager.file();
            let counts_by_list = state.file.count_by_list();
            drop(state);
            for list_name in &list_names {
                let counts = counts_by_list.get(list_name).copied().unwrap_or_default();
                out.info(&format!("{} ({})", list_name, counts.open));
                for task in &manager.open_tasks_in_list(list_name) {
                    print_task(out, task);
                }
                out.newline();
            }
        }
    });
    Ok(())
}

// =============================================================================
// Helpers
// =============================================================================

fn task_to_json(task: &Task) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    map.insert("id".into(), task.id.into());
    map.insert("list".into(), task.list.clone().into());
    map.insert("text".into(), task.text.clone().into());
    map.insert("status".into(), task.status.as_str().into());
    map.insert("tags".into(), task.tags.clone().into());
    map.insert("created".into(), task.created.into());
    map.insert("modified".into(), task.modified.into());

    if let Some(ref assigned) = task.assigned {
        map.insert("assigned".into(), assigned.clone().into());
    }
    if let Some(due) = task.due {
        map.insert("due".into(), due.into());
    }
    if let Some(completed) = task.completed {
        map.insert("completed".into(), completed.into());
    }
    if let Some(parent) = task.parent {
        map.insert("parent".into(), parent.into());
    }

    map
}

fn print_task(out: &Outputter, task: &Task) {
    let status_icon = match task.status {
        TaskStatus::Open => "○",
        TaskStatus::InProgress => "◐",
        TaskStatus::Done => "●",
        TaskStatus::Cancelled => "✕",
    };

    let mut line = format!("  {} #{} {}", status_icon, task.id, task.text);

    if !task.tags.is_empty() {
        line.push_str(&format!(" [{}]", task.tags.join(", ")));
    }

    if let Some(ref assigned) = task.assigned {
        line.push_str(&format!(" @{assigned}"));
    }

    if task.is_overdue() {
        line.push_str(" (overdue)");
    } else if task.is_due_today() {
        line.push_str(" (today)");
    }

    out.info(&line);
}

fn parse_due_date(s: &str) -> Option<i64> {
    use foundation::util::now_timestamp;

    let now = now_timestamp();
    let day = 86400;

    match s.to_lowercase().as_str() {
        "today" => Some(now - (now % day) + day - 1),
        "tomorrow" => Some(now - (now % day) + 2 * day - 1),
        "week" | "next week" => Some(now + 7 * day),
        _ => s.parse::<i64>().ok(),
    }
}

fn format_timestamp(ts: i64) -> String {
    let secs_per_day = 86400;
    let days_since_epoch = ts / secs_per_day;
    let secs_today = ts % secs_per_day;

    let hours = secs_today / 3600;
    let mins = (secs_today % 3600) / 60;

    let mut year = 1970;
    let mut remaining_days = days_since_epoch;

    loop {
        let days_in_year = if year % 4 == 0 { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let months = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1;
    for days_in_month in months {
        let dim = if month == 2 && year % 4 == 0 {
            29
        } else {
            days_in_month
        };
        if remaining_days < dim {
            break;
        }
        remaining_days -= dim;
        month += 1;
    }

    let day = remaining_days + 1;

    format!("{year}-{month:02}-{day:02} {hours:02}:{mins:02}")
}
