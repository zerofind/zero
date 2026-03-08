use super::{BrowserEntry, sort_entries};
use crate::models::{SortDirection, SortField};

fn entry(name: &str, is_dir: bool) -> BrowserEntry {
    BrowserEntry {
        name: name.to_string(),
        path: std::path::PathBuf::from(name),
        size: 0,
        mtime: 0,
        is_dir,
        extension: None,
        depth: 0,
        expanded: false,
        mode: None,
        has_xattrs: false,
        is_symlink: false,
        symlink_target: None,
        flags: None,
        owner: None,
    }
}

#[test]
fn natural_sort_numbers_in_names() {
    let mut entries = vec![
        entry("file10.txt", false),
        entry("file2.txt", false),
        entry("file1.txt", false),
        entry("file20.txt", false),
    ];

    sort_entries(&mut entries, SortField::Name, SortDirection::Ascending);

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["file1.txt", "file2.txt", "file10.txt", "file20.txt"]
    );
}

#[test]
fn natural_sort_case_insensitive() {
    let mut entries = vec![
        entry("Banana", false),
        entry("apple", false),
        entry("cherry", false),
    ];

    sort_entries(&mut entries, SortField::Name, SortDirection::Ascending);

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["apple", "Banana", "cherry"]);
}

#[test]
fn natural_sort_directories_first() {
    let mut entries = vec![
        entry("zebra.txt", false),
        entry("alpha", true),
        entry("aardvark.txt", false),
        entry("beta", true),
    ];

    sort_entries(&mut entries, SortField::Name, SortDirection::Ascending);

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "beta", "aardvark.txt", "zebra.txt"]);
}

#[test]
fn natural_sort_descending() {
    let mut entries = vec![
        entry("file1.txt", false),
        entry("file10.txt", false),
        entry("file2.txt", false),
    ];

    sort_entries(&mut entries, SortField::Name, SortDirection::Descending);

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["file10.txt", "file2.txt", "file1.txt"]);
}
