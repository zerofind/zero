//! Tests for TodoManager operations

use std::path::PathBuf;

use tempfile::TempDir;

use super::manager::TodoManager;
use super::storage::clear_context;
use super::task::{DEFAULT_LIST, Task, TaskStatus};

fn setup() -> (TempDir, PathBuf) {
    let _ = clear_context();
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.todo");
    (dir, path)
}

#[test]
fn test_open_file_creates_new() {
    let (_dir, path) = setup();

    assert!(!path.exists());

    let manager = TodoManager::open_file(&path).unwrap();

    assert!(path.exists());
    assert!(manager.file().file.is_empty());
}

#[test]
fn test_open_file_loads_existing() {
    let (_dir, path) = setup();

    // Create file with a task
    {
        let mut manager = TodoManager::open_file(&path).unwrap();
        manager.add("Existing task", None).unwrap();
    }

    // Reopen
    let manager = TodoManager::open_file(&path).unwrap();

    assert_eq!(manager.file().file.len(), 1);
    assert_eq!(manager.get_task(1).unwrap().text, "Existing task");
}

#[test]
fn test_filename() {
    let (_dir, path) = setup();
    let manager = TodoManager::open_file(&path).unwrap();

    assert_eq!(manager.filename(), "test.todo");
}

#[test]
fn test_add_task() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager.add("Buy groceries", None).unwrap();

    assert_eq!(id, 1);
    let task = manager.get_task(1).unwrap();
    assert_eq!(task.text, "Buy groceries");
    assert_eq!(task.list, DEFAULT_LIST);
}

#[test]
fn test_add_task_to_list() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager.add("Fix crash", Some("bugs")).unwrap();

    let task = manager.get_task(id).unwrap();
    assert_eq!(task.list, "bugs");
}

#[test]
fn test_add_task_with_full_builder() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let task = Task::new("Complex task")
        .with_list("features")
        .with_tag("urgent")
        .with_assigned("alice")
        .with_due(1700000000);

    let id = manager.add_task(task).unwrap();

    let loaded = manager.get_task(id).unwrap();
    assert_eq!(loaded.text, "Complex task");
    assert_eq!(loaded.list, "features");
    assert!(loaded.has_tag("urgent"));
    assert_eq!(loaded.assigned, Some("alice".to_string()));
    assert_eq!(loaded.due, Some(1700000000));
}

#[test]
fn test_add_task_validation_error() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let task = Task {
        id: 0,
        list: DEFAULT_LIST.to_string(),
        status: TaskStatus::Open,
        text: String::new(), // Invalid
        created: 0,
        modified: 0,
        tags: vec![],
        assigned: None,
        due: None,
        completed: None,
        parent: None,
        indent_level: 0,
    };

    let result = manager.add_task(task);
    assert!(result.is_err());
}

#[test]
fn test_sequential_ids() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id1 = manager.add("Task 1", None).unwrap();
    let id2 = manager.add("Task 2", None).unwrap();
    let id3 = manager.add("Task 3", None).unwrap();

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(id3, 3);
}

#[test]
fn test_get_task() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();
    manager.add("Test", None).unwrap();

    assert!(manager.get_task(1).is_ok());
    assert!(manager.get_task(999).is_err());
}

#[test]
fn test_update_text() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager.add("Original", None).unwrap();
    manager.update_text(id, "Updated").unwrap();

    assert_eq!(manager.get_task(id).unwrap().text, "Updated");
}

#[test]
fn test_complete() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager.add("To complete", None).unwrap();
    manager.complete(id).unwrap();

    let task = manager.get_task(id).unwrap();
    assert_eq!(task.status, TaskStatus::Done);
    assert!(task.completed.is_some());
}

#[test]
fn test_complete_many() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id1 = manager.add("Task 1", None).unwrap();
    let id2 = manager.add("Task 2", None).unwrap();
    let id3 = manager.add("Task 3", None).unwrap();

    let completed = manager.complete_many(&[id1, id3]).unwrap();

    assert_eq!(completed.len(), 2);
    assert!(completed.contains(&id1));
    assert!(completed.contains(&id3));

    assert_eq!(manager.get_task(id1).unwrap().status, TaskStatus::Done);
    assert_eq!(manager.get_task(id2).unwrap().status, TaskStatus::Open);
    assert_eq!(manager.get_task(id3).unwrap().status, TaskStatus::Done);
}

#[test]
fn test_reopen() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager.add("To reopen", None).unwrap();
    manager.complete(id).unwrap();
    manager.reopen(id).unwrap();

    let task = manager.get_task(id).unwrap();
    assert_eq!(task.status, TaskStatus::Open);
    assert!(task.completed.is_none());
}

#[test]
fn test_remove() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager.add("To remove", None).unwrap();
    let removed = manager.remove(id).unwrap();

    assert_eq!(removed.text, "To remove");
    assert!(manager.file().file.is_empty());
}

#[test]
fn test_assign() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager.add("To assign", None).unwrap();
    manager.assign(id, "alice").unwrap();

    assert_eq!(
        manager.get_task(id).unwrap().assigned,
        Some("alice".to_string())
    );
}

#[test]
fn test_unassign() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager
        .add_task(Task::new("Assigned").with_assigned("bob"))
        .unwrap();
    manager.unassign(id).unwrap();

    assert!(manager.get_task(id).unwrap().assigned.is_none());
}

