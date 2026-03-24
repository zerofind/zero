//! Permission comparison and synchronization for file transfers
//!
//! This module provides utilities for comparing and syncing Unix file permissions
//! between source and destination directory trees.
//!
//! ## Use Cases
//!
//! - Verifying that copied files have correct permissions
//! - Syncing directory permissions after transfer (file permissions are preserved by `std::fs::copy`)
//! - Auditing permission differences between source and destination

use std::fs;
use std::io;
use std::path::Path;

use jwalk::WalkDir;

/// Result of permission comparison
#[derive(Debug, Default)]
pub struct PermissionCompareResult {
    /// Number of files/directories checked
    pub checked: usize,
    /// Paths where permissions differ
    pub mismatches: Vec<PermissionMismatch>,
}

/// A permission mismatch between source and destination
#[derive(Debug, Clone)]
pub struct PermissionMismatch {
    /// Relative path
    pub path: String,
    /// Whether it's a directory
    pub is_dir: bool,
    /// Source permissions (octal)
    pub source_mode: u32,
    /// Destination permissions (octal)
    pub dest_mode: u32,
}

/// Compare permissions between source and destination directories
///
/// Walks both directory trees and compares file/directory permissions.
///
/// # Arguments
/// * `source_root` - Root of the source directory tree
/// * `dest_root` - Root of the destination directory tree
///
/// # Returns
/// Result containing comparison results
pub fn compare_permissions(
    source_root: &Path,
    dest_root: &Path,
) -> io::Result<PermissionCompareResult> {
    use std::os::unix::fs::PermissionsExt;

    let mut result = PermissionCompareResult::default();

    // Walk source directory tree
    for entry in WalkDir::new(source_root)
        .skip_hidden(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let entry_path = entry.path();

        // Get relative path
        let Ok(rel_path) = entry_path.strip_prefix(source_root) else {
            continue;
        };

        // Skip the root directory itself
        if rel_path.as_os_str().is_empty() {
            continue;
        }

        let dest_path = dest_root.join(rel_path);

        // Skip if destination doesn't exist
        if !dest_path.exists() {
            continue;
        }

        // Get permissions from both
        let Ok(source_meta) = fs::metadata(&entry_path) else {
            continue;
        };
        let Ok(dest_meta) = fs::metadata(&dest_path) else {
            continue;
        };

        let source_mode = source_meta.permissions().mode();
        let dest_mode = dest_meta.permissions().mode();

        result.checked += 1;

        // Compare (mask out file type bits, only compare permission bits)
        let source_perms = source_mode & 0o7777;
        let dest_perms = dest_mode & 0o7777;

        if source_perms != dest_perms {
            result.mismatches.push(PermissionMismatch {
                path: rel_path.to_string_lossy().to_string(),
                is_dir: entry.file_type().is_dir(),
                source_mode: source_perms,
                dest_mode: dest_perms,
            });
        }
    }

    Ok(result)
}

/// Result of directory permission sync
#[must_use]
#[derive(Debug, Default)]
pub struct PermissionSyncResult {
    /// Number of directories whose permissions were synced
    pub dirs_synced: usize,
    /// Number of errors encountered
    pub errors: usize,
}

/// Sync directory permissions from source to destination
///
/// After a transfer/sync completes, this function walks the source directory tree
/// and copies the permissions of each directory to the corresponding destination directory.
///
/// File permissions are already preserved by `std::fs::copy`, so this only handles directories.
///
/// # Arguments
/// * `source_root` - Root of the source directory tree
/// * `dest_root` - Root of the destination directory tree
///
/// # Returns
/// Result containing the number of directories synced
pub fn sync_dir_permissions(
    source_root: &Path,
    dest_root: &Path,
) -> io::Result<PermissionSyncResult> {
    let mut result = PermissionSyncResult::default();

    // Walk source directory tree
    for entry in WalkDir::new(source_root)
        .skip_hidden(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        // Only process directories
        if !entry.file_type().is_dir() {
            continue;
        }

        // Get the path and store it to extend its lifetime
        let entry_path = entry.path();

        // Get relative path
        let Ok(rel_path) = entry_path.strip_prefix(source_root) else {
            continue;
        };

        // Skip the root directory itself (empty relative path)
        if rel_path.as_os_str().is_empty() {
            continue;
        }

        let dest_dir = dest_root.join(rel_path);

        // Only sync if destination exists
        if !dest_dir.exists() {
            continue;
        }

        // Get source permissions and apply to destination
        match fs::metadata(&entry_path) {
            Ok(source_meta) => {
                let perms = source_meta.permissions();
                if let Err(_e) = fs::set_permissions(&dest_dir, perms) {
                    result.errors += 1;
                    // Optionally log: tracing::warn!("Failed to set permissions on {}: {}", dest_dir.display(), e);
                } else {
                    result.dirs_synced += 1;
                }
            }
            Err(_) => {
                result.errors += 1;
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
#[path = "permissions_test.rs"]
mod permissions_test;
