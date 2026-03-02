use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use tempfile::TempDir;

use super::{PermissionCompareResult, compare_permissions, sync_dir_permissions};

/// Create a test directory structure with specific permissions
fn create_test_structure(root: &Path, mode: u32) {
    // Create directories
    let sub_dir = root.join("subdir");
    fs::create_dir_all(&sub_dir).unwrap();

    let nested_dir = sub_dir.join("nested");
    fs::create_dir_all(&nested_dir).unwrap();

    // Create files
    File::create(root.join("file1.txt")).unwrap();
    File::create(sub_dir.join("file2.txt")).unwrap();
    File::create(nested_dir.join("file3.txt")).unwrap();

    // Set permissions
    set_mode(&root.join("file1.txt"), mode);
    set_mode(&sub_dir.join("file2.txt"), mode);
    set_mode(&nested_dir.join("file3.txt"), mode);
    set_mode(&sub_dir, mode | 0o111); // directories need execute
    set_mode(&nested_dir, mode | 0o111);
}

fn set_mode(path: &Path, mode: u32) {
    let perms = fs::Permissions::from_mode(mode);
    fs::set_permissions(path, perms).unwrap();
}

fn get_mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

#[test]
fn test_compare_permissions_identical() {
    let source = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();

    create_test_structure(source.path(), 0o644);
    create_test_structure(dest.path(), 0o644);

    let result = compare_permissions(source.path(), dest.path()).unwrap();

    assert!(
        result.mismatches.is_empty(),
        "Expected no mismatches for identical permissions"
    );
    assert!(result.checked > 0, "Expected some files to be checked");
}

#[test]
fn test_compare_permissions_different() {
    let source = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();

    create_test_structure(source.path(), 0o644);
    create_test_structure(dest.path(), 0o600);

    let result = compare_permissions(source.path(), dest.path()).unwrap();

    assert!(
        !result.mismatches.is_empty(),
        "Expected mismatches for different permissions"
    );

    // All files should have mismatches
    for mismatch in &result.mismatches {
        assert_ne!(
            mismatch.source_mode, mismatch.dest_mode,
            "Mismatch should have different modes"
        );
    }
}

#[test]
fn test_compare_permissions_partial_mismatch() {
    let source = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();

    create_test_structure(source.path(), 0o644);
    create_test_structure(dest.path(), 0o644);

    // Change only one file in destination
    set_mode(&dest.path().join("file1.txt"), 0o600);

    let result = compare_permissions(source.path(), dest.path()).unwrap();

    assert_eq!(result.mismatches.len(), 1, "Expected exactly one mismatch");
    assert!(
        result.mismatches[0].path.contains("file1.txt"),
        "Expected mismatch to be file1.txt"
    );
}

#[test]
fn test_compare_permissions_missing_dest_file() {
    let source = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();

    create_test_structure(source.path(), 0o644);
    create_test_structure(dest.path(), 0o644);

    // Remove a file from destination
    fs::remove_file(dest.path().join("file1.txt")).unwrap();

    let result = compare_permissions(source.path(), dest.path()).unwrap();

    // Should not report missing file as mismatch
    for mismatch in &result.mismatches {
        assert!(
            !mismatch.path.contains("file1.txt"),
            "Should not report missing file as mismatch"
        );
    }
}

#[test]
fn test_compare_permissions_empty_directories() {
    let source = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();

    // Empty directories - nothing to compare
    let result = compare_permissions(source.path(), dest.path()).unwrap();

    assert_eq!(result.checked, 0, "No files should be checked");
    assert!(result.mismatches.is_empty(), "No mismatches expected");
}

#[test]
fn test_sync_dir_permissions() {
    let source = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();

    // Create directories in both
    let src_sub = source.path().join("subdir");
    let dest_sub = dest.path().join("subdir");
    fs::create_dir_all(&src_sub).unwrap();
    fs::create_dir_all(&dest_sub).unwrap();

    // Set different permissions
    set_mode(&src_sub, 0o755);
    set_mode(&dest_sub, 0o700);

    // Verify they're different before sync
    assert_ne!(get_mode(&src_sub), get_mode(&dest_sub));

    // Sync permissions
    let result = sync_dir_permissions(source.path(), dest.path()).unwrap();

    assert!(result.dirs_synced > 0, "Expected at least one dir synced");
    assert_eq!(result.errors, 0, "Expected no errors");

    // Verify permissions match after sync
    assert_eq!(get_mode(&src_sub), get_mode(&dest_sub));
}

