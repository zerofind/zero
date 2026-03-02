//! Tests for the automation executor

use super::*;
use crate::cache::{NewAutomation, PathMapping, RunStatus, Settings, Triggers};
use std::path::PathBuf;
use tempfile::TempDir;

fn setup_test_executor() -> (Executor, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db = CacheDb::open_memory().unwrap();
    let executor = Executor::with_db(db, ExecutorConfig::default());
    (executor, temp_dir)
}

#[tokio::test]
async fn test_create_executor() {
    let db = CacheDb::open_memory().unwrap();
    let _executor = Executor::with_db(db, ExecutorConfig::default());
}

#[tokio::test]
async fn test_handle_usb_mount_no_automations() {
    let (executor, _temp) = setup_test_executor();

    let run_ids = executor
        .handle_event(AutomationEvent::UsbMounted {
            serial: "UNKNOWN".to_string(),
            mount_point: PathBuf::from("/Volumes/Test"),
            volume_name: Some("Test".to_string()),
        })
        .await
        .unwrap();

    assert!(run_ids.is_empty());
}

#[tokio::test]
async fn test_handle_usb_mount_with_automation() {
    let (executor, temp_dir) = setup_test_executor();

    // Create source directory with a file
    let source_dir = temp_dir.path().join("source");
    std::fs::create_dir(&source_dir).unwrap();
    std::fs::write(source_dir.join("test.txt"), "hello").unwrap();

    // Create destination directory
    let dest_dir = temp_dir.path().join("dest");
    std::fs::create_dir(&dest_dir).unwrap();

    // Create an automation
    let _automation = executor.db().create_automation(
        NewAutomation {
            name: "Test Backup".to_string(),
            dest_device_serial: Some("TEST123".to_string()),
            dest_volume_name: Some("TestDrive".to_string()),
            dest_path: Some(dest_dir.to_string_lossy().to_string()),
            triggers: Triggers {
                on_mount: true,
                on_change: false,
                on_schedule: None,
            },
            paths: vec![PathMapping {
                source: source_dir.to_string_lossy().to_string(),
                dest: "backup".to_string(),
                exclude: vec![],
            }],
            settings: Settings {
                verify: false,
                delete_orphans: false,
                notify: false,
                debounce_ms: 5000,
            },
        },
    )
    .unwrap();

    // Trigger USB mount
    let run_ids = executor
        .handle_event(AutomationEvent::UsbMounted {
            serial: "TEST123".to_string(),
            mount_point: dest_dir.clone(),
            volume_name: Some("TestDrive".to_string()),
        })
        .await
        .unwrap();

    assert_eq!(run_ids.len(), 1);

    // Check run completed
    let run = executor.db().get_run(run_ids[0]).unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Success);

    // Check file was synced
    assert!(dest_dir.join("backup").join("test.txt").exists());
}

#[tokio::test]
async fn test_debounce_file_changes() {
    let (executor, _temp) = setup_test_executor();

    // First event should not be debounced
    let result1 = executor
        .handle_event(AutomationEvent::FilesChanged {
            watch_root: PathBuf::from("/test"),
            changed_paths: vec![PathBuf::from("/test/file.txt")],
        })
        .await;
    assert!(result1.is_ok());

    // Immediate second event should be debounced
    let result2 = executor
        .handle_event(AutomationEvent::FilesChanged {
            watch_root: PathBuf::from("/test"),
            changed_paths: vec![PathBuf::from("/test/file.txt")],
        })
        .await
        .unwrap();

    // Should return empty (debounced)
    assert!(result2.is_empty());
}

#[tokio::test]
async fn test_recover_interrupted_runs() {
    let (executor, _temp) = setup_test_executor();

    // Create an automation
    let automation = executor.db().create_automation(
        NewAutomation {
            name: "Test".to_string(),
            dest_device_serial: None,
            dest_volume_name: None,
            dest_path: None,
            triggers: Triggers::default(),
            paths: vec![],
            settings: Settings::default(),
        },
    )
    .unwrap();

    // Start a run (simulating a running state)
    let run = executor.db().start_run(
        automation.id,
        crate::cache::runs::TriggerType::Manual,
        None,
    )
    .unwrap();

    assert_eq!(run.status, RunStatus::Running);

    // Recover interrupted runs
    let count = executor.recover_interrupted_runs().await.unwrap();
    assert_eq!(count, 1);

    // Check run is now partial
    let updated = executor.db().get_run(run.id).unwrap().unwrap();
    assert_eq!(updated.status, RunStatus::Partial);
    assert!(updated.resumable);
}

#[tokio::test]
async fn test_manual_trigger() {
    let (executor, temp_dir) = setup_test_executor();

    // Create source and dest
    let source_dir = temp_dir.path().join("source");
    let dest_dir = temp_dir.path().join("dest");
    std::fs::create_dir(&source_dir).unwrap();
    std::fs::create_dir(&dest_dir).unwrap();
    std::fs::write(source_dir.join("file.txt"), "content").unwrap();

    // Create automation with direct dest_path
    let automation = executor.db().create_automation(
        NewAutomation {
            name: "Manual Test".to_string(),
            dest_device_serial: None,
            dest_volume_name: None,
            dest_path: Some(dest_dir.to_string_lossy().to_string()),
            triggers: Triggers::default(),
            paths: vec![PathMapping {
                source: source_dir.to_string_lossy().to_string(),
                dest: "".to_string(),
                exclude: vec![],
            }],
            settings: Settings {
                verify: false,
                delete_orphans: false,
                notify: false,
                debounce_ms: 5000,
            },
        },
    )
    .unwrap();

    // Run manually
    let run_id = executor
        .handle_event(AutomationEvent::Manual {
            automation_id: automation.id,
        })
        .await
        .unwrap();

    assert_eq!(run_id.len(), 1);

    // Check file synced
    assert!(dest_dir.join("file.txt").exists());
}
