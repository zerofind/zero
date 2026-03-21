//! Tests for PathArena

use super::arena::{MAX_PATH_LEN, PathArena};

// -- basic operations ---

#[test]
fn push_and_get() {
    let mut arena = PathArena::new();
    let (off1, len1) = arena.push("Documents/report.pdf").unwrap();
    let (off2, len2) = arena.push("src/main.rs").unwrap();

    assert_eq!(arena.get(off1, len1), "Documents/report.pdf");
    assert_eq!(arena.get(off2, len2), "src/main.rs");
}

#[test]
fn empty_path() {
    let mut arena = PathArena::new();
    let (off, len) = arena.push("").unwrap();
    assert_eq!(len, 0);
    assert_eq!(arena.get(off, len), "");
}

#[test]
fn unicode_paths() {
    let mut arena = PathArena::new();
    let (off, len) = arena.push("文档/报告.pdf").unwrap();
    assert_eq!(arena.get(off, len), "文档/报告.pdf");
}

#[test]
fn name_extraction() {
    let mut arena = PathArena::new();
    let (off, len) = arena.push("a/b/c.txt").unwrap();
    let path = arena.get(off, len);
    let name = match path.rfind('/') {
        Some(pos) => &path[pos + 1..],
        None => path,
    };
    assert_eq!(name, "c.txt");
}

// -- capacity limits ---

#[test]
fn rejects_path_exceeding_max_len() {
    let mut arena = PathArena::new();
    let too_long = "x".repeat(MAX_PATH_LEN + 1);
    assert!(arena.push(&too_long).is_none());
}

#[test]
fn accepts_path_at_max_len() {
    let mut arena = PathArena::new();
    let exact = "x".repeat(MAX_PATH_LEN);
    let (off, len) = arena.push(&exact).unwrap();
    assert_eq!(arena.get(off, len).len(), MAX_PATH_LEN);
}

// -- free list and reuse ---

#[test]
fn reuse_freed_slot() {
    let mut arena = PathArena::new();
    let (off1, len1) = arena.push("hello_world.txt").unwrap();
    let bytes_after_first = arena.total_bytes();

    arena.remove(off1, len1);

    // Insert a shorter string — should reuse the freed slot
    let (off2, len2) = arena.push("short.txt").unwrap();
    assert_eq!(arena.get(off2, len2), "short.txt");

    // Arena shouldn't have grown (reused the freed slot)
    assert_eq!(arena.total_bytes(), bytes_after_first);
}

#[test]
fn coalesces_adjacent_free_regions() {
    let mut arena = PathArena::new();
    let (off1, len1) = arena.push("aaaa").unwrap();
    let (off2, len2) = arena.push("bbbb").unwrap();
    let (off3, len3) = arena.push("cccc").unwrap();

    // Remove middle then neighbors — should coalesce into one region
    arena.remove(off2, len2);
    assert_eq!(arena.free_list_len(), 1);

    arena.remove(off1, len1);
    assert_eq!(arena.free_list_len(), 1, "should coalesce left neighbor");

    arena.remove(off3, len3);
    assert_eq!(arena.free_list_len(), 1, "should coalesce right neighbor");

    // The merged region should be large enough to hold all 12 bytes
    let (reuse_off, reuse_len) = arena.push("xxxxxxxxxxxx").unwrap();
    assert_eq!(arena.get(reuse_off, reuse_len), "xxxxxxxxxxxx");
    // Should have reused, not grown
    assert_eq!(arena.total_bytes(), 12);
}

// -- many inserts ---

#[test]
fn many_inserts() {
    let mut arena = PathArena::new();
    let mut refs = Vec::new();

    for i in 0..10_000 {
        let path = format!("dir{}/file{}.txt", i % 100, i);
        let (off, len) = arena.push(&path).unwrap();
        refs.push((off, len, path));
    }

    for (off, len, expected) in &refs {
        assert_eq!(arena.get(*off, *len), expected);
    }
}

#[test]
fn churn_does_not_grow_free_list_unboundedly() {
    let mut arena = PathArena::new();
    let mut refs = Vec::new();

    // Push 1000 entries
    for i in 0..1_000 {
        let path = format!("dir/file{i:04}.txt");
        refs.push(arena.push(&path).unwrap());
    }

    // Remove all, then re-add — free list should coalesce
    for (off, len) in refs.drain(..) {
        arena.remove(off, len);
    }

    // After coalescing all adjacent regions, free list should be small
    assert!(
        arena.free_list_len() <= 10,
        "free list should coalesce: got {} entries",
        arena.free_list_len()
    );
}

#[test]
fn shrink_to_fit() {
    let mut arena = PathArena::with_capacity(1_000_000);
    arena.push("small.txt").unwrap();
    arena.shrink_to_fit();
    // Just verify it doesn't panic — actual capacity is implementation detail
}
