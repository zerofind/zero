//! Tests for storage operations (etch-backed)

use std::path::Path;

use tempfile::TempDir;

use super::storage::{
    DEFAULT_FILENAME, TodoContext, clear_context, load_context, normalize_path, open_store,
    resolve_todo_path, save_context,
};
use super::task::Task;

#[test]
fn test_open_store_creates_dir() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.todo");

    assert!(!path.exists());

    let store = open_store(&path).unwrap();
    let state = store.read();

    assert!(path.exists());
    assert!(state.file.is_empty());
}

#[test]
fn test_store_roundtrip_with_tasks() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("project.todo");

    {
        let store = open_store(&path).unwrap();
        store
            .write(|tx| {
                let mut t1 = Task::new("Fix bug").with_list("bugs").with_tag("urgent");
                t1.id = 1;
                let mut t2 = Task::new("Add feature").with_list("features");
                t2.id = 2;
                let mut t3 = Task::new("Review docs")
                    .with_list("inbox")
                    .with_assigned("alice");
                t3.id = 3;
                tx.put_task(&t1);
                tx.put_task(&t2);
                tx.put_task(&t3);
                Ok(())
            })
            .unwrap();
    }

    // Reopen from disk
    {
        let store = open_store(&path).unwrap();
        let state = store.read();

        assert_eq!(state.file.len(), 3);
        assert_eq!(state.file.get_task(1).unwrap().text, "Fix bug");
        assert_eq!(state.file.get_task(1).unwrap().list, "bugs");
        assert!(state.file.get_task(1).unwrap().has_tag("urgent"));
        assert_eq!(state.file.get_task(2).unwrap().text, "Add feature");
        assert_eq!(
            state.file.get_task(3).unwrap().assigned,
            Some("alice".to_string())
        );
    }
}

#[test]
fn test_store_creates_parent_directories() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested").join("dirs").join("test.todo");

    let store = open_store(&path).unwrap();
    let state = store.read();
    assert!(state.file.is_empty());
    assert!(path.exists());
}

#[test]
fn test_context_save_and_load() {
    let _ = clear_context();

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.todo");

    // Create the directory so context validation passes
    std::fs::create_dir_all(&path).unwrap();

    let ctx = TodoContext { path: path.clone() };
    save_context(&ctx).unwrap();

    let loaded = load_context();
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().path, path);

    // Clean up
    clear_context().unwrap();
}

#[test]
fn test_context_clear() {
    let _ = clear_context();

    // Should succeed even if no context exists
    clear_context().unwrap();
    assert!(load_context().is_none());
}

#[test]
fn test_context_invalid_file_cleared() {
    let _ = clear_context();

    // Save context pointing to non-existent directory
    let ctx = TodoContext {
        path: "/nonexistent/file.todo".into(),
    };
    save_context(&ctx).unwrap();

    // Loading should return None and clear the invalid context
    let loaded = load_context();
    assert!(loaded.is_none());
}

#[test]
fn test_resolve_todo_path_default() {
    let _ = clear_context();

    let path = resolve_todo_path().unwrap();

    // Should resolve to project.todo in current directory
    assert!(path.ends_with(DEFAULT_FILENAME));
}

#[test]
fn test_normalize_path_relative() {
    let normalized = normalize_path(Path::new("test.todo")).unwrap();

    assert!(normalized.is_absolute());
    assert!(normalized.ends_with("test.todo"));
}

#[test]
fn test_normalize_path_absolute() {
    let absolute = Path::new("/tmp/test.todo");
    let normalized = normalize_path(absolute).unwrap();

    assert_eq!(normalized, absolute);
}

#[test]
fn test_preserve_task_data_roundtrip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("roundtrip.todo");

    {
        let store = open_store(&path).unwrap();
        store
            .write(|tx| {
                let mut task = Task::new("Complex task")
                    .with_list("bugs")
                    .with_tag("urgent")
                    .with_tag("work")
                    .with_assigned("alice")
                    .with_due(1700000000)
                    .with_parent(0, 1);
                task.id = 1;
                task.created = 1699900000;
                task.modified = 1699950000;

                let mut task2 = Task::new("Done task").with_list("features");
                task2.id = 2;
                task2.complete();

                tx.put_task(&task);
                tx.put_task(&task2);
                Ok(())
            })
            .unwrap();
    }

    {
        let store = open_store(&path).unwrap();
        let state = store.read();

        let t1 = state.file.get_task(1).unwrap();
        assert_eq!(t1.list, "bugs");
        assert_eq!(t1.text, "Complex task");
        assert_eq!(t1.tags, vec!["urgent", "work"]);
        assert_eq!(t1.assigned, Some("alice".to_string()));
        assert_eq!(t1.due, Some(1700000000));
        assert_eq!(t1.parent, Some(0));
        assert_eq!(t1.indent_level, 1);
        assert_eq!(t1.created, 1699900000);
        assert_eq!(t1.modified, 1699950000);

        let t2 = state.file.get_task(2).unwrap();
        assert_eq!(t2.list, "features");
        assert!(t2.status.is_closed());
        assert!(t2.completed.is_some());
    }
}

#[test]
fn test_multiple_lists_in_store() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("multi.todo");

    let store = open_store(&path).unwrap();
    store
        .write(|tx| {
            let mut t1 = Task::new("Inbox 1").with_list("inbox");
            t1.id = 1;
            let mut t2 = Task::new("Bug 1").with_list("bugs");
            t2.id = 2;
            let mut t3 = Task::new("Bug 2").with_list("bugs");
            t3.id = 3;
            let mut t4 = Task::new("Feature 1").with_list("features");
            t4.id = 4;
            let mut t5 = Task::new("Inbox 2").with_list("inbox");
            t5.id = 5;
            tx.put_task(&t1);
            tx.put_task(&t2);
            tx.put_task(&t3);
            tx.put_task(&t4);
            tx.put_task(&t5);
            Ok(())
        })
        .unwrap();

    let state = store.read();
    assert_eq!(state.file.len(), 5);

    let list_names = state.file.list_names();
    assert_eq!(list_names.len(), 3);
    assert!(list_names.contains(&"inbox".to_string()));
    assert!(list_names.contains(&"bugs".to_string()));
    assert!(list_names.contains(&"features".to_string()));

    assert_eq!(state.file.tasks_in_list("bugs").len(), 2);
    assert_eq!(state.file.tasks_in_list("inbox").len(), 2);
    assert_eq!(state.file.tasks_in_list("features").len(), 1);
}
