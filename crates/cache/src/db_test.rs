use super::*;
use std::sync::Arc;

use tempfile::TempDir;

fn setup_test_env() -> (TempDir, CacheManager) {
    let temp_dir = TempDir::new().unwrap();
    let manager = CacheManager::open_at(temp_dir.path()).unwrap();
    (temp_dir, manager)
}

#[test]
fn test_control_db_open() {
    let temp_dir = TempDir::new().unwrap();
    let control = ControlDb::open_at(temp_dir.path()).unwrap();

    assert!(control.db_path().exists());
    assert!(control.storages_dir().exists());
}

#[test]
fn test_storage_lifecycle() {
    let (_temp_dir, manager) = setup_test_env();

    // Create a storage
    let device_info = DeviceInfo::usb("TEST123", Some("Test Drive".to_string()));
    let (storage, _db) = manager
        .get_or_create_storage(Path::new("/Volumes/Test"), &Some(device_info))
        .unwrap();

    assert_eq!(storage.device_serial, Some("TEST123".to_string()));
    assert!(storage.db_filename.starts_with("serial_"));

    // Storage database should exist
    let db_path = manager.control.storages_dir().join(&storage.db_filename);
    assert!(db_path.exists());

    // Delete the storage
    manager.close_storage_db(storage.id);
    manager.control.delete_storage(storage.id).unwrap();

    // Database dir should be deleted
    assert!(!db_path.exists());
}

#[test]
fn test_file_operations() {
    let (_temp_dir, manager) = setup_test_env();

    let (storage, db) = manager
        .get_or_create_storage(Path::new("/test"), &None)
        .unwrap();

    // Insert a file
    let entry = CacheEntry::new("test/file.txt", 1024, 1700000000);
    db.upsert_file(&entry).unwrap();

    // Look it up
    let cached = db.lookup_file("test/file.txt").unwrap().unwrap();
    assert_eq!(cached.path, "test/file.txt");
    assert_eq!(cached.size, 1024);

    // Look up with validation
    let valid = db
        .lookup_valid_file("test/file.txt", 1024, 1700000000)
        .unwrap();
    assert!(valid.is_some());

    let invalid = db
        .lookup_valid_file("test/file.txt", 9999, 1700000000)
        .unwrap();
    assert!(invalid.is_none());

    // Update stats
    let stats = db.get_stats().unwrap();
    manager
        .control
        .update_storage_stats(storage.id, stats.file_count, stats.total_bytes)
        .unwrap();

    let updated = manager
        .control
        .get_storage_by_id(storage.id)
        .unwrap()
        .unwrap();
    assert_eq!(updated.file_count, 1);
    assert_eq!(updated.total_bytes, 1024);
}

#[test]
fn test_batch_operations() {
    let db = StorageDb::open_memory().unwrap();

    let mut batch = db.new_batch();
    batch.add(CacheEntry::new("file1.txt", 100, 1700000000));
    batch.add(CacheEntry::new("file2.txt", 200, 1700000001));
    batch.add(CacheEntry::new("file3.txt", 300, 1700000002));

    let flushed = db.flush_batch(&mut batch).unwrap();
    assert_eq!(flushed, 3);

    let count = db.count_files().unwrap();
    assert_eq!(count, 3);

    let total = db.total_bytes().unwrap();
    assert_eq!(total, 600);
}

#[test]
fn test_find_by_hash() {
    let db = StorageDb::open_memory().unwrap();

    let hash1 = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    let hash2 = vec![16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1];

    db.upsert_file(&CacheEntry::with_xxh3(
        "file1.txt",
        100,
        1700000000,
        hash1.clone(),
    ))
    .unwrap();

    db.upsert_file(&CacheEntry::with_xxh3(
        "file2.txt",
        100,
        1700000001,
        hash1.clone(),
    ))
    .unwrap();

    db.upsert_file(&CacheEntry::with_xxh3("file3.txt", 100, 1700000002, hash2))
        .unwrap();

    let matches = db.find_by_hash_xxh3(&hash1).unwrap();
    assert_eq!(matches.len(), 2);
}

#[test]
fn test_cache_manager_db_caching() {
    let (_temp_dir, manager) = setup_test_env();

    let (storage, _) = manager
        .get_or_create_storage(Path::new("/test"), &None)
        .unwrap();

    // First access - opens DB
    let db1 = manager.storage_db(storage.id).unwrap();

    // Second access - should return cached DB
    let db2 = manager.storage_db(storage.id).unwrap();

    // Should be the same Arc
    assert!(Arc::ptr_eq(&db1, &db2));
}

#[test]
fn test_list_storages() {
    let (_temp_dir, manager) = setup_test_env();

    manager
        .get_or_create_storage(
            Path::new("/a"),
            &Some(DeviceInfo::usb("A", Some("Drive A".to_string()))),
        )
        .unwrap();

    manager
        .get_or_create_storage(
            Path::new("/b"),
            &Some(DeviceInfo::usb("B", Some("Drive B".to_string()))),
        )
        .unwrap();

    let storages = manager.control.list_storages().unwrap();
    assert_eq!(storages.len(), 2);
}

#[test]
fn test_default_db_dir() {
    let dir = default_db_dir();
    assert!(dir.to_string_lossy().contains("zero"));
}
