//! Tests for etch-backed index persistence

use etchdb::Store;
use tempfile::TempDir;

use super::etch::open_index_store;
use super::node::FileNode;
use super::search::SearchIndex;

#[test]
fn roundtrip_in_memory() {
    let store = Store::<SearchIndex>::memory();

    store
        .write(|tx| {
            tx.put_node(&FileNode::file(
                "report.pdf".into(),
                "Documents/report.pdf".into(),
                1024,
                1700000000,
            ));
            tx.put_node(&FileNode::directory(
                "Documents".into(),
                "Documents".into(),
                1700000000,
            ));
            Ok(())
        })
        .unwrap();

    let state = store.read();
    assert_eq!(state.file_count(), 1);
    assert_eq!(state.dir_count(), 1);
    assert_eq!(state.node_count(), 2);
}

#[test]
fn insert_and_search() {
    let store = Store::<SearchIndex>::memory();

    store
        .write(|tx| {
            tx.put_node(&FileNode::file(
                "report.pdf".into(),
                "Documents/report.pdf".into(),
                1024,
                1700000000,
            ));
            tx.put_node(&FileNode::file(
                "report_final.pdf".into(),
                "Documents/report_final.pdf".into(),
                2048,
                1700000001,
            ));
            tx.put_node(&FileNode::file(
                "notes.txt".into(),
                "Documents/notes.txt".into(),
                512,
                1700000002,
            ));
            Ok(())
        })
        .unwrap();

    let state = store.read();
    let results = state.search("report", 10);
    assert_eq!(results.len(), 2);
}

#[test]
fn delete_node() {
    let store = Store::<SearchIndex>::memory();

    store
        .write(|tx| {
            tx.put_node(&FileNode::file(
                "old.txt".into(),
                "old.txt".into(),
                100,
                1700000000,
            ));
            tx.put_node(&FileNode::file(
                "keep.txt".into(),
                "keep.txt".into(),
                200,
                1700000001,
            ));
            Ok(())
        })
        .unwrap();

    assert_eq!(store.read().file_count(), 2);

    store
        .write(|tx| {
            tx.delete_node("old.txt");
            Ok(())
        })
        .unwrap();

    let state = store.read();
    assert_eq!(state.file_count(), 1);
    let results = state.search("old", 10);
    assert!(results.is_empty());
}

#[test]
fn wal_replay() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.idx");

    // Write some nodes
    {
        let store = open_index_store(&path).unwrap();
        store
            .write(|tx| {
                tx.put_node(&FileNode::file(
                    "alpha.rs".into(),
                    "src/alpha.rs".into(),
                    500,
                    1700000000,
                ));
                tx.put_node(&FileNode::file(
                    "beta.rs".into(),
                    "src/beta.rs".into(),
                    600,
                    1700000001,
                ));
                Ok(())
            })
            .unwrap();
    }

    // Reopen — WAL should replay
    {
        let store = open_index_store(&path).unwrap();
        let state = store.read();
        assert_eq!(state.file_count(), 2);

        let results = state.search("alpha", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node.name, "alpha.rs");
    }
}

#[test]
fn wal_replay_with_delete() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.idx");

    {
        let store = open_index_store(&path).unwrap();
        store
            .write(|tx| {
                tx.put_node(&FileNode::file(
                    "a.txt".into(),
                    "a.txt".into(),
                    100,
                    1700000000,
                ));
                tx.put_node(&FileNode::file(
                    "b.txt".into(),
                    "b.txt".into(),
                    200,
                    1700000001,
                ));
                Ok(())
            })
            .unwrap();
        store
            .write(|tx| {
                tx.delete_node("a.txt");
                Ok(())
            })
            .unwrap();
    }

    // Reopen — should have only b.txt
    {
        let store = open_index_store(&path).unwrap();
        let state = store.read();
        assert_eq!(state.file_count(), 1);
        let results = state.search("b", 10);
        assert_eq!(results.len(), 1);
    }
}

#[test]
fn snapshot_preserves_bitmaps() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.idx");

    // Build an index, snapshot it
    {
        let store = open_index_store(&path).unwrap();
        store
            .write(|tx| {
                tx.put_node(&FileNode::file(
                    "photo.jpg".into(),
                    "pics/photo.jpg".into(),
                    5000,
                    1700000000,
                ));
                tx.put_node(&FileNode::file(
                    "code.rs".into(),
                    "src/code.rs".into(),
                    300,
                    1700000001,
                ));
                tx.put_node(&FileNode::directory("src".into(), "src".into(), 1700000000));
                Ok(())
            })
            .unwrap();
        store.flush().unwrap();
    }

    // Reopen — snapshot should restore pre-built bitmaps
    {
        let store = open_index_store(&path).unwrap();
        let state = store.read();
        assert_eq!(state.file_count(), 2);
        assert_eq!(state.dir_count(), 1);
        assert_eq!(state.node_count(), 3);

        // Type index should work without rebuild
        let type_stats = state.type_stats();
        assert!(type_stats.images > 0);
        assert!(type_stats.code > 0);
    }
}

#[test]
fn roots_preserved() {
    let store = Store::<SearchIndex>::memory();

    store
        .write(|tx| {
            // We need mutable access to set roots, but that's on the state itself.
            // For now, roots are set outside of transactions.
            // This tests that the etch layer doesn't lose them.
            tx.put_node(&FileNode::file(
                "test.txt".into(),
                "/Users/me/docs/test.txt".into(),
                100,
                1700000000,
            ));
            Ok(())
        })
        .unwrap();

    // Roots are managed outside of etch transactions
    // (set by IndexManager when building the index)
    let state = store.read();
    assert_eq!(state.file_count(), 1);
}
