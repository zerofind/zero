//! Tests for etch-backed control state persistence

use etchdb::Store;
use tempfile::TempDir;

use super::*;
use crate::automations::{NewAutomation, PathMapping, Settings, Triggers};
use crate::runs::TriggerType;
use crate::storages::DeviceInfo;

#[test]
fn storage_create_and_lookup() {
    let store = Store::<ControlState>::memory();

    let storage = {
        let mut state = store.read().clone();
        let s = state.get_or_create_storage(
            std::path::Path::new("/Volumes/MyDrive"),
            Some(DeviceInfo::usb("ABC123", Some("My USB".into()))),
        );
        // Write to store
        store
            .write(|tx| {
                tx.put_storage(&s);
                Ok(())
            })
            .unwrap();
        s
    };

    let state = store.read();
    assert!(state.get_storage_by_id(storage.id).is_some());
    assert!(state.get_storage_by_serial("ABC123").is_some());
    assert_eq!(state.list_storages().len(), 1);
}

#[test]
fn automation_crud() {
    let store = Store::<ControlState>::memory();

    let automation = {
        let mut state = store.read().clone();
        let a = state.create_automation(NewAutomation {
            name: "Backup photos".into(),
            dest_device_serial: Some("USB123".into()),
            dest_volume_name: Some("My USB".into()),
            dest_path: None,
            triggers: Triggers {
                on_mount: true,
                ..Default::default()
            },
            paths: vec![PathMapping {
                source: "/Users/me/Photos".into(),
                dest: "Photos".into(),
                exclude: vec![],
            }],
            settings: Settings::default(),
        });
        store
            .write(|tx| {
                tx.put_automation(&a);
                Ok(())
            })
            .unwrap();
        a
    };

    let state = store.read();
    assert_eq!(state.list_automations().len(), 1);
    assert_eq!(
        state.get_automation(automation.id).unwrap().name,
        "Backup photos"
    );
    assert_eq!(state.find_automations_by_serial("USB123").len(), 1);
}

#[test]
fn run_lifecycle() {
    let store = Store::<ControlState>::memory();

    // Create automation first
    let auto_id = {
        let mut state = store.read().clone();
        let a = state.create_automation(NewAutomation {
            name: "Test".into(),
            dest_device_serial: None,
            dest_volume_name: None,
            dest_path: Some("/dest".into()),
            triggers: Triggers::default(),
            paths: vec![],
            settings: Settings::default(),
        });
        store
            .write(|tx| {
                tx.put_automation(&a);
                Ok(())
            })
            .unwrap();
        a.id
    };

    // Start a run
    let run = {
        let mut state = store.read().clone();
        let r = state.start_run(auto_id, TriggerType::Manual, None);
        store
            .write(|tx| {
                tx.put_run(&r);
                Ok(())
            })
            .unwrap();
        r
    };

    let state = store.read();
    let r = state.get_run(run.id).unwrap();
    assert_eq!(r.status, crate::runs::RunStatus::Running);
    assert_eq!(r.automation_id, auto_id);
}

#[test]
fn wal_replay() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("control");

    {
        let store = open_control_store(&path).unwrap();
        store
            .write(|tx| {
                tx.put_storage(&Storage {
                    id: 1,
                    device_serial: Some("SER1".into()),
                    device_uuid: None,
                    path_hash: None,
                    display_name: Some("Drive".into()),
                    last_mount_path: Some("/mnt".into()),
                    mount_type: None,
                    db_filename: "serial_SER1.db".into(),
                    file_count: 0,
                    total_bytes: 0,
                    last_seen_at: None,
                    created_at: 1700000000,
                });
                tx.put_automation(&Automation {
                    id: 1,
                    name: "Backup".into(),
                    dest_device_serial: Some("SER1".into()),
                    dest_volume_name: None,
                    dest_path: None,
                    triggers: Triggers {
                        on_mount: true,
                        ..Default::default()
                    },
                    paths: vec![],
                    settings: Settings::default(),
                    enabled: true,
                    created_at: 1700000000,
                    updated_at: 1700000000,
                });
                Ok(())
            })
            .unwrap();
    }

    // Reopen — WAL replay
    {
        let store = open_control_store(&path).unwrap();
        let state = store.read();
        assert!(state.get_storage_by_id(1).is_some());
        assert!(state.get_automation(1).is_some());
        assert_eq!(state.list_storages().len(), 1);
        assert_eq!(state.list_automations().len(), 1);
    }
}

#[test]
fn snapshot_roundtrip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("control");

    {
        let store = open_control_store(&path).unwrap();
        store
            .write(|tx| {
                tx.put_storage(&Storage {
                    id: 1,
                    device_serial: Some("X".into()),
                    device_uuid: None,
                    path_hash: None,
                    display_name: None,
                    last_mount_path: None,
                    mount_type: None,
                    db_filename: "serial_X.db".into(),
                    file_count: 100,
                    total_bytes: 1024,
                    last_seen_at: None,
                    created_at: 1700000000,
                });
                Ok(())
            })
            .unwrap();
        store.flush().unwrap();
    }

    // Reopen from snapshot
    {
        let store = open_control_store(&path).unwrap();
        let state = store.read();
        let s = state.get_storage_by_id(1).unwrap();
        assert_eq!(s.file_count, 100);
        assert_eq!(s.total_bytes, 1024);
    }
}

#[test]
fn id_counters_survive_replay() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("control");

    {
        let store = open_control_store(&path).unwrap();
        store
            .write(|tx| {
                tx.put_storage(&Storage {
                    id: 5,
                    device_serial: None,
                    device_uuid: None,
                    path_hash: Some("abc".into()),
                    display_name: None,
                    last_mount_path: None,
                    mount_type: None,
                    db_filename: "path_abc.db".into(),
                    file_count: 0,
                    total_bytes: 0,
                    last_seen_at: None,
                    created_at: 1700000000,
                });
                Ok(())
            })
            .unwrap();
    }

    // Reopen — next_storage_id should be >= 5
    {
        let store = open_control_store(&path).unwrap();
        let mut state = store.read().clone();
        let new = state.get_or_create_storage(std::path::Path::new("/new"), None);
        // New ID must be > 5
        assert!(new.id > 5);
    }
}
