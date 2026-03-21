//! Tests for wipe engine module

use super::*;

#[test]
fn test_verify_from_str() {
    assert_eq!(Verify::parse_str("no"), Some(Verify::No));
    assert_eq!(Verify::parse_str("none"), Some(Verify::No));
    assert_eq!(Verify::parse_str("last"), Some(Verify::Last));
    assert_eq!(Verify::parse_str("all"), Some(Verify::All));
    assert_eq!(Verify::parse_str("invalid"), None);
}

#[test]
fn test_progress_percent() {
    let progress = EraseProgress {
        stage: 0,
        total_stages: 2,
        pass: 1,
        total_passes: 2,
        verifying: false,
        bytes_done: 500,
        bytes_total: 1000,
        speed_bps: 100_000_000,
        eta_seconds: 5,
        bad_blocks: 0,
        description: "Test".to_string(),
    };

    assert_eq!(progress.percent(), 50);
}

#[test]
fn test_progress_overall() {
    let progress = EraseProgress {
        stage: 1,
        total_stages: 4,
        pass: 2,
        total_passes: 4,
        verifying: false,
        bytes_done: 500,
        bytes_total: 1000,
        speed_bps: 0,
        eta_seconds: 0,
        bad_blocks: 0,
        description: "Test".to_string(),
    };

    // Stage 1 (0-indexed) + 50% of current = 1.5 / 4 = 0.375
    let expected = 0.375;
    let actual = progress.overall_progress();
    assert!((actual - expected).abs() < 0.001);
}

#[test]
fn test_speed_display() {
    let mut progress = EraseProgress {
        stage: 0,
        total_stages: 1,
        pass: 1,
        total_passes: 1,
        verifying: false,
        bytes_done: 0,
        bytes_total: 1000,
        speed_bps: 500_000,
        eta_seconds: 0,
        bad_blocks: 0,
        description: "Test".to_string(),
    };

    assert_eq!(progress.speed_display(), "500 KB/s");

    progress.speed_bps = 150_000_000;
    assert_eq!(progress.speed_display(), "150.0 MB/s");

    progress.speed_bps = 1_500_000_000;
    assert_eq!(progress.speed_display(), "1.5 GB/s");
}

#[test]
fn test_eta_display() {
    let mut progress = EraseProgress {
        stage: 0,
        total_stages: 1,
        pass: 1,
        total_passes: 1,
        verifying: false,
        bytes_done: 0,
        bytes_total: 1000,
        speed_bps: 0,
        eta_seconds: 45,
        bad_blocks: 0,
        description: "Test".to_string(),
    };

    assert_eq!(progress.eta_display(), "45s");

    progress.eta_seconds = 125;
    assert_eq!(progress.eta_display(), "2m 5s");

    progress.eta_seconds = 3725;
    assert_eq!(progress.eta_display(), "1h 2m");
}

#[test]
fn test_wipe_state() {
    let mut state = WipeState::new();

    assert!(state.bad_blocks.is_empty());
    assert_eq!(state.position, 0);

    state.position = 4096;
    assert_eq!(state.block_number(4096), 1);

    state.mark_bad_block(4096);
    assert!(state.is_bad_block(4096));

    state.position = 8192;
    assert!(!state.is_bad_block(4096));
}
