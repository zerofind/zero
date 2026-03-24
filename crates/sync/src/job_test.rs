use super::*;
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;

fn create_test_source(dir: &Path) {
    fs::create_dir_all(dir.join("subdir")).unwrap();

    File::create(dir.join("file1.txt"))
        .unwrap()
        .write_all(b"hello world")
        .unwrap();

    File::create(dir.join("file2.txt"))
        .unwrap()
        .write_all(b"goodbye world")
        .unwrap();

    File::create(dir.join("subdir/nested.txt"))
        .unwrap()
        .write_all(b"nested content")
        .unwrap();
}

#[test]
fn test_sync_fresh_dest() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");

    create_test_source(&source);

    let job = SyncJob::new(&source, &dest, SyncOptions::default()).unwrap();
    let result = job.run(|_| {}).unwrap();

    assert_eq!(result.files_transferred, 3);
    assert_eq!(result.files_unchanged, 0);
    assert_eq!(result.errors, 0);
    assert_eq!(result.diff_summary.added, 3);

    // Verify files exist
    assert!(dest.join("file1.txt").exists());
    assert!(dest.join("file2.txt").exists());
    assert!(dest.join("subdir/nested.txt").exists());
}

#[test]
fn test_sync_already_synced() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");

    create_test_source(&source);

    // First sync
    let job = SyncJob::new(&source, &dest, SyncOptions::default()).unwrap();
    let _ = job.run(|_| {}).unwrap();

    // Second sync - should be no-op
    let job2 = SyncJob::new(&source, &dest, SyncOptions::default()).unwrap();
    let result = job2.run(|_| {}).unwrap();

    assert_eq!(result.files_transferred, 0);
    assert_eq!(result.files_unchanged, 3);
    assert_eq!(result.diff_summary.identical, 3);
}

#[test]
fn test_sync_with_changes() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");

    create_test_source(&source);

    // First sync
    let job = SyncJob::new(&source, &dest, SyncOptions::default()).unwrap();
    let _ = job.run(|_| {}).unwrap();

    // Modify a file in source
    std::thread::sleep(std::time::Duration::from_millis(10)); // Ensure mtime differs
    File::create(source.join("file1.txt"))
        .unwrap()
        .write_all(b"modified content")
        .unwrap();

    // Add a new file
    File::create(source.join("new.txt"))
        .unwrap()
        .write_all(b"new file")
        .unwrap();

    // Second sync
    let job2 = SyncJob::new(&source, &dest, SyncOptions::default()).unwrap();
    let result = job2.run(|_| {}).unwrap();

    assert_eq!(result.files_transferred, 2); // modified + new
    assert_eq!(result.diff_summary.modified, 1);
    assert_eq!(result.diff_summary.added, 1);

    // Verify content
    assert_eq!(
        fs::read_to_string(dest.join("file1.txt")).unwrap(),
        "modified content"
    );
    assert!(dest.join("new.txt").exists());
}

#[test]
fn test_sync_delete_orphans() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");

    create_test_source(&source);

    // First sync
    let job = SyncJob::new(&source, &dest, SyncOptions::default()).unwrap();
    let _ = job.run(|_| {}).unwrap();

    // Delete a file from source
    fs::remove_file(source.join("file2.txt")).unwrap();

    // Sync with delete_orphans
    let options = SyncOptions {
        delete_orphans: true,
        ..Default::default()
    };
    let job2 = SyncJob::new(&source, &dest, options).unwrap();
    let result = job2.run(|_| {}).unwrap();

    assert_eq!(result.files_deleted, 1);
    assert_eq!(result.diff_summary.removed, 1);
    assert!(!dest.join("file2.txt").exists());
}

#[test]
fn test_sync_source_not_found() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("nonexistent");
    let dest = dir.path().join("dest");

    let result = SyncJob::new(&source, &dest, SyncOptions::default());
    assert!(matches!(result, Err(SyncError::SourceNotFound(_))));
}

#[test]
fn test_sync_progress_percent() {
    let progress = SyncProgress {
        phase: SyncPhase::Transferring,
        total_files: 10,
        files_done: 5,
        total_bytes: 1000,
        bytes_done: 500,
        current_file: None,
        files_to_delete: 0,
        files_deleted: 0,
        errors: 0,
    };

    assert!((progress.percent() - 50.0).abs() < 0.001);
}

#[test]
fn test_sync_empty_source() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");

    fs::create_dir_all(&source).unwrap();

    let job = SyncJob::new(&source, &dest, SyncOptions::default()).unwrap();
    let result = job.run(|_| {}).unwrap();

    assert_eq!(result.files_transferred, 0);
    assert_eq!(result.errors, 0);
}

#[test]
fn test_sync_parallel_copies() {
    use std::io::Write;

    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");

    // Create many small files to test parallel copying
    fs::create_dir_all(&source).unwrap();
    for i in 0..20 {
        fs::File::create(source.join(format!("file{i}.txt")))
            .unwrap()
            .write_all(format!("content {i}").as_bytes())
            .unwrap();
    }

    let options = SyncOptions {
        parallel_copies: 4,
        ..Default::default()
    };

    let job = SyncJob::new(&source, &dest, options).unwrap();
    let result = job.run(|_| {}).unwrap();

    assert_eq!(result.files_transferred, 20);
    assert_eq!(result.errors, 0);

    // Verify all files exist
    for i in 0..20 {
        assert!(dest.join(format!("file{i}.txt")).exists());
    }
}

#[test]
fn test_is_dest_empty() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let dest = dir.path().join("dest");

    fs::create_dir_all(&source).unwrap();

    // Test with non-existent dest
    let job = SyncJob::new(&source, &dest, SyncOptions::default()).unwrap();
    // dest was created by SyncJob::new, but should be empty
    assert!(job.is_dest_empty());

    // Add a file to dest
    std::fs::write(dest.join("file.txt"), b"test").unwrap();

    // Now it should not be empty
    assert!(!job.is_dest_empty());
}

#[test]
fn test_preallocate_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("preallocated.bin");

    preallocate_file(&path, 1024).unwrap();

    assert!(path.exists());
    let metadata = fs::metadata(&path).unwrap();
    assert_eq!(metadata.len(), 1024);
}
