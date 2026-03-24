//! Tests for erase state persistence module

use cache::ControlDb;

use super::*;

fn create_test_state() -> EraseState {
    let device = DeviceIdentifier {
        bsd_name: "disk22s1".to_string(),
        volume_uuid: Some("ABC-123".to_string()),
        serial_number: Some("SN12345".to_string()),
        size_bytes: 123_000_000_000,
        volume_name: Some("Ghost".to_string()),
        mount_point: Some("/Volumes/Ghost".to_string()),
    };

    EraseState::new(device, EraseLevel::Strong, Verify::Last, 1024 * 1024)
}

#[test]
fn test_state_creation() {
    let state = create_test_state();

    assert_eq!(state.version(), 1);
    assert_eq!(state.device().bsd_name, "disk22s1");
    assert_eq!(state.settings().level, "strong");
    assert_eq!(state.settings().total_passes, 7);
    assert_eq!(state.progress().current_stage, 0);
    assert_eq!(state.progress().stage_position, 0);
    assert!(state.progress().completed_stages.is_empty());
}

#[test]
fn test_progress_update() {
    let mut state = create_test_state();

    state.update_progress(2, 50_000_000_000, false, 250_000_000_000);

    assert_eq!(state.progress().current_stage, 2);
    assert_eq!(state.progress().stage_position, 50_000_000_000);
    assert!(!state.progress().in_verification);
    assert_eq!(state.progress().total_bytes_written, 250_000_000_000);
}

#[test]
fn test_seed_recording() {
    let mut state = create_test_state();
    let seed1 = [1u8; 32];
    let seed2 = [2u8; 32];

    state.record_seed(0, seed1);
    state.record_seed(6, seed2);

    assert_eq!(state.get_seed(0), Some(seed1));
    assert_eq!(state.get_seed(6), Some(seed2));
    assert_eq!(state.get_seed(3), None);

    // Recording again should replace
    let seed3 = [3u8; 32];
    state.record_seed(0, seed3);
    assert_eq!(state.get_seed(0), Some(seed3));
}

#[test]
fn test_stage_completion() {
    let mut state = create_test_state();

    assert!(!state.is_stage_complete(0));

    state.mark_stage_complete(0);
    assert!(state.is_stage_complete(0));
    assert!(!state.is_stage_complete(1));

    state.mark_stage_complete(1);
    state.mark_stage_complete(2);
    assert!(state.is_stage_complete(0));
    assert!(state.is_stage_complete(1));
    assert!(state.is_stage_complete(2));
}

#[test]
fn test_can_resume() {
    let state = create_test_state();

    // Matching device and level
    assert!(state.can_resume_for("disk22s1", 123_000_000_000, "strong"));

    // Wrong BSD name
    assert!(!state.can_resume_for("disk23s1", 123_000_000_000, "strong"));

    // Wrong size
    assert!(!state.can_resume_for("disk22s1", 100_000_000_000, "strong"));

    // Wrong level
    assert!(!state.can_resume_for("disk22s1", 123_000_000_000, "normal"));
}

#[test]
fn test_save_load_roundtrip() {
    let db = ControlDb::open_memory().unwrap();
    let mut state = create_test_state();

    state.mark_stage_complete(0);
    state.mark_stage_complete(1);
    state.mark_stage_complete(2);
    state.update_progress(3, 60_000_000_000, true, 400_000_000_000);
    state.record_seed(0, [42u8; 32]);

    // Save
    state.save(&db).unwrap();

    // Load
    let restored = EraseState::load(&db, "disk22s1").unwrap().unwrap();

    assert_eq!(restored.device().bsd_name, state.device().bsd_name);
    assert_eq!(restored.settings().level, state.settings().level);
    assert_eq!(restored.progress().current_stage, 3);
    assert_eq!(restored.progress().stage_position, 60_000_000_000);
    assert!(restored.progress().in_verification);
    assert_eq!(restored.get_seed(0), Some([42u8; 32]));
    assert!(restored.is_stage_complete(0));
    assert!(restored.is_stage_complete(1));
    assert!(restored.is_stage_complete(2));
    assert!(!restored.is_stage_complete(3));
}

#[test]
fn test_delete() {
    let db = ControlDb::open_memory().unwrap();
    let mut state = create_test_state();
    state.save(&db).unwrap();

    assert!(EraseState::load(&db, "disk22s1").unwrap().is_some());

    EraseState::delete(&db, "disk22s1").unwrap();
    assert!(EraseState::load(&db, "disk22s1").unwrap().is_none());
}

#[test]
fn test_find_by_volume_name_or_mount_point() {
    let db = ControlDb::open_memory().unwrap();
    let mut state = create_test_state();
    state.save(&db).unwrap();

    // Find by mount point
    let found = EraseState::find_by_volume_name_or_mount_point(&db, "/Volumes/Ghost")
        .unwrap()
        .unwrap();
    assert_eq!(found.device().bsd_name, "disk22s1");

    // Find by volume name
    let found = EraseState::find_by_volume_name_or_mount_point(&db, "Ghost").unwrap();
    // The ControlDb lookup strips /Volumes/ prefix, so "Ghost" matches volume_name
    assert!(found.is_some());
}

#[test]
fn test_summary() {
    let mut state = create_test_state();
    state.update_progress(2, 61_500_000_000, false, 0);
    state.mark_stage_complete(0);
    state.mark_stage_complete(1);
    // After mark_stage_complete resets position, re-set it
    state.update_progress(2, 61_500_000_000, false, 0);

    let summary = state.summary();
    assert!(summary.contains("Stage 3/7"));
    assert!(summary.contains("57.28 GB"));
}

#[test]
fn test_format_bytes() {
    assert_eq!(format_bytes(500), "500 bytes");
    assert_eq!(format_bytes(1536), "1.50 KB");
    assert_eq!(format_bytes(1_500_000), "1.43 MB");
    assert_eq!(format_bytes(1_500_000_000), "1.40 GB");
}

#[test]
fn test_device_has_new_label() {
    let state = create_test_state();

    // Original name was "Ghost"
    assert!(!state.device_has_new_label(Some("Ghost")));
    assert!(state.device_has_new_label(Some("NewName")));
    assert!(!state.device_has_new_label(None));
}

#[test]
fn test_device_has_new_label_no_original() {
    let device = DeviceIdentifier {
        bsd_name: "disk22s1".to_string(),
        volume_uuid: None,
        serial_number: None,
        size_bytes: 123_000_000_000,
        volume_name: None,
        mount_point: None,
    };
    let state = EraseState::new(device, EraseLevel::Quick, Verify::No, 1024 * 1024);

    assert!(!state.device_has_new_label(None));
    assert!(state.device_has_new_label(Some("NewVolume")));
}

#[test]
fn test_elapsed_seconds() {
    let state = create_test_state();
    let elapsed = state.elapsed_seconds();
    assert!(elapsed < 2, "Elapsed seconds should be < 2, got {elapsed}");
}

#[test]
fn test_is_recent() {
    let state = create_test_state();
    assert!(state.is_recent(1));
    assert!(state.is_recent(24));
}
