//! Tests for todo search functionality

use std::path::PathBuf;

use tempfile::TempDir;

use super::search::{TodoSearchOptions, find_todo_files, search_todos};
use super::storage::open_store;
use super::task::Task;

fn create_test_todo_dir(dir: &TempDir, name: &str, tasks: Vec<Task>) -> PathBuf {
    let path = dir.path().join(name);
    let store = open_store(&path).unwrap();
    store
        .write(|tx| {
            for (i, mut task) in tasks.into_iter().enumerate() {
                task.id = (i + 1) as u64;
                tx.put_task(&task);
            }
            Ok(())
        })
        .unwrap();
    path
}

#[test]
fn test_search_by_text() {
    let dir = TempDir::new().unwrap();

    let path = create_test_todo_dir(
        &dir,
        "project.todo",
        vec![
            Task::new("Fix crash on startup"),
            Task::new("Add dark mode"),
            Task::new("Fix login bug"),
        ],
    );

    let results = search_todos(&[path], &TodoSearchOptions::with_query("fix"));

    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|r| r.task.text.contains("crash")));
    assert!(results.iter().any(|r| r.task.text.contains("login")));
}

#[test]
fn test_search_case_insensitive() {
    let dir = TempDir::new().unwrap();

    let path = create_test_todo_dir(
        &dir,
        "project.todo",
        vec![Task::new("FIX URGENT BUG"), Task::new("fix small issue")],
    );

    let results = search_todos(&[path], &TodoSearchOptions::with_query("FIX"));

    assert_eq!(results.len(), 2);
}

#[test]
fn test_search_by_tag() {
    let dir = TempDir::new().unwrap();

    let path = create_test_todo_dir(
        &dir,
        "project.todo",
        vec![
            Task::new("Urgent task").with_tag("urgent"),
            Task::new("Normal task"),
            Task::new("Another urgent")
                .with_tag("urgent")
                .with_tag("work"),
        ],
    );

    let options = TodoSearchOptions::default().with_tag("urgent");
    let results = search_todos(&[path], &options);

    assert_eq!(results.len(), 2);
    assert!(
        results
            .iter()
            .all(|r| r.task.tags.contains(&"urgent".to_string()))
    );
}

#[test]
fn test_search_by_status() {
    let dir = TempDir::new().unwrap();

    let task1 = Task::new("Open task");
    let mut task2 = Task::new("Done task");
    task2.complete();
    let task3 = Task::new("Another open");

    let path = create_test_todo_dir(&dir, "project.todo", vec![task1, task2, task3]);

    let options = TodoSearchOptions::default().with_status("open");
    let results = search_todos(&[path], &options);

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.task.status == "open"));
}

#[test]
fn test_search_by_list() {
    let dir = TempDir::new().unwrap();

    let path = create_test_todo_dir(
        &dir,
        "project.todo",
        vec![
            Task::new("Bug 1").with_list("bugs"),
            Task::new("Feature 1").with_list("features"),
            Task::new("Bug 2").with_list("bugs"),
        ],
    );

    let options = TodoSearchOptions::default().with_list("bugs");
    let results = search_todos(&[path], &options);

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.task.list == "bugs"));
}

#[test]
fn test_search_by_assignee() {
    let dir = TempDir::new().unwrap();

    let path = create_test_todo_dir(
        &dir,
        "project.todo",
        vec![
            Task::new("Alice task").with_assigned("alice"),
            Task::new("Bob task").with_assigned("bob"),
            Task::new("Unassigned task"),
        ],
    );

    let options = TodoSearchOptions::default().with_assigned("alice");
    let results = search_todos(&[path], &options);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].task.assigned, Some("alice".to_string()));
}

#[test]
fn test_search_combined_filters() {
    let dir = TempDir::new().unwrap();

    let path = create_test_todo_dir(
        &dir,
        "project.todo",
        vec![
            Task::new("Fix crash").with_list("bugs").with_tag("urgent"),
            Task::new("Fix typo").with_list("bugs"),
            Task::new("Add feature")
                .with_list("features")
                .with_tag("urgent"),
            Task::new("Fix login").with_list("bugs").with_tag("urgent"),
        ],
    );

    let options = TodoSearchOptions::with_query("fix")
        .with_list("bugs")
        .with_tag("urgent");
    let results = search_todos(&[path], &options);

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.task.list == "bugs"));
    assert!(
        results
            .iter()
            .all(|r| r.task.tags.contains(&"urgent".to_string()))
    );
}

