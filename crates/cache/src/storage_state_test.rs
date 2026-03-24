//! Tests for etch-backed storage state persistence

use etchdb::Store;
use tempfile::TempDir;

use super::storage_state::{CachedFileEntry, StorageState, open_storage_store};

fn test_entry(
    path: &str,
    size: i64,
    xxh3: Option<&[u8]>,
    blake3: Option<&[u8]>,
) -> CachedFileEntry {
    CachedFileEntry {
        path: path.to_string(),
        size,
        mtime: 1700000000,
        hash_xxh3: xxh3.map(<[u8]>::to_vec),
        hash_blake3: blake3.map(<[u8]>::to_vec),
        indexed_at: 1700000000,
    }
}

#[test]
fn roundtrip_in_memory() {
    let store = Store::<StorageState>::memory();

    store
        .write(|tx| {
            tx.put(&test_entry("photos/a.jpg", 1024, Some(&[1; 16]), None));
            tx.put(&test_entry("docs/b.pdf", 2048, None, Some(&[2; 32])));
            Ok(())
        })
        .unwrap();

    let state = store.read();
    assert_eq!(state.file_count(), 2);
    assert_eq!(state.total_bytes(), 3072);
    assert_eq!(state.hashed_count(), 2);
}

#[test]
fn secondary_index_xxh3() {
    let store = Store::<StorageState>::memory();
    let hash = [0xAB; 16];

    store
        .write(|tx| {
            tx.put(&test_entry("a.txt", 100, Some(&hash), None));
            tx.put(&test_entry("b.txt", 200, Some(&hash), None));
            tx.put(&test_entry("c.txt", 300, None, None));
            Ok(())
        })
        .unwrap();

    let state = store.read();
    let found = state.find_by_hash_xxh3(&hash);
    assert_eq!(found.len(), 2);
}

#[test]
fn secondary_index_blake3() {
    let store = Store::<StorageState>::memory();
    let hash = [0xCD; 32];

    store
        .write(|tx| {
            tx.put(&test_entry("a.txt", 100, None, Some(&hash)));
            tx.put(&test_entry("b.txt", 200, None, Some(&hash)));
            Ok(())
        })
        .unwrap();

    let state = store.read();
    let found = state.find_by_hash_blake3(&hash);
    assert_eq!(found.len(), 2);
}

#[test]
fn secondary_index_size() {
    let store = Store::<StorageState>::memory();

    store
        .write(|tx| {
            tx.put(&test_entry("a.txt", 1024, None, None));
            tx.put(&test_entry("b.txt", 1024, None, None));
            tx.put(&test_entry("c.txt", 2048, None, None));
            Ok(())
        })
        .unwrap();

    let state = store.read();
    assert_eq!(state.find_by_size(1024).len(), 2);
    assert_eq!(state.find_by_size(2048).len(), 1);
    assert_eq!(state.find_by_size(9999).len(), 0);
}

#[test]
fn upsert_updates_indexes() {
    let store = Store::<StorageState>::memory();
    let old_hash = [1; 16];
    let new_hash = [2; 16];

    store
        .write(|tx| {
            tx.put(&test_entry("file.txt", 100, Some(&old_hash), None));
            Ok(())
        })
        .unwrap();

    // Update with new hash
    store
        .write(|tx| {
            tx.put(&test_entry("file.txt", 200, Some(&new_hash), None));
            Ok(())
        })
        .unwrap();

    let state = store.read();
    assert_eq!(state.file_count(), 1);
    assert_eq!(state.find_by_hash_xxh3(&old_hash).len(), 0);
    assert_eq!(state.find_by_hash_xxh3(&new_hash).len(), 1);
    assert_eq!(state.find_by_size(100).len(), 0);
    assert_eq!(state.find_by_size(200).len(), 1);
}

#[test]
fn delete_cleans_indexes() {
    let store = Store::<StorageState>::memory();
    let hash = [0xFF; 16];

    store
        .write(|tx| {
            tx.put(&test_entry("file.txt", 500, Some(&hash), None));
            Ok(())
        })
        .unwrap();

    store
        .write(|tx| {
            tx.delete("file.txt");
            Ok(())
        })
        .unwrap();

    let state = store.read();
    assert_eq!(state.file_count(), 0);
    assert_eq!(state.find_by_hash_xxh3(&hash).len(), 0);
    assert_eq!(state.find_by_size(500).len(), 0);
}

