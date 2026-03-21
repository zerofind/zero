//! Tests for AtomicProgress and ProgressSnapshot

use super::tracker::AtomicProgress;
use std::sync::Arc;
use std::thread;

#[test]
fn test_new() {
    let progress = AtomicProgress::new(10, 1000);

    assert_eq!(progress.files_total(), 10);
    assert_eq!(progress.bytes_total(), 1000);
    assert_eq!(progress.files_done(), 0);
    assert_eq!(progress.bytes_done(), 0);
    assert_eq!(progress.errors(), 0);
}

#[test]
fn test_empty() {
    let progress = AtomicProgress::empty();

    assert_eq!(progress.files_total(), 0);
    assert_eq!(progress.bytes_total(), 0);
}

#[test]
fn test_add_bytes() {
    let progress = AtomicProgress::new(1, 1000);

    progress.add_bytes(100);
    assert_eq!(progress.bytes_done(), 100);

    progress.add_bytes(200);
    assert_eq!(progress.bytes_done(), 300);
}

#[test]
fn test_file_done() {
    let progress = AtomicProgress::new(10, 1000);

    progress.file_done();
    assert_eq!(progress.files_done(), 1);

    progress.file_done();
    progress.file_done();
    assert_eq!(progress.files_done(), 3);
}

#[test]
fn test_add_error() {
    let progress = AtomicProgress::new(10, 1000);

    progress.add_error();
    assert_eq!(progress.errors(), 1);

    progress.add_error();
    assert_eq!(progress.errors(), 2);
}

#[test]
fn test_current_file() {
    let progress = AtomicProgress::new(1, 100);

    assert_eq!(progress.current_file(), "");

    progress.set_current_file("test.txt");
    assert_eq!(progress.current_file(), "test.txt");

    progress.set_current_file("another/path/file.rs");
    assert_eq!(progress.current_file(), "another/path/file.rs");

    progress.clear_current_file();
    assert_eq!(progress.current_file(), "");
}

#[test]
fn test_set_totals() {
    let progress = AtomicProgress::empty();

    progress.set_files_total(50);
    progress.set_bytes_total(5000);

    assert_eq!(progress.files_total(), 50);
    assert_eq!(progress.bytes_total(), 5000);
}

#[test]
fn test_snapshot() {
    let progress = AtomicProgress::new(10, 1000);
    progress.add_bytes(250);
    progress.file_done();
    progress.file_done();
    progress.add_error();
    progress.set_current_file("current.txt");

    let snapshot = progress.snapshot();

    assert_eq!(snapshot.files_total, 10);
    assert_eq!(snapshot.bytes_total, 1000);
    assert_eq!(snapshot.bytes_done, 250);
    assert_eq!(snapshot.files_done, 2);
    assert_eq!(snapshot.errors, 1);
    assert_eq!(snapshot.current_file, "current.txt");
    assert!(snapshot.elapsed_secs >= 0.0);
}

#[test]
fn test_snapshot_percent_by_bytes() {
    let progress = AtomicProgress::new(4, 1000);
    progress.add_bytes(250);

    let snapshot = progress.snapshot();
    assert!((snapshot.percent() - 25.0).abs() < 0.01);
}

#[test]
fn test_snapshot_percent_by_files_when_no_bytes() {
    let progress = AtomicProgress::new(4, 0);
    progress.file_done();
    progress.file_done();

    let snapshot = progress.snapshot();
    assert!((snapshot.percent() - 50.0).abs() < 0.01);
}

#[test]
fn test_snapshot_percent_empty() {
    let progress = AtomicProgress::new(0, 0);

    let snapshot = progress.snapshot();
    assert!((snapshot.percent() - 100.0).abs() < 0.01);
}

#[test]
fn test_snapshot_throughput() {
    // Can't easily test exact throughput since it depends on elapsed time,
    // but we can verify it returns a reasonable value
    let progress = AtomicProgress::new(1, 1000);
    progress.add_bytes(500);

    let snapshot = progress.snapshot();
    // Throughput should be positive if bytes were processed
    assert!(snapshot.throughput() >= 0.0);
}

#[test]
fn test_snapshot_eta_secs() {
    let progress = AtomicProgress::new(1, 1000);

    // Initially no throughput, so ETA should be None
    let snapshot = progress.snapshot();
    // Note: might be Some if enough time has passed, so just check it doesn't panic
    let _ = snapshot.eta_secs();

    progress.add_bytes(500);
    let snapshot2 = progress.snapshot();
    // ETA might be Some or None depending on timing
    let _ = snapshot2.eta_secs();
}

#[test]
fn test_is_complete() {
    let progress = AtomicProgress::new(2, 200);
    assert!(!progress.is_complete());

    progress.file_done();
    assert!(!progress.is_complete());

    progress.file_done();
    assert!(progress.is_complete());
}

#[test]
fn test_is_complete_empty() {
    let progress = AtomicProgress::new(0, 0);
    // Empty progress is not considered complete (files_total == 0)
    assert!(!progress.is_complete());
}

#[test]
fn test_reset() {
    let progress = AtomicProgress::new(10, 1000);
    progress.add_bytes(500);
    progress.file_done();
    progress.add_error();
    progress.set_current_file("test.txt");

    progress.reset(20, 2000);

    assert_eq!(progress.files_total(), 20);
    assert_eq!(progress.bytes_total(), 2000);
    assert_eq!(progress.files_done(), 0);
    assert_eq!(progress.bytes_done(), 0);
    assert_eq!(progress.errors(), 0);
    assert_eq!(progress.current_file(), "");
}

#[test]
fn test_concurrent_updates() {
    let progress = Arc::new(AtomicProgress::new(100, 10000));

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let p = Arc::clone(&progress);
            thread::spawn(move || {
                for _ in 0..25 {
                    p.add_bytes(100);
                    p.file_done();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(progress.files_done(), 100);
    assert_eq!(progress.bytes_done(), 10000);
}

#[test]
fn test_concurrent_errors() {
    let progress = Arc::new(AtomicProgress::new(100, 10000));

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let p = Arc::clone(&progress);
            thread::spawn(move || {
                for _ in 0..10 {
                    p.add_error();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(progress.errors(), 40);
}
