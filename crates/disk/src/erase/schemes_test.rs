//! Tests for erasure schemes module

use super::*;

#[test]
fn test_erase_level_defaults_to_normal() {
    let level = EraseLevel::default();
    assert_eq!(level, EraseLevel::Normal);
}

#[test]
fn test_erase_level_pass_counts() {
    assert_eq!(EraseLevel::Quick.passes(), 1);
    assert_eq!(EraseLevel::Normal.passes(), 2);
    assert_eq!(EraseLevel::Strong.passes(), 7);
}

#[test]
fn test_erase_level_from_str() {
    assert_eq!(EraseLevel::parse_str("quick"), Some(EraseLevel::Quick));
    assert_eq!(EraseLevel::parse_str("QUICK"), Some(EraseLevel::Quick));
    assert_eq!(EraseLevel::parse_str("fast"), Some(EraseLevel::Quick));

    assert_eq!(EraseLevel::parse_str("normal"), Some(EraseLevel::Normal));
    assert_eq!(EraseLevel::parse_str("default"), Some(EraseLevel::Normal));

    assert_eq!(EraseLevel::parse_str("strong"), Some(EraseLevel::Strong));
    assert_eq!(EraseLevel::parse_str("secure"), Some(EraseLevel::Strong));
    assert_eq!(EraseLevel::parse_str("max"), Some(EraseLevel::Strong));

    assert_eq!(EraseLevel::parse_str("invalid"), None);
}

#[test]
fn test_scheme_zero() {
    let scheme = Scheme::zero();
    assert_eq!(scheme.pass_count(), 1);
}

#[test]
fn test_scheme_random2x() {
    let scheme = Scheme::random2x();
    assert_eq!(scheme.pass_count(), 2);
}

#[test]
fn test_scheme_vsitr() {
    let scheme = Scheme::vsitr();
    assert_eq!(scheme.pass_count(), 7);
}

#[test]
fn test_scheme_dod() {
    let scheme = Scheme::dod();
    assert_eq!(scheme.pass_count(), 3);
}

#[test]
fn test_scheme_gutmann() {
    let scheme = Scheme::gutmann();
    assert_eq!(scheme.pass_count(), 35);
}

#[test]
fn test_time_estimate() {
    let scheme = Scheme::zero();
    // 1 GB at 100 MB/s = 10 seconds
    assert_eq!(scheme.estimate_time(1_000_000_000, 100), 10);

    let scheme2x = Scheme::random2x();
    // 2 passes = 20 seconds
    assert_eq!(scheme2x.estimate_time(1_000_000_000, 100), 20);
}

#[test]
fn test_time_display() {
    let scheme = Scheme::zero();

    assert_eq!(
        scheme.estimate_time_display(1_000_000_000, 100),
        "10 seconds"
    );
    assert_eq!(
        scheme.estimate_time_display(10_000_000_000, 100),
        "1 minutes"
    );
    assert_eq!(
        scheme.estimate_time_display(400_000_000_000, 100),
        "1 hours 6 minutes"
    );
}

#[test]
fn test_all_levels() {
    let levels = EraseLevel::all();
    assert_eq!(levels.len(), 3);
    assert!(levels.contains(&EraseLevel::Quick));
    assert!(levels.contains(&EraseLevel::Normal));
    assert!(levels.contains(&EraseLevel::Strong));
}

#[test]
fn test_scheme_display() {
    let scheme = Scheme::vsitr();
    assert_eq!(format!("{scheme}"), "VSITR 7-Pass (7 passes)");
}

#[test]
fn test_level_display() {
    assert_eq!(format!("{}", EraseLevel::Quick), "quick");
    assert_eq!(format!("{}", EraseLevel::Normal), "normal");
    assert_eq!(format!("{}", EraseLevel::Strong), "strong");
}
