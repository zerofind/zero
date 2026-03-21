//! Tests for Task and TaskStatus

use super::task::{DEFAULT_LIST, Task, TaskStatus};
use foundation::util::now_timestamp;

#[test]
fn test_task_status_is_closed() {
    assert!(!TaskStatus::Open.is_closed());
    assert!(!TaskStatus::InProgress.is_closed());
    assert!(TaskStatus::Done.is_closed());
    assert!(TaskStatus::Cancelled.is_closed());
}

#[test]
fn test_task_status_is_open() {
    assert!(TaskStatus::Open.is_open());
    assert!(TaskStatus::InProgress.is_open());
    assert!(!TaskStatus::Done.is_open());
    assert!(!TaskStatus::Cancelled.is_open());
}

#[test]
fn test_task_status_from_str() {
    // Open variations
    assert_eq!(TaskStatus::parse_str("open"), Some(TaskStatus::Open));
    assert_eq!(TaskStatus::parse_str("o"), Some(TaskStatus::Open));
    assert_eq!(TaskStatus::parse_str("OPEN"), Some(TaskStatus::Open));

    // InProgress variations
    assert_eq!(
        TaskStatus::parse_str("inprogress"),
        Some(TaskStatus::InProgress)
    );
    assert_eq!(
        TaskStatus::parse_str("in_progress"),
        Some(TaskStatus::InProgress)
    );
    assert_eq!(
        TaskStatus::parse_str("in-progress"),
        Some(TaskStatus::InProgress)
    );
    assert_eq!(TaskStatus::parse_str("ip"), Some(TaskStatus::InProgress));
    assert_eq!(TaskStatus::parse_str("wip"), Some(TaskStatus::InProgress));

    // Done variations
    assert_eq!(TaskStatus::parse_str("done"), Some(TaskStatus::Done));
    assert_eq!(TaskStatus::parse_str("d"), Some(TaskStatus::Done));
    assert_eq!(TaskStatus::parse_str("completed"), Some(TaskStatus::Done));
    assert_eq!(TaskStatus::parse_str("complete"), Some(TaskStatus::Done));

    // Cancelled variations
    assert_eq!(
        TaskStatus::parse_str("cancelled"),
        Some(TaskStatus::Cancelled)
    );
    assert_eq!(
        TaskStatus::parse_str("canceled"),
        Some(TaskStatus::Cancelled)
    );
    assert_eq!(TaskStatus::parse_str("c"), Some(TaskStatus::Cancelled));

    // Invalid
    assert_eq!(TaskStatus::parse_str("invalid"), None);
    assert_eq!(TaskStatus::parse_str(""), None);
}

#[test]
fn test_task_status_as_str() {
    assert_eq!(TaskStatus::Open.as_str(), "open");
    assert_eq!(TaskStatus::InProgress.as_str(), "in_progress");
    assert_eq!(TaskStatus::Done.as_str(), "done");
    assert_eq!(TaskStatus::Cancelled.as_str(), "cancelled");
}

#[test]
fn test_task_status_display() {
    assert_eq!(format!("{}", TaskStatus::Open), "open");
    assert_eq!(format!("{}", TaskStatus::InProgress), "in_progress");
    assert_eq!(format!("{}", TaskStatus::Done), "done");
    assert_eq!(format!("{}", TaskStatus::Cancelled), "cancelled");
}

#[test]
fn test_task_new() {
    let task = Task::new("Buy groceries");

    assert_eq!(task.id, 0);
    assert_eq!(task.list, DEFAULT_LIST);
    assert_eq!(task.status, TaskStatus::Open);
    assert_eq!(task.text, "Buy groceries");
    assert!(task.tags.is_empty());
    assert!(task.assigned.is_none());
    assert!(task.due.is_none());
    assert!(task.completed.is_none());
    assert!(task.parent.is_none());
    assert_eq!(task.indent_level, 0);

    // Timestamps should be recent
    let now = now_timestamp();
    assert!(task.created >= now - 1);
    assert!(task.modified >= now - 1);
}

#[test]
fn test_task_builder_with_list() {
    let task = Task::new("Fix bug").with_list("bugs");

    assert_eq!(task.list, "bugs");
}

#[test]
fn test_task_default_list() {
    let task = Task::new("Test");

    assert_eq!(task.list, "inbox");
}

#[test]
fn test_task_builder_with_tag() {
    let task = Task::new("Test task").with_tag("urgent").with_tag("work");

    assert_eq!(task.tags.len(), 2);
    assert!(task.tags.contains(&"urgent".to_string()));
    assert!(task.tags.contains(&"work".to_string()));
}

#[test]
fn test_task_builder_with_tags() {
    let task = Task::new("Test task").with_tags(vec!["a", "b", "c"]);

    assert_eq!(task.tags.len(), 3);
    assert!(task.tags.contains(&"a".to_string()));
    assert!(task.tags.contains(&"b".to_string()));
    assert!(task.tags.contains(&"c".to_string()));
}

#[test]
fn test_task_builder_with_assigned() {
    let task = Task::new("Test task").with_assigned("alice");

    assert_eq!(task.assigned, Some("alice".to_string()));
}

#[test]
fn test_task_builder_with_due() {
    let due_time = now_timestamp() + 86400; // tomorrow
    let task = Task::new("Test task").with_due(due_time);

    assert_eq!(task.due, Some(due_time));
}

#[test]
fn test_task_builder_with_parent() {
    let task = Task::new("Subtask").with_parent(42, 1);

    assert_eq!(task.parent, Some(42));
    assert_eq!(task.indent_level, 1);
}

