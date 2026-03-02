//! Tests for volume information

use super::*;
use std::path::Path;

#[test]
fn test_get_root_volume() {
    let volume = VolumeInfo::for_path(Path::new("/")).unwrap();
    assert!(!volume.name.is_empty());
    assert!(volume.size_bytes > 0);
    assert!(volume.physical_drive.is_internal);
}

#[test]
fn test_all_volumes() {
    let volumes = VolumeInfo::all().unwrap();
    assert!(!volumes.is_empty());

    // Should have at least the root volume
    let has_root = volumes.iter().any(|v| v.mount_point == "/");
    assert!(has_root);
}

#[test]
fn test_usage_calculation() {
    let volume = VolumeInfo::for_path(Path::new("/")).unwrap();
    let used = volume.used_bytes();
    let pct = volume.usage_percent();

    assert!(used <= volume.size_bytes);
    assert!(pct >= 0.0 && pct <= 100.0);
}

#[test]
fn test_volume_info_methods() {
    let volume = VolumeInfo::for_path(Path::new("/")).unwrap();

    // Root volume should be internal, not USB
    assert!(!volume.is_external());
    assert!(!volume.is_usb());
}

#[test]
fn test_usage_percent_zero_size() {
    // Create a mock volume with zero size to test edge case
    let volume = VolumeInfo {
        name: "Test".to_string(),
        bsd_name: "disk0".to_string(),
        file_system: "APFS".to_string(),
        size_bytes: 0,
        free_bytes: 0,
        mount_point: "/test".to_string(),
        volume_uuid: "test-uuid".to_string(),
        writable: true,
        ignore_ownership: false,
        physical_drive: PhysicalDrive {
            device_name: "Test Drive".to_string(),
            media_name: "Test Media".to_string(),
            protocol: "SATA".to_string(),
            is_internal: true,
            medium_type: Some("ssd".to_string()),
            partition_map_type: None,
            smart_status: None,
        },
    };

    assert_eq!(volume.usage_percent(), 0.0);
    assert_eq!(volume.used_bytes(), 0);
}

#[test]
fn test_volume_by_name_not_found() {
    let result = VolumeInfo::by_name("NonExistentVolume12345");
    assert!(result.is_err());
}

#[test]
fn test_physical_drive_external() {
    let volume = VolumeInfo {
        name: "External".to_string(),
        bsd_name: "disk5s1".to_string(),
        file_system: "exFAT".to_string(),
        size_bytes: 1_000_000_000,
        free_bytes: 500_000_000,
        mount_point: "/Volumes/External".to_string(),
        volume_uuid: "ext-uuid".to_string(),
        writable: true,
        ignore_ownership: true,
        physical_drive: PhysicalDrive {
            device_name: "USB Drive".to_string(),
            media_name: "USB Storage".to_string(),
            protocol: "USB".to_string(),
            is_internal: false,
            medium_type: None,
            partition_map_type: Some("GPT".to_string()),
            smart_status: None,
        },
    };

    assert!(volume.is_external());
    assert!(volume.is_usb());
    assert_eq!(volume.used_bytes(), 500_000_000);
    assert!((volume.usage_percent() - 50.0).abs() < 0.01);
}
