//! Tests for TodoFile

use super::file::TodoFile;
use super::task::{DEFAULT_LIST, Task, TaskStatus};

#[test]
fn test_new_file() {
    let file = TodoFile::new();

    assert!(file.is_empty());
    assert_eq!(file.len(), 0);
    assert!(file.notes.is_none());
}

#[test]
fn test_add_task() {
    let mut file = TodoFile::new();

    let id = file.add_task(Task::new("Test task"));

    assert_eq!(id, 1);
    assert_eq!(file.len(), 1);

    let task = file.get_task(1).unwrap();
    assert_eq!(task.text, "Test task");
    assert_eq!(task.list, DEFAULT_LIST);
}

#[test]
fn test_add_task_to_list() {
    let mut file = TodoFile::new();

    let id = file.add_task(Task::new("Bug fix").with_list("bugs"));

    assert_eq!(id, 1);
    let task = file.get_task(1).unwrap();
    assert_eq!(task.list, "bugs");
}

#[test]
fn test_sequential_ids() {
    let mut file = TodoFile::new();

    let id1 = file.add_task(Task::new("Task 1"));
    let id2 = file.add_task(Task::new("Task 2"));
    let id3 = file.add_task(Task::new("Task 3"));

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(id3, 3);
}

#[test]
fn test_get_task() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Test"));

    assert!(file.get_task(1).is_some());
    assert!(file.get_task(999).is_none());
}

#[test]
fn test_get_task_mut() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Original"));

    let task = file.get_task_mut(1).unwrap();
    task.text = "Modified".to_string();

    assert_eq!(file.get_task(1).unwrap().text, "Modified");
}

#[test]
fn test_remove_task() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("To remove"));

    let removed = file.remove_task(1);

    assert!(removed.is_some());
    assert_eq!(removed.unwrap().text, "To remove");
    assert!(file.is_empty());
}

#[test]
fn test_remove_nonexistent() {
    let mut file = TodoFile::new();

    assert!(file.remove_task(999).is_none());
}

#[test]
fn test_complete_task() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("To complete"));

    assert!(file.complete_task(1));

    let task = file.get_task(1).unwrap();
    assert_eq!(task.status, TaskStatus::Done);
    assert!(task.completed.is_some());
}

#[test]
fn test_complete_nonexistent() {
    let mut file = TodoFile::new();

    assert!(!file.complete_task(999));
}

#[test]
fn test_reopen_task() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("To reopen"));
    file.complete_task(1);

    assert!(file.reopen_task(1));

    let task = file.get_task(1).unwrap();
    assert_eq!(task.status, TaskStatus::Open);
    assert!(task.completed.is_none());
}

#[test]
fn test_list_names() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Task 1").with_list("bugs"));
    file.add_task(Task::new("Task 2").with_list("features"));
    file.add_task(Task::new("Task 3").with_list("inbox"));
    file.add_task(Task::new("Task 4").with_list("bugs"));

    let names = file.list_names();

    // inbox should be first, rest sorted
    assert_eq!(names[0], "inbox");
    assert!(names.contains(&"bugs".to_string()));
    assert!(names.contains(&"features".to_string()));
    assert_eq!(names.len(), 3); // deduplicated
}

#[test]
fn test_tasks_in_list() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Bug 1").with_list("bugs"));
    file.add_task(Task::new("Feature 1").with_list("features"));
    file.add_task(Task::new("Bug 2").with_list("bugs"));

    let bugs = file.tasks_in_list("bugs");

    assert_eq!(bugs.len(), 2);
    assert!(bugs.iter().all(|t| t.list == "bugs"));
}

#[test]
fn test_open_tasks_in_list() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Open bug").with_list("bugs"));
    let id2 = file.add_task(Task::new("Closed bug").with_list("bugs"));
    file.complete_task(id2);
    file.add_task(Task::new("Another open").with_list("bugs"));

    let open_bugs = file.open_tasks_in_list("bugs");

    assert_eq!(open_bugs.len(), 2);
    assert!(open_bugs.iter().all(|t| t.status.is_open()));
}

#[test]
fn test_open_tasks() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Open 1"));
    let id2 = file.add_task(Task::new("Closed"));
    file.complete_task(id2);
    file.add_task(Task::new("Open 2"));

    let open: Vec<_> = file.open_tasks().collect();

    assert_eq!(open.len(), 2);
}

#[test]
fn test_closed_tasks() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Open"));
    let id2 = file.add_task(Task::new("Done"));
    file.complete_task(id2);

    let closed: Vec<_> = file.closed_tasks().collect();

    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].text, "Done");
}

#[test]
fn test_count_by_list() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Bug 1").with_list("bugs"));
    let id2 = file.add_task(Task::new("Bug 2 done").with_list("bugs"));
    file.complete_task(id2);
    file.add_task(Task::new("Feature").with_list("features"));

    let counts = file.count_by_list();

    let bugs = counts.get("bugs").unwrap();
    assert_eq!(bugs.total, 2);
    assert_eq!(bugs.open, 1);
    assert_eq!(bugs.closed, 1);

    let features = counts.get("features").unwrap();
    assert_eq!(features.total, 1);
    assert_eq!(features.open, 1);
}

#[test]
fn test_count_by_status() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Open 1"));
    file.add_task(Task::new("Open 2"));
    let id3 = file.add_task(Task::new("Done"));
    file.complete_task(id3);

    let counts = file.count_by_status();

    assert_eq!(counts.open, 2);
    assert_eq!(counts.done, 1);
    assert_eq!(counts.total(), 3);
    assert_eq!(counts.active(), 2);
    assert_eq!(counts.closed(), 1);
}