#[test]
fn test_search_across_multiple_files() {
    let dir = TempDir::new().unwrap();

    let path1 = create_test_todo_dir(&dir, "project.todo", vec![Task::new("Fix bug in project")]);

    let path2 = create_test_todo_dir(&dir, "security.todo", vec![Task::new("Fix security bug")]);

    let results = search_todos(&[path1, path2], &TodoSearchOptions::with_query("fix"));

    assert_eq!(results.len(), 2);

    let files: Vec<&str> = results.iter().map(|r| r.file_name.as_str()).collect();
    assert!(files.contains(&"project.todo"));
    assert!(files.contains(&"security.todo"));
}

#[test]
fn test_search_with_limit() {
    let dir = TempDir::new().unwrap();

    let tasks: Vec<Task> = (0..20).map(|i| Task::new(format!("Task {i}"))).collect();
    let path = create_test_todo_dir(&dir, "project.todo", tasks);

    let options = TodoSearchOptions::default().with_limit(5);
    let results = search_todos(&[path], &options);

    assert_eq!(results.len(), 5);
}

#[test]
fn test_search_empty_query_returns_all() {
    let dir = TempDir::new().unwrap();

    let path = create_test_todo_dir(
        &dir,
        "project.todo",
        vec![
            Task::new("Task 1"),
            Task::new("Task 2"),
            Task::new("Task 3"),
        ],
    );

    let options = TodoSearchOptions::default();
    let results = search_todos(&[path], &options);

    assert_eq!(results.len(), 3);
}

#[test]
fn test_search_no_matches() {
    let dir = TempDir::new().unwrap();

    let path = create_test_todo_dir(
        &dir,
        "project.todo",
        vec![Task::new("Fix bug"), Task::new("Add feature")],
    );

    let results = search_todos(&[path], &TodoSearchOptions::with_query("nonexistent"));

    assert!(results.is_empty());
}

#[test]
fn test_search_score_ordering() {
    let dir = TempDir::new().unwrap();

    let path = create_test_todo_dir(
        &dir,
        "project.todo",
        vec![
            Task::new("Contains fix somewhere"),
            Task::new("Fix at start should rank higher"),
            Task::new("Another fix in middle"),
        ],
    );

    let results = search_todos(&[path], &TodoSearchOptions::with_query("fix"));

    assert_eq!(results.len(), 3);
    // "Fix at start" should be first due to prefix bonus
    assert!(results[0].task.text.starts_with("Fix"));
}

#[test]
fn test_find_todo_files() {
    let dir = TempDir::new().unwrap();

    // Create some .todo directories (etch stores)
    create_test_todo_dir(&dir, "project.todo", vec![]);
    create_test_todo_dir(&dir, "security.todo", vec![]);

    // Create a subdirectory with another .todo directory
    let subdir = dir.path().join("subproject");
    std::fs::create_dir(&subdir).unwrap();
    let tasks_path = subdir.join("tasks.todo");
    let _store = open_store(&tasks_path).unwrap();

    // Create a non-todo file
    std::fs::write(dir.path().join("readme.md"), "# Readme").unwrap();

    let files = find_todo_files(&[dir.path().to_path_buf()]);

    // find_todo_files looks for .todo extension. With etch stores,
    // these are directories, so we need to check for both.
    assert!(files.len() >= 2);
}

#[test]
fn test_task_ref_from_task() {
    let task = Task::new("Test task")
        .with_list("bugs")
        .with_tag("urgent")
        .with_assigned("alice")
        .with_due(1700000000);

    let task_ref = super::search::TaskRef::from(&task);

    assert_eq!(task_ref.text, "Test task");
    assert_eq!(task_ref.list, "bugs");
    assert_eq!(task_ref.tags, vec!["urgent"]);
    assert_eq!(task_ref.assigned, Some("alice".to_string()));
    assert_eq!(task_ref.due, Some(1700000000));
    assert_eq!(task_ref.status, "open");
}
