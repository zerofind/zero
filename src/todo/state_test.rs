//! Tests for etch-backed TodoState

use etchdb::Store;
use tempfile::TempDir;

use super::state::{TodoState, open_store};
use super::task::Task;

#[test]
fn roundtrip_in_memory() {
    let store = Store::<TodoState>::memory();

    store
        .write(|tx| {
            let mut task = Task::new("First task");
            task.id = 1;
            tx.put_task(&task);
            Ok(())
        })
        .unwrap();

    let state = store.read();
    assert_eq!(state.file.len(), 1);
    assert_eq!(state.file.get_task(1).unwrap().text, "First task");
}

#[test]
fn multiple_tasks() {
    let store = Store::<TodoState>::memory();

    store
        .write(|tx| {
            let mut t1 = Task::new("One");
            t1.id = 1;
            let mut t2 = Task::new("Two").with_list("bugs");
            t2.id = 2;
            tx.put_task(&t1);
            tx.put_task(&t2);
            Ok(())
        })
        .unwrap();

    let state = store.read();
    assert_eq!(state.file.len(), 2);
    assert_eq!(state.file.get_task(2).unwrap().list, "bugs");
}

#[test]
fn delete_task() {
    let store = Store::<TodoState>::memory();

    store
        .write(|tx| {
            let mut t = Task::new("Gone");
            t.id = 1;
            tx.put_task(&t);
            Ok(())
        })
        .unwrap();

    store
        .write(|tx| {
            tx.delete_task(1);
            Ok(())
        })
        .unwrap();

    let state = store.read();
    assert!(state.file.is_empty());
}

#[test]
fn upsert_replaces() {
    let store = Store::<TodoState>::memory();

    store
        .write(|tx| {
            let mut t = Task::new("Original");
            t.id = 1;
            tx.put_task(&t);
            Ok(())
        })
        .unwrap();

    store
        .write(|tx| {
            let mut t = Task::new("Updated");
            t.id = 1;
            tx.put_task(&t);
            Ok(())
        })
        .unwrap();

    let state = store.read();
    assert_eq!(state.file.len(), 1);
    assert_eq!(state.file.get_task(1).unwrap().text, "Updated");
}

#[test]
fn wal_replay() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.todo");

    // Write some tasks
    {
        let store = open_store(&path).unwrap();
        store
            .write(|tx| {
                let mut t1 = Task::new("Persisted one");
                t1.id = 1;
                let mut t2 = Task::new("Persisted two").with_tag("urgent");
                t2.id = 2;
                tx.put_task(&t1);
                tx.put_task(&t2);
                Ok(())
            })
            .unwrap();
    }

    // Reopen and verify WAL replay
    {
        let store = open_store(&path).unwrap();
        let state = store.read();
        assert_eq!(state.file.len(), 2);
        assert_eq!(state.file.get_task(1).unwrap().text, "Persisted one");
        assert!(state.file.get_task(2).unwrap().has_tag("urgent"));
    }
}

#[test]
fn wal_replay_with_delete() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("del.todo");

    {
        let store = open_store(&path).unwrap();
        store
            .write(|tx| {
                let mut t1 = Task::new("Keep");
                t1.id = 1;
                let mut t2 = Task::new("Delete me");
                t2.id = 2;
                tx.put_task(&t1);
                tx.put_task(&t2);
                Ok(())
            })
            .unwrap();

        store
            .write(|tx| {
                tx.delete_task(2);
                Ok(())
            })
            .unwrap();
    }

    {
        let store = open_store(&path).unwrap();
        let state = store.read();
        assert_eq!(state.file.len(), 1);
        assert_eq!(state.file.get_task(1).unwrap().text, "Keep");
        assert!(state.file.get_task(2).is_none());
    }
}

#[test]
fn tx_read_your_writes() {
    let store = Store::<TodoState>::memory();

    store
        .write(|tx| {
            let mut t = Task::new("Written in tx");
            t.id = 42;
            tx.put_task(&t);

            // Should be visible within the same transaction
            let found = tx.get_task(42);
            assert!(found.is_some());
            assert_eq!(found.unwrap().text, "Written in tx");
            Ok(())
        })
        .unwrap();
}

#[test]
fn tx_delete_hides_committed() {
    let store = Store::<TodoState>::memory();

    // First, commit a task
    store
        .write(|tx| {
            let mut t = Task::new("Exists");
            t.id = 1;
            tx.put_task(&t);
            Ok(())
        })
        .unwrap();

    // In a new tx, delete it — should be invisible
    store
        .write(|tx| {
            tx.delete_task(1);
            assert!(tx.get_task(1).is_none());
            Ok(())
        })
        .unwrap();
}

#[test]
fn next_id_tracks_across_upserts() {
    let store = Store::<TodoState>::memory();

    store
        .write(|tx| {
            let mut t = Task::new("Task");
            t.id = 5;
            tx.put_task(&t);
            Ok(())
        })
        .unwrap();

    let state = store.read();
    assert!(state.file.next_id() > 5);
}