#[test]
fn get_valid_checks_metadata() {
    let store = Store::<StorageState>::memory();

    store
        .write(|tx| {
            tx.put(&CachedFileEntry {
                path: "file.txt".into(),
                size: 100,
                mtime: 1700000000,
                hash_xxh3: Some(vec![1; 16]),
                hash_blake3: None,
                indexed_at: 1700000000,
            });
            Ok(())
        })
        .unwrap();

    let state = store.read();
    assert!(state.get_valid("file.txt", 100, 1700000000).is_some());
    assert!(state.get_valid("file.txt", 999, 1700000000).is_none());
    assert!(state.get_valid("file.txt", 100, 9999999999).is_none());
}

#[test]
fn tx_overlay_read_your_writes() {
    let store = Store::<StorageState>::memory();

    store
        .write(|tx| {
            tx.put(&test_entry("a.txt", 100, None, None));
            // Should be visible within this transaction
            assert!(tx.get("a.txt").is_some());
            assert!(tx.get("b.txt").is_none());
            Ok(())
        })
        .unwrap();
}

#[test]
fn tx_overlay_delete_hides_committed() {
    let store = Store::<StorageState>::memory();

    store
        .write(|tx| {
            tx.put(&test_entry("a.txt", 100, None, None));
            Ok(())
        })
        .unwrap();

    store
        .write(|tx| {
            // Committed entry visible
            assert!(tx.get("a.txt").is_some());
            tx.delete("a.txt");
            // Now hidden by overlay
            assert!(tx.get("a.txt").is_none());
            Ok(())
        })
        .unwrap();
}

#[test]
fn wal_replay() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test_storage");

    {
        let store = open_storage_store(&path).unwrap();
        store
            .write(|tx| {
                tx.put(&test_entry("a.txt", 100, Some(&[1; 16]), None));
                tx.put(&test_entry("b.txt", 200, None, Some(&[2; 32])));
                Ok(())
            })
            .unwrap();
    }

    // Reopen — WAL should replay and rebuild indexes
    {
        let store = open_storage_store(&path).unwrap();
        let state = store.read();
        assert_eq!(state.file_count(), 2);
        assert_eq!(state.find_by_hash_xxh3(&[1; 16]).len(), 1);
        assert_eq!(state.find_by_hash_blake3(&[2; 32]).len(), 1);
        assert_eq!(state.find_by_size(100).len(), 1);
    }
}

#[test]
fn snapshot_rebuilds_indexes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test_storage");

    {
        let store = open_storage_store(&path).unwrap();
        let hash_xxh3 = [0xAA; 16];
        let hash_blake3 = [0xBB; 32];

        store
            .write(|tx| {
                tx.put(&test_entry(
                    "dup1.txt",
                    1024,
                    Some(&hash_xxh3),
                    Some(&hash_blake3),
                ));
                tx.put(&test_entry(
                    "dup2.txt",
                    1024,
                    Some(&hash_xxh3),
                    Some(&hash_blake3),
                ));
                tx.put(&test_entry("other.txt", 2048, None, None));
                Ok(())
            })
            .unwrap();
        store.flush().unwrap();
    }

    // Reopen from snapshot — secondary indexes rebuilt via after_load
    {
        let store = open_storage_store(&path).unwrap();
        let state = store.read();
        assert_eq!(state.file_count(), 3);
        assert_eq!(state.find_by_hash_xxh3(&[0xAA; 16]).len(), 2);
        assert_eq!(state.find_by_hash_blake3(&[0xBB; 32]).len(), 2);
        assert_eq!(state.find_by_size(1024).len(), 2);
        assert_eq!(state.find_by_size(2048).len(), 1);
    }
}

#[test]
fn wal_replay_with_delete() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test_storage");

    {
        let store = open_storage_store(&path).unwrap();
        store
            .write(|tx| {
                tx.put(&test_entry("a.txt", 100, Some(&[1; 16]), None));
                tx.put(&test_entry("b.txt", 200, Some(&[2; 16]), None));
                Ok(())
            })
            .unwrap();
        store
            .write(|tx| {
                tx.delete("a.txt");
                Ok(())
            })
            .unwrap();
    }

    {
        let store = open_storage_store(&path).unwrap();
        let state = store.read();
        assert_eq!(state.file_count(), 1);
        assert!(state.get("a.txt").is_none());
        assert!(state.get("b.txt").is_some());
        assert_eq!(state.find_by_hash_xxh3(&[1; 16]).len(), 0);
        assert_eq!(state.find_by_hash_xxh3(&[2; 16]).len(), 1);
    }
}