#[test]
fn test_task_builder_with_indent() {
    let task = Task::new("Test task").with_indent(2);

    assert_eq!(task.indent_level, 2);
}

#[test]
fn test_task_is_overdue() {
    // Task with no due date
    let task = Task::new("No due date");
    assert!(!task.is_overdue());

    // Task due in the future
    let future_task = Task::new("Future").with_due(now_timestamp() + 86400);
    assert!(!future_task.is_overdue());

    // Task due in the past
    let past_task = Task::new("Past").with_due(now_timestamp() - 86400);
    assert!(past_task.is_overdue());

    // Completed task is not overdue even if past due
    let mut completed_task = Task::new("Completed").with_due(now_timestamp() - 86400);
    completed_task.complete();
    assert!(!completed_task.is_overdue());
}

#[test]
fn test_task_is_due_today() {
    // Task with no due date
    let task = Task::new("No due date");
    assert!(!task.is_due_today());

    // Task due now (should be today)
    let now = now_timestamp();
    let today_task = Task::new("Today").with_due(now);
    assert!(today_task.is_due_today());

    // Task due yesterday
    let yesterday_task = Task::new("Yesterday").with_due(now - 86400);
    assert!(!yesterday_task.is_due_today());

    // Task due tomorrow
    let tomorrow_task = Task::new("Tomorrow").with_due(now + 86400);
    assert!(!tomorrow_task.is_due_today());
}

#[test]
fn test_task_touch() {
    let mut task = Task::new("Test");
    let original_modified = task.modified;

    // Wait a tiny bit and touch
    std::thread::sleep(std::time::Duration::from_millis(10));
    task.touch();

    // Modified should be updated (or at least not decreased)
    assert!(task.modified >= original_modified);
}

#[test]
fn test_task_complete() {
    let mut task = Task::new("Test");
    assert_eq!(task.status, TaskStatus::Open);
    assert!(task.completed.is_none());

    task.complete();

    assert_eq!(task.status, TaskStatus::Done);
    assert!(task.completed.is_some());
}

#[test]
fn test_task_cancel() {
    let mut task = Task::new("Test");

    task.cancel();

    assert_eq!(task.status, TaskStatus::Cancelled);
    assert!(task.completed.is_some());
}

#[test]
fn test_task_reopen() {
    let mut task = Task::new("Test");
    task.complete();

    assert_eq!(task.status, TaskStatus::Done);
    assert!(task.completed.is_some());

    task.reopen();

    assert_eq!(task.status, TaskStatus::Open);
    assert!(task.completed.is_none());
}

#[test]
fn test_task_has_tag() {
    let task = Task::new("Test").with_tag("Urgent").with_tag("Work");

    // Case-insensitive matching
    assert!(task.has_tag("urgent"));
    assert!(task.has_tag("URGENT"));
    assert!(task.has_tag("Urgent"));
    assert!(task.has_tag("work"));
    assert!(!task.has_tag("home"));
}

#[test]
fn test_task_validate_empty_text() {
    let task = Task {
        id: 0,
        list: DEFAULT_LIST.to_string(),
        status: TaskStatus::Open,
        text: "".to_string(),
        created: now_timestamp(),
        modified: now_timestamp(),
        tags: vec![],
        assigned: None,
        due: None,
        completed: None,
        parent: None,
        indent_level: 0,
    };

    assert_eq!(task.validate(), Some("Task text cannot be empty"));
}

#[test]
fn test_task_validate_whitespace_text() {
    let task = Task {
        id: 0,
        list: DEFAULT_LIST.to_string(),
        status: TaskStatus::Open,
        text: "   ".to_string(),
        created: now_timestamp(),
        modified: now_timestamp(),
        tags: vec![],
        assigned: None,
        due: None,
        completed: None,
        parent: None,
        indent_level: 0,
    };

    assert_eq!(task.validate(), Some("Task text cannot be empty"));
}

#[test]
fn test_task_validate_long_text() {
    let task = Task {
        id: 0,
        list: DEFAULT_LIST.to_string(),
        status: TaskStatus::Open,
        text: "x".repeat(10001),
        created: now_timestamp(),
        modified: now_timestamp(),
        tags: vec![],
        assigned: None,
        due: None,
        completed: None,
        parent: None,
        indent_level: 0,
    };

    assert_eq!(
        task.validate(),
        Some("Task text too long (max 10000 chars)")
    );
}

#[test]
fn test_task_validate_too_many_tags() {
    let task = Task {
        id: 0,
        list: DEFAULT_LIST.to_string(),
        status: TaskStatus::Open,
        text: "Valid text".to_string(),
        created: now_timestamp(),
        modified: now_timestamp(),
        tags: (0..101).map(|i| format!("tag{}", i)).collect(),
        assigned: None,
        due: None,
        completed: None,
        parent: None,
        indent_level: 0,
    };

    assert_eq!(task.validate(), Some("Too many tags (max 100)"));
}

#[test]
fn test_task_validate_deep_indent() {
    let task = Task {
        id: 0,
        list: DEFAULT_LIST.to_string(),
        status: TaskStatus::Open,
        text: "Valid text".to_string(),
        created: now_timestamp(),
        modified: now_timestamp(),
        tags: vec![],
        assigned: None,
        due: None,
        completed: None,
        parent: None,
        indent_level: 11,
    };

    assert_eq!(task.validate(), Some("Indent level too deep (max 10)"));
}

#[test]
fn test_task_validate_ok() {
    let task = Task::new("Valid task")
        .with_tag("test")
        .with_assigned("alice")
        .with_due(now_timestamp() + 86400)
        .with_indent(2);

    assert!(task.validate().is_none());
}
