//! Tests for transfer state persistence (etch-backed)

use super::*;
use std::path::PathBuf;
use tempfile::TempDir;

// ============================================================================
// JobState tests
// ============================================================================

#[test]
fn test_job_state_creation() {
    let job = JobState::new(
        PathBuf::from("/source"),
        PathBuf::from("/dest"),
        100,
        1_000_000,
    );

    assert!(!job.id.is_empty());
    assert_eq!(job.total_files, 100);
    assert_eq!(job.total_bytes, 1_000_000);
    assert!(job.started_at > 0);
}

#[test]
fn test_job_state_paths() {
    let job = JobState::new(
        PathBuf::from("/my/source"),
        PathBuf::from("/my/dest"),
        50,
        500_000,
    );

    assert_eq!(job.source, PathBuf::from("/my/source"));
    assert_eq!(job.dest, PathBuf::from("/my/dest"));
}

// ============================================================================
// CompletedEntry tests
// ============================================================================

#[test]
fn test_completed_entry_roundtrip() {
    let entry = CompletedEntry {
        path: PathBuf::from("foo/bar.txt"),
        size: 12345,
        hash: "abc123def456".to_string(),
    };

    let line = entry.to_line();
    let parsed = CompletedEntry::from_line(&line).unwrap();

    assert_eq!(parsed.path, entry.path);
    assert_eq!(parsed.size, entry.size);
    assert_eq!(parsed.hash, entry.hash);
}

#[test]
fn test_completed_entry_empty_hash() {
    let entry = CompletedEntry {
        path: PathBuf::from("test.txt"),
        size: 100,
        hash: String::new(),
    };

    let line = entry.to_line();
    let parsed = CompletedEntry::from_line(&line).unwrap();

    assert_eq!(parsed.hash, "");
}

#[test]
fn test_completed_entry_with_spaces_in_path() {
    let entry = CompletedEntry {
        path: PathBuf::from("path with spaces/file name.txt"),
        size: 500,
        hash: "hash123".to_string(),
    };

    let line = entry.to_line();
    let parsed = CompletedEntry::from_line(&line).unwrap();

    assert_eq!(parsed.path, entry.path);
}

#[test]
fn test_completed_entry_zero_size() {
    let entry = CompletedEntry {
        path: PathBuf::from("empty.txt"),
        size: 0,
        hash: "emptyhash".to_string(),
    };

    let line = entry.to_line();
    let parsed = CompletedEntry::from_line(&line).unwrap();

    assert_eq!(parsed.size, 0);
}

#[test]
fn test_completed_entry_large_size() {
    let entry = CompletedEntry {
        path: PathBuf::from("large.bin"),
        size: 10_000_000_000, // 10GB
        hash: "largehash".to_string(),
    };

    let line = entry.to_line();
    let parsed = CompletedEntry::from_line(&line).unwrap();

    assert_eq!(parsed.size, 10_000_000_000);
}

// ============================================================================
// TransferState lifecycle tests
// ============================================================================

#[test]
fn test_transfer_state_lifecycle() {
    let dir = TempDir::new().unwrap();
    let state = TransferState::new(dir.path()).unwrap();

    // Initially no active job
    assert!(!state.has_active_job());

    // Save job
    let job = JobState::new(PathBuf::from("/source"), PathBuf::from("/dest"), 10, 10000);
    state.save_job(&job).unwrap();
    assert!(state.has_active_job());

    // Load job
    let loaded = state.load_job().unwrap().unwrap();
    assert_eq!(loaded.total_files, 10);
    assert_eq!(loaded.source, PathBuf::from("/source"));

    // Append completed entries
    let entry1 = CompletedEntry {
        path: PathBuf::from("file1.txt"),
        size: 100,
        hash: "hash1".to_string(),
    };
    let entry2 = CompletedEntry {
        path: PathBuf::from("subdir/file2.txt"),
        size: 200,
        hash: "hash2".to_string(),
    };
    state.append_completed(&entry1).unwrap();
    state.append_completed(&entry2).unwrap();

    assert_eq!(state.completed_count().unwrap(), 2);

    // Load completed set
    let set = state.load_completed_set().unwrap();
    assert!(set.contains(&PathBuf::from("file1.txt")));
    assert!(set.contains(&PathBuf::from("subdir/file2.txt")));
    assert!(!set.contains(&PathBuf::from("nonexistent.txt")));

    // Load completed entries
    let entries = state.load_completed_entries().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].size, 100);
    assert_eq!(entries[1].size, 200);

    // Cleanup
    state.cleanup().unwrap();
    // After cleanup, the store dir is gone
    assert!(!state.state_dir().exists());
}

#[test]
fn test_state_dir_creation() {
    let dir = TempDir::new().unwrap();
    let nested_dest = dir.path().join("nested").join("dest");

    // Create the dest directory first
    std::fs::create_dir_all(&nested_dest).unwrap();

    let state = TransferState::new(&nested_dest).unwrap();
    assert!(state.state_dir().exists());
    assert!(state.state_dir().ends_with(".zero"));
}