#[test]
fn test_all_tags() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Task 1").with_tag("urgent").with_tag("work"));
    file.add_task(Task::new("Task 2").with_tag("work"));
    file.add_task(Task::new("Task 3").with_tag("personal"));

    let tags = file.all_tags();

    assert_eq!(tags.len(), 3);
    assert!(tags.contains(&"urgent".to_string()));
    assert!(tags.contains(&"work".to_string()));
    assert!(tags.contains(&"personal".to_string()));
}

#[test]
fn test_all_assignees() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Task 1").with_assigned("alice"));
    file.add_task(Task::new("Task 2").with_assigned("bob"));
    file.add_task(Task::new("Task 3").with_assigned("alice"));
    file.add_task(Task::new("Task 4")); // no assignee

    let assignees = file.all_assignees();

    assert_eq!(assignees.len(), 2);
    assert!(assignees.contains(&"alice".to_string()));
    assert!(assignees.contains(&"bob".to_string()));
}

#[test]
fn test_empty_list_names() {
    let file = TodoFile::new();
    let names = file.list_names();

    assert!(names.is_empty());
}

#[test]
fn test_default() {
    let file = TodoFile::default();

    assert!(file.is_empty());
}

// =============================================================================
// Move task tests
// =============================================================================

#[test]
fn test_move_task_within_list_after() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Task 1").with_list("bugs"));
    file.add_task(Task::new("Task 2").with_list("bugs"));
    file.add_task(Task::new("Task 3").with_list("bugs"));

    // Move #3 after #1 (order: 1, 3, 2)
    assert!(file.move_task(3, None, Some(1)));

    let bugs = file.tasks_in_list("bugs");
    assert_eq!(bugs[0].id, 1);
    assert_eq!(bugs[1].id, 3);
    assert_eq!(bugs[2].id, 2);
}

#[test]
fn test_move_task_to_top() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Task 1").with_list("bugs"));
    file.add_task(Task::new("Task 2").with_list("bugs"));
    file.add_task(Task::new("Task 3").with_list("bugs"));

    // Move #3 to top (order: 3, 1, 2)
    assert!(file.move_task(3, None, None));

    let bugs = file.tasks_in_list("bugs");
    assert_eq!(bugs[0].id, 3);
    assert_eq!(bugs[1].id, 1);
    assert_eq!(bugs[2].id, 2);
}

#[test]
fn test_move_task_to_different_list() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Bug 1").with_list("bugs"));
    file.add_task(Task::new("Feature 1").with_list("features"));
    file.add_task(Task::new("Bug 2").with_list("bugs"));

    // Move bug #1 to features at top
    assert!(file.move_task(1, Some("features"), None));

    let bugs = file.tasks_in_list("bugs");
    assert_eq!(bugs.len(), 1);
    assert_eq!(bugs[0].id, 3);

    let features = file.tasks_in_list("features");
    assert_eq!(features.len(), 2);
    assert_eq!(features[0].id, 1);
    assert_eq!(features[0].list, "features"); // list updated
    assert_eq!(features[1].id, 2);
}

#[test]
fn test_move_task_to_different_list_after_specific() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Bug 1").with_list("bugs"));
    file.add_task(Task::new("Feature 1").with_list("features"));
    file.add_task(Task::new("Feature 2").with_list("features"));

    // Move bug #1 to features after #2
    assert!(file.move_task(1, Some("features"), Some(2)));

    let features = file.tasks_in_list("features");
    assert_eq!(features.len(), 3);
    assert_eq!(features[0].id, 2);
    assert_eq!(features[1].id, 1);
    assert_eq!(features[2].id, 3);
}

#[test]
fn test_move_nonexistent_task() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Task 1").with_list("bugs"));

    assert!(!file.move_task(999, None, None));
}

#[test]
fn test_move_after_nonexistent_goes_to_end() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Task 1").with_list("bugs"));
    file.add_task(Task::new("Task 2").with_list("bugs"));
    file.add_task(Task::new("Task 3").with_list("bugs"));

    // Move #1 after nonexistent #999 -> goes to end of list
    assert!(file.move_task(1, None, Some(999)));

    let bugs = file.tasks_in_list("bugs");
    assert_eq!(bugs[0].id, 2);
    assert_eq!(bugs[1].id, 3);
    assert_eq!(bugs[2].id, 1);
}

#[test]
fn test_move_to_new_list() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Task 1").with_list("bugs"));

    // Move to a list that doesn't exist yet
    assert!(file.move_task(1, Some("security"), None));

    assert!(file.tasks_in_list("bugs").is_empty());
    let security = file.tasks_in_list("security");
    assert_eq!(security.len(), 1);
    assert_eq!(security[0].list, "security");
}

#[test]
fn test_move_preserves_other_lists() {
    let mut file = TodoFile::new();
    file.add_task(Task::new("Bug 1").with_list("bugs"));
    file.add_task(Task::new("Feature 1").with_list("features"));
    file.add_task(Task::new("Bug 2").with_list("bugs"));
    file.add_task(Task::new("Feature 2").with_list("features"));

    // Reorder within bugs
    file.move_task(3, None, None); // move bug 2 to top

    // Features should be unchanged
    let features = file.tasks_in_list("features");
    assert_eq!(features.len(), 2);
    assert_eq!(features[0].id, 2);
    assert_eq!(features[1].id, 4);
}
