//! Tests for UsageStore

use super::usage::UsageStore;

#[test]
fn test_record_open_new_entry() {
    let mut store = UsageStore::new();
    let is_new = store.record_open("/Users/me/doc.pdf", 1000);
    assert!(is_new);
    assert_eq!(store.len(), 1);
}

#[test]
fn test_record_open_increment() {
    let mut store = UsageStore::new();
    store.record_open("/Users/me/doc.pdf", 1000);
    let is_new = store.record_open("/Users/me/doc.pdf", 2000);
    assert!(!is_new);
    assert_eq!(store.len(), 1);
    // Bonus should be higher with more opens
    let bonus = store.frequency_bonus("/Users/me/doc.pdf", 2000);
    assert!(bonus > 0);
}

#[test]
fn test_frequency_bonus_recent_high() {
    let mut store = UsageStore::new();
    let now = 1_700_000_000;
    for _ in 0..10 {
        store.record_open("/Users/me/doc.pdf", now);
    }
    let bonus = store.frequency_bonus("/Users/me/doc.pdf", now);
    assert!(
        bonus > 50,
        "frequent + recent should have high bonus: {bonus}"
    );
}

#[test]
fn test_frequency_bonus_old_low() {
    let mut store = UsageStore::new();
    let open_time = 1_700_000_000;
    store.record_open("/Users/me/doc.pdf", open_time);
    // Check bonus 365 days later
    let now = open_time + 365 * 86400;
    let bonus = store.frequency_bonus("/Users/me/doc.pdf", now);
    assert!(bonus < 10, "old single open should have low bonus: {bonus}");
}

#[test]
fn test_frequency_bonus_unknown_zero() {
    let store = UsageStore::new();
    let bonus = store.frequency_bonus("/nonexistent/file.txt", 1_700_000_000);
    assert_eq!(bonus, 0);
}

#[test]
fn test_prune_max_entries() {
    let mut store = UsageStore::new();
    let now = 1_700_000_000;
    // Insert more than MAX_ENTRIES
    for i in 0..10_050 {
        store.record_open(&format!("/file_{}", i), now);
    }
    // Should have been pruned during insert
    assert!(
        store.len() <= 10_000,
        "should be pruned to MAX_ENTRIES, got {}",
        store.len()
    );
}