#[test]
fn test_empty_completed_log() {
    let dir = TempDir::new().unwrap();
    let state = TransferState::new(dir.path()).unwrap();

    // No completed entries yet
    assert_eq!(state.completed_count().unwrap(), 0);
    assert!(state.load_completed_set().unwrap().is_empty());
    assert!(state.load_completed_entries().unwrap().is_empty());
}

// ============================================================================
// TransferState job management tests
// ============================================================================

#[test]
fn test_save_and_load_job() {
    let dir = TempDir::new().unwrap();
    let state = TransferState::new(dir.path()).unwrap();

    let job = JobState::new(
        PathBuf::from("/test/source"),
        PathBuf::from("/test/dest"),
        50,
        500_000,
    );

    state.save_job(&job).unwrap();
    let loaded = state.load_job().unwrap().unwrap();

    assert_eq!(loaded.id, job.id);
    assert_eq!(loaded.source, job.source);
    assert_eq!(loaded.dest, job.dest);
    assert_eq!(loaded.total_files, job.total_files);
    assert_eq!(loaded.total_bytes, job.total_bytes);
}

#[test]
fn test_load_job_when_none_exists() {
    let dir = TempDir::new().unwrap();
    let state = TransferState::new(dir.path()).unwrap();

    let loaded = state.load_job().unwrap();
    assert!(loaded.is_none());
}

// ============================================================================
// TransferState completed entries tests
// ============================================================================

#[test]
fn test_append_multiple_completed() {
    let dir = TempDir::new().unwrap();
    let state = TransferState::new(dir.path()).unwrap();

    for i in 0..10 {
        let entry = CompletedEntry {
            path: PathBuf::from(format!("file{}.txt", i)),
            size: i * 100,
            hash: format!("hash{}", i),
        };
        state.append_completed(&entry).unwrap();
    }

    assert_eq!(state.completed_count().unwrap(), 10);

    let set = state.load_completed_set().unwrap();
    assert_eq!(set.len(), 10);
    assert!(set.contains(&PathBuf::from("file5.txt")));
}

#[test]
fn test_completed_set_deduplicates() {
    let dir = TempDir::new().unwrap();
    let state = TransferState::new(dir.path()).unwrap();

    // Add same path twice
    let entry1 = CompletedEntry {
        path: PathBuf::from("duplicate.txt"),
        size: 100,
        hash: "hash1".to_string(),
    };
    let entry2 = CompletedEntry {
        path: PathBuf::from("duplicate.txt"),
        size: 100,
        hash: "hash2".to_string(),
    };

    state.append_completed(&entry1).unwrap();
    state.append_completed(&entry2).unwrap();

    // Set should deduplicate by path
    let set = state.load_completed_set().unwrap();
    assert!(set.contains(&PathBuf::from("duplicate.txt")));
    // But entries list keeps both
    assert_eq!(state.completed_count().unwrap(), 2);
}

// ============================================================================
// TransferState cleanup tests
// ============================================================================

#[test]
fn test_cleanup_removes_state() {
    let dir = TempDir::new().unwrap();
    let state = TransferState::new(dir.path()).unwrap();

    // Create some state
    let job = JobState::new(PathBuf::from("/src"), PathBuf::from("/dst"), 5, 5000);
    state.save_job(&job).unwrap();
    state
        .append_completed(&CompletedEntry {
            path: PathBuf::from("test.txt"),
            size: 100,
            hash: "hash".to_string(),
        })
        .unwrap();

    assert!(state.has_active_job());
    assert_eq!(state.completed_count().unwrap(), 1);

    // Cleanup
    state.cleanup().unwrap();

    assert!(!state.state_dir().exists());
}

// ============================================================================
// WAL replay / crash recovery tests
// ============================================================================

#[test]
fn test_wal_replay_on_reopen() {
    let dir = TempDir::new().unwrap();

    // Write state and drop
    {
        let state = TransferState::new(dir.path()).unwrap();
        let job = JobState::new(PathBuf::from("/src"), PathBuf::from("/dst"), 3, 3000);
        state.save_job(&job).unwrap();
        state
            .append_completed(&CompletedEntry {
                path: PathBuf::from("a.txt"),
                size: 100,
                hash: "h1".to_string(),
            })
            .unwrap();
        state
            .append_completed(&CompletedEntry {
                path: PathBuf::from("b.txt"),
                size: 200,
                hash: "h2".to_string(),
            })
            .unwrap();
    }

    // Reopen — WAL should replay
    let state = TransferState::new(dir.path()).unwrap();
    assert!(state.has_active_job());
    let job = state.load_job().unwrap().unwrap();
    assert_eq!(job.total_files, 3);

    assert_eq!(state.completed_count().unwrap(), 2);
    let set = state.load_completed_set().unwrap();
    assert!(set.contains(&PathBuf::from("a.txt")));
    assert!(set.contains(&PathBuf::from("b.txt")));
}

// ============================================================================
// StateError tests
// ============================================================================

#[test]
fn test_state_error_display() {
    let err = StateError::NoActiveJob;
    let display = format!("{}", err);
    assert!(!display.is_empty());
}
