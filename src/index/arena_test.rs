//! Tests for PathArena

use super::arena::PathArena;

#[test]
fn push_and_get() {
    let mut arena = PathArena::new();
    let (off1, len1) = arena.push("Documents/report.pdf");
    let (off2, len2) = arena.push("src/main.rs");

    assert_eq!(arena.get(off1, len1), "Documents/report.pdf");
    assert_eq!(arena.get(off2, len2), "src/main.rs");
}

#[test]
fn reuse_freed_slot() {
    let mut arena = PathArena::new();
    let (off1, len1) = arena.push("hello_world.txt");
    let bytes_after_first = arena.total_bytes();

    // Remove the first entry
    arena.remove(off1, len1);

    // Insert a shorter string — should reuse the freed slot
    let (off2, len2) = arena.push("short.txt");
    assert_eq!(arena.get(off2, len2), "short.txt");

    // Arena shouldn't have grown (reused the freed slot)
    assert_eq!(arena.total_bytes(), bytes_after_first);
}

#[test]
fn name_extraction() {
    let mut arena = PathArena::new();
    let (off, len) = arena.push("a/b/c.txt");
    let path = arena.get(off, len);
    let name = match path.rfind('/') {
        Some(pos) => &path[pos + 1..],
        None => path,
    };
    assert_eq!(name, "c.txt");
}

#[test]
fn large_paths() {
    let mut arena = PathArena::new();
    let long_path = "a/".repeat(500) + "file.txt";
    let (off, len) = arena.push(&long_path);
    assert_eq!(arena.get(off, len), &long_path[..len as usize]);
}

#[test]
fn empty_path() {
    let mut arena = PathArena::new();
    let (off, len) = arena.push("");
    assert_eq!(len, 0);
    assert_eq!(arena.get(off, len), "");
}

#[test]
fn unicode_paths() {
    let mut arena = PathArena::new();
    let (off, len) = arena.push("文档/报告.pdf");
    assert_eq!(arena.get(off, len), "文档/报告.pdf");
}

#[test]
fn many_inserts() {
    let mut arena = PathArena::new();
    let mut refs = Vec::new();

    for i in 0..10_000 {
        let path = format!("dir{}/file{}.txt", i % 100, i);
        let (off, len) = arena.push(&path);
        refs.push((off, len, path));
    }

    // Verify all paths can be read back
    for (off, len, expected) in &refs {
        assert_eq!(arena.get(*off, *len), expected);
    }
}

#[test]
fn shrink_to_fit() {
    let mut arena = PathArena::with_capacity(1_000_000);
    arena.push("small.txt");
    arena.shrink_to_fit();
    // Just verify it doesn't panic — actual capacity is implementation detail
}