#[test]
fn test_set_due() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager.add("With due", None).unwrap();
    manager.set_due(id, Some(1700000000)).unwrap();

    assert_eq!(manager.get_task(id).unwrap().due, Some(1700000000));

    // Clear due
    manager.set_due(id, None).unwrap();
    assert!(manager.get_task(id).unwrap().due.is_none());
}

#[test]
fn test_add_tag() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager.add("Tagged", None).unwrap();
    manager.add_tag(id, "urgent").unwrap();
    manager.add_tag(id, "work").unwrap();

    let task = manager.get_task(id).unwrap();
    assert!(task.has_tag("urgent"));
    assert!(task.has_tag("work"));
}

#[test]
fn test_add_tag_duplicate_ignored() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager.add("Tagged", None).unwrap();
    manager.add_tag(id, "urgent").unwrap();
    manager.add_tag(id, "urgent").unwrap(); // Duplicate

    assert_eq!(manager.get_task(id).unwrap().tags.len(), 1);
}

#[test]
fn test_remove_tag() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager
        .add_task(Task::new("Tagged").with_tag("urgent"))
        .unwrap();
    manager.remove_tag(id, "urgent").unwrap();

    assert!(!manager.get_task(id).unwrap().has_tag("urgent"));
}

#[test]
fn test_move_to_list() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager.add("Task", Some("inbox")).unwrap();
    manager.move_to_list(id, "bugs").unwrap();

    assert_eq!(manager.get_task(id).unwrap().list, "bugs");
}

#[test]
fn test_set_status() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    let id = manager.add("Task", None).unwrap();
    manager.set_status(id, TaskStatus::InProgress).unwrap();

    assert_eq!(manager.get_task(id).unwrap().status, TaskStatus::InProgress);
}

#[test]
fn test_list_names() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    manager.add("Task 1", Some("bugs")).unwrap();
    manager.add("Task 2", Some("features")).unwrap();
    manager.add("Task 3", Some("inbox")).unwrap();
    manager.add("Task 4", Some("bugs")).unwrap();

    let names = manager.list_names();

    // inbox should be first
    assert_eq!(names[0], "inbox");
    assert!(names.contains(&"bugs".to_string()));
    assert!(names.contains(&"features".to_string()));
    assert_eq!(names.len(), 3);
}

#[test]
fn test_tasks_in_list() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    manager.add("Bug 1", Some("bugs")).unwrap();
    manager.add("Feature 1", Some("features")).unwrap();
    manager.add("Bug 2", Some("bugs")).unwrap();

    let bugs = manager.tasks_in_list("bugs");

    assert_eq!(bugs.len(), 2);
    assert!(bugs.iter().all(|t| t.list == "bugs"));
}

#[test]
fn test_open_tasks_in_list() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    manager.add("Open bug", Some("bugs")).unwrap();
    let id2 = manager.add("Done bug", Some("bugs")).unwrap();
    manager.complete(id2).unwrap();
    manager.add("Another open", Some("bugs")).unwrap();

    let open_bugs = manager.open_tasks_in_list("bugs");

    assert_eq!(open_bugs.len(), 2);
    assert!(open_bugs.iter().all(|t| t.status.is_open()));
}

#[test]
fn test_open_tasks() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    manager.add("Open 1", None).unwrap();
    let id2 = manager.add("Done", None).unwrap();
    manager.complete(id2).unwrap();
    manager.add("Open 2", None).unwrap();

    let open = manager.open_tasks();

    assert_eq!(open.len(), 2);
}

#[test]
fn test_all_tasks() {
    let (_dir, path) = setup();
    let mut manager = TodoManager::open_file(&path).unwrap();

    manager.add("Task 1", None).unwrap();
    manager.add("Task 2", None).unwrap();

    assert_eq!(manager.all_tasks().len(), 2);
}

#[test]
fn test_persistence() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("persist.todo");
    let _ = clear_context();

    // Create and populate
    {
        let mut manager = TodoManager::open_file(&path).unwrap();
        manager.add("Persistent task", Some("bugs")).unwrap();
        manager.add_tag(1, "important").unwrap();
    }

    // Reopen and verify
    {
        let manager = TodoManager::open_file(&path).unwrap();

        assert_eq!(manager.file().file.len(), 1);
        let task = manager.get_task(1).unwrap();
        assert_eq!(task.text, "Persistent task");
        assert_eq!(task.list, "bugs");
        assert!(task.has_tag("important"));
    }
}

#[test]
#[ignore = "uses global context state — run with: cargo test test_context_operations -- --ignored"]
fn test_context_operations() {
    // This test covers context save, restore, and close in sequence
    // to avoid parallel test interference with global context state
    let _ = clear_context();

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("context_test.todo");

    // 1. Open file (saves context)
    {
        let mut manager = TodoManager::open_file(&path).unwrap();
        manager.add("Test", None).unwrap();
    }

    // 2. Verify context was saved
    let ctx = TodoManager::current_context();
    assert!(ctx.is_some());
    assert!(ctx.unwrap().path.ends_with("context_test.todo"));

    // 3. Open current should use saved context
    {
        let manager = TodoManager::open_current().unwrap();
        assert_eq!(manager.filename(), "context_test.todo");
        assert_eq!(manager.file().file.len(), 1);
    }

    // 4. Close context
    TodoManager::close().unwrap();

    // 5. Context should be cleared
    assert!(TodoManager::current_context().is_none());
}
