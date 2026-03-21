//! Tests for erase module public API

use super::*;

#[test]
fn test_erase_options_defaults() {
    let opts = EraseOptions::default();
    assert!(matches!(opts.level, EraseLevel::Normal));
    assert!(matches!(opts.verify, Verify::Last));
    assert_eq!(opts.block_size, 1024 * 1024);
    assert_eq!(opts.retries, 8);
}

#[test]
fn test_erase_options_builder() {
    let opts = EraseOptions::new(EraseLevel::Strong)
        .with_verify(Verify::All)
        .with_block_size(512 * 1024)
        .with_retries(3)
        .with_force(true);

    assert!(matches!(opts.level, EraseLevel::Strong));
    assert!(matches!(opts.verify, Verify::All));
    assert_eq!(opts.block_size, 512 * 1024);
    assert_eq!(opts.retries, 3);
    assert!(opts.force);
}

#[test]
fn test_block_size_normalization() {
    // Should round up to power of 2
    let opts = EraseOptions::new(EraseLevel::Quick).with_block_size(5000);
    assert_eq!(opts.block_size, 8192);

    // Should enforce minimum
    let opts = EraseOptions::new(EraseLevel::Quick).with_block_size(100);
    assert_eq!(opts.block_size, 4096);
}

#[test]
fn test_erase_target_size_display() {
    let target = EraseTarget {
        device_path: "/dev/rdisk4".to_string(),
        name: "Test".to_string(),
        size_bytes: 64_000_000_000,
        is_flash: true,
        mount_point: None,
        warnings: vec![],
    };
    assert_eq!(target.size_display(), "64.0 GB");

    let target_small = EraseTarget {
        size_bytes: 500_000_000,
        ..target
    };
    assert_eq!(target_small.size_display(), "500.0 MB");
}