#[test]
fn test_sync_dir_permissions_nested() {
    let source = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();

    // Create nested directories
    let src_nested = source.path().join("a/b/c");
    let dest_nested = dest.path().join("a/b/c");
    fs::create_dir_all(&src_nested).unwrap();
    fs::create_dir_all(&dest_nested).unwrap();

    // Set different permissions at each level
    set_mode(&source.path().join("a"), 0o755);
    set_mode(&source.path().join("a/b"), 0o750);
    set_mode(&src_nested, 0o700);

    set_mode(&dest.path().join("a"), 0o777);
    set_mode(&dest.path().join("a/b"), 0o777);
    set_mode(&dest_nested, 0o777);

    // Sync
    let result = sync_dir_permissions(source.path(), dest.path()).unwrap();

    assert_eq!(result.dirs_synced, 3, "Expected 3 directories synced");
    assert_eq!(result.errors, 0, "Expected no errors");

    // Verify all match
    assert_eq!(
        get_mode(&source.path().join("a")),
        get_mode(&dest.path().join("a"))
    );
    assert_eq!(
        get_mode(&source.path().join("a/b")),
        get_mode(&dest.path().join("a/b"))
    );
    assert_eq!(get_mode(&src_nested), get_mode(&dest_nested));
}

#[test]
fn test_sync_dir_permissions_skips_missing() {
    let source = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();

    // Create directories only in source
    let src_only = source.path().join("source_only");
    fs::create_dir_all(&src_only).unwrap();

    // Create common directory
    let src_common = source.path().join("common");
    let dest_common = dest.path().join("common");
    fs::create_dir_all(&src_common).unwrap();
    fs::create_dir_all(&dest_common).unwrap();

    set_mode(&src_common, 0o755);
    set_mode(&dest_common, 0o700);

    // Sync - should only sync "common", skip "source_only"
    let result = sync_dir_permissions(source.path(), dest.path()).unwrap();

    assert_eq!(result.dirs_synced, 1, "Expected 1 directory synced");
    assert_eq!(result.errors, 0, "Expected no errors");
}

#[test]
fn test_sync_dir_permissions_files_not_affected() {
    let source = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();

    // Create file with specific permissions
    let src_file = source.path().join("test.txt");
    let dest_file = dest.path().join("test.txt");

    File::create(&src_file).unwrap();
    File::create(&dest_file).unwrap();

    set_mode(&src_file, 0o644);
    set_mode(&dest_file, 0o600);

    let original_dest_mode = get_mode(&dest_file);

    // Sync directories (should not touch files)
    let result = sync_dir_permissions(source.path(), dest.path()).unwrap();

    // File permissions should be unchanged
    assert_eq!(
        get_mode(&dest_file),
        original_dest_mode,
        "File permissions should not be changed by sync_dir_permissions"
    );
    assert_eq!(result.dirs_synced, 0, "No directories to sync");
}

#[test]
fn test_permission_mismatch_fields() {
    let source = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();

    // Create a file and directory
    let src_dir = source.path().join("testdir");
    let dest_dir = dest.path().join("testdir");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    File::create(src_dir.join("file.txt")).unwrap();
    File::create(dest_dir.join("file.txt")).unwrap();

    // Set different permissions
    set_mode(&src_dir, 0o755);
    set_mode(&dest_dir, 0o700);
    set_mode(&src_dir.join("file.txt"), 0o644);
    set_mode(&dest_dir.join("file.txt"), 0o600);

    let result = compare_permissions(source.path(), dest.path()).unwrap();

    // Should have 2 mismatches
    assert_eq!(result.mismatches.len(), 2);

    // Find directory mismatch
    let dir_mismatch = result
        .mismatches
        .iter()
        .find(|m| m.is_dir)
        .expect("Should have directory mismatch");

    assert!(dir_mismatch.path.contains("testdir"));
    assert_eq!(dir_mismatch.source_mode, 0o755);
    assert_eq!(dir_mismatch.dest_mode, 0o700);

    // Find file mismatch
    let file_mismatch = result
        .mismatches
        .iter()
        .find(|m| !m.is_dir)
        .expect("Should have file mismatch");

    assert!(file_mismatch.path.contains("file.txt"));
    assert_eq!(file_mismatch.source_mode, 0o644);
    assert_eq!(file_mismatch.dest_mode, 0o600);
}

#[test]
fn test_permission_compare_result_default() {
    let result = PermissionCompareResult::default();

    assert_eq!(result.checked, 0);
    assert!(result.mismatches.is_empty());
}

#[test]
fn test_compare_permissions_with_hidden_files() {
    let source = TempDir::new().unwrap();
    let dest = TempDir::new().unwrap();

    // Create hidden file
    File::create(source.path().join(".hidden")).unwrap();
    File::create(dest.path().join(".hidden")).unwrap();

    set_mode(&source.path().join(".hidden"), 0o644);
    set_mode(&dest.path().join(".hidden"), 0o600);

    let result = compare_permissions(source.path(), dest.path()).unwrap();

    // Should include hidden files
    assert!(
        result.mismatches.iter().any(|m| m.path.contains(".hidden")),
        "Should compare hidden files"
    );
}
