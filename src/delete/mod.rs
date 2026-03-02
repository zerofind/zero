//! File and directory deletion operations
//!
//! Provides both permanent deletion and move-to-trash functionality.
//!
//! ## Features
//!
//! - Single file/directory deletion
//! - Batch deletion (multiple paths)
//! - Move to system trash (macOS)
//! - Parallel deletion for batches
//! - Dry-run mode
//!
//! ## Usage
//!
//! ```no_run
//! use std::path::Path;
//! use zero::delete::{DeleteOptions, delete_paths};
//!
//! // Permanent delete
//! let result = delete_paths(&[Path::new("/tmp/file.txt")], DeleteOptions::default())?;
//!
//! // Move to trash
//! let result = delete_paths(
//!     &[Path::new("/tmp/file.txt")],
//!     DeleteOptions::default().use_trash(true)
//! )?;
//! # Ok::<(), zero::delete::DeleteError>(())
//! ```

use std::fs;
use std::io;
use std::panic;
use std::path::{Path, PathBuf};
use std::process::Command;

use rayon::prelude::*;

/// Options for delete operations
#[derive(Debug, Clone)]
pub struct DeleteOptions {
    /// Move to system trash instead of permanent delete
    pub use_trash: bool,
    /// Delete directories recursively
    pub recursive: bool,
    /// Continue on errors (don't stop at first failure)
    pub continue_on_error: bool,
}

impl Default for DeleteOptions {
    fn default() -> Self {
        Self {
            use_trash: false,
            recursive: true,
            continue_on_error: true,
        }
    }
}

impl DeleteOptions {
    /// Create new options with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set whether to use system trash
    pub fn use_trash(mut self, use_trash: bool) -> Self {
        self.use_trash = use_trash;
        self
    }

    /// Set whether to delete directories recursively
    pub fn recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    /// Set whether to continue on errors
    pub fn continue_on_error(mut self, continue_on_error: bool) -> Self {
        self.continue_on_error = continue_on_error;
        self
    }
}

/// Information about a single delete operation
#[derive(Debug, Clone)]
pub struct DeletedItem {
    /// Path that was deleted
    pub path: PathBuf,
    /// Size in bytes (for files)
    pub size: u64,
    /// Whether it was a directory
    pub is_dir: bool,
    /// Number of items inside (for directories)
    pub item_count: usize,
}

/// Error during delete operation
#[derive(Debug, Clone)]
pub struct DeleteFailure {
    /// Path that failed to delete
    pub path: PathBuf,
    /// Error message
    pub message: String,
}

/// Result of a delete operation
#[derive(Debug)]
pub struct DeleteResult {
    /// Items that were successfully deleted
    pub deleted: Vec<DeletedItem>,
    /// Items that failed to delete
    pub failures: Vec<DeleteFailure>,
    /// Total bytes deleted
    pub bytes_deleted: u64,
    /// Total files deleted
    pub files_deleted: usize,
    /// Total directories deleted
    pub dirs_deleted: usize,
}

impl DeleteResult {
    /// Create an empty result
    pub fn empty() -> Self {
        Self {
            deleted: Vec::new(),
            failures: Vec::new(),
            bytes_deleted: 0,
            files_deleted: 0,
            dirs_deleted: 0,
        }
    }

    /// Check if all deletions succeeded
    pub fn success(&self) -> bool {
        self.failures.is_empty()
    }

    /// Total items deleted
    pub fn total_deleted(&self) -> usize {
        self.deleted.len()
    }

    /// Total items that failed
    pub fn total_failed(&self) -> usize {
        self.failures.len()
    }
}

/// Error type for delete operations
#[derive(Debug, thiserror::Error)]
pub enum DeleteError {
    #[error("Path does not exist: {0}")]
    NotFound(PathBuf),

    #[error("Permission denied: {0}")]
    PermissionDenied(PathBuf),

    #[error("Cannot delete directory without recursive flag: {0}")]
    IsDirectory(PathBuf),

    #[error("IO error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("Trash operation failed: {0}")]
    TrashError(String),

    #[error("All deletions failed")]
    AllFailed,
}

/// Delete a single file or directory
pub fn delete_path(path: &Path, options: &DeleteOptions) -> Result<DeletedItem, DeleteError> {
    // Check if path exists
    if !path.exists() {
        return Err(DeleteError::NotFound(path.to_path_buf()));
    }

    let metadata = path.metadata().map_err(|e| DeleteError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    let is_dir = metadata.is_dir();

    // Check if we can delete directories
    if is_dir && !options.recursive {
        return Err(DeleteError::IsDirectory(path.to_path_buf()));
    }

    // Calculate size and item count before deletion
    let (size, item_count) = if is_dir {
        calculate_dir_stats(path)
    } else {
        (metadata.len(), 1)
    };

    // Perform the delete
    if options.use_trash {
        move_to_trash(path)?;
    } else {
        permanent_delete(path, is_dir)?;
    }

    Ok(DeletedItem {
        path: path.to_path_buf(),
        size,
        is_dir,
        item_count,
    })
}

/// Delete multiple paths (batch operation)
///
/// Uses parallel deletion for efficiency.
pub fn delete_paths(paths: &[&Path], options: DeleteOptions) -> Result<DeleteResult, DeleteError> {
    if paths.is_empty() {
        return Ok(DeleteResult::empty());
    }

    // For small batches, do sequential deletion
    if paths.len() < 4 {
        return delete_paths_sequential(paths, &options);
    }

    // Parallel deletion for larger batches
    delete_paths_parallel(paths, &options)
}

/// Sequential deletion (for small batches or when order matters)
fn delete_paths_sequential(
    paths: &[&Path],
    options: &DeleteOptions,
) -> Result<DeleteResult, DeleteError> {
    let mut result = DeleteResult::empty();

    for path in paths {
        match delete_path(path, options) {
            Ok(item) => {
                result.bytes_deleted += item.size;
                if item.is_dir {
                    result.dirs_deleted += 1;
                } else {
                    result.files_deleted += 1;
                }
                result.deleted.push(item);
            }
            Err(e) => {
                result.failures.push(DeleteFailure {
                    path: path.to_path_buf(),
                    message: e.to_string(),
                });

                if !options.continue_on_error {
                    break;
                }
            }
        }
    }

    if result.deleted.is_empty() && !result.failures.is_empty() {
        return Err(DeleteError::AllFailed);
    }

    Ok(result)
}

/// Parallel deletion for large batches
fn delete_paths_parallel(
    paths: &[&Path],
    options: &DeleteOptions,
) -> Result<DeleteResult, DeleteError> {
    // Clone options for thread safety
    let options = options.clone();

    // Wrap in catch_unwind for panic safety (early exit strategy)
    let delete_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        paths
            .par_iter()
            .map(|path| {
                let result = delete_path(path, &options);
                (path.to_path_buf(), result)
            })
            .collect::<Vec<(PathBuf, Result<DeletedItem, DeleteError>)>>()
    }));

    // Handle panic during parallel deletion
    let results = match delete_result {
        Ok(results) => results,
        Err(panic_payload) => {
            eprintln!("\n⚠ Fatal error during parallel deletion");
            panic::resume_unwind(panic_payload);
        }
    };

    // Aggregate results
    let mut result = DeleteResult::empty();

    for (_path, item_result) in results {
        match item_result {
            Ok(item) => {
                result.bytes_deleted += item.size;
                if item.is_dir {
                    result.dirs_deleted += 1;
                } else {
                    result.files_deleted += 1;
                }
                result.deleted.push(item);
            }
            Err(e) => {
                // Path is already in the error, but we need it for the failure record
                let path = match &e {
                    DeleteError::NotFound(p) => p.clone(),
                    DeleteError::PermissionDenied(p) => p.clone(),
                    DeleteError::IsDirectory(p) => p.clone(),
                    DeleteError::Io { path, .. } => path.clone(),
                    DeleteError::TrashError(_) => PathBuf::new(),
                    DeleteError::AllFailed => PathBuf::new(),
                };
                result.failures.push(DeleteFailure {
                    path,
                    message: e.to_string(),
                });
            }
        }
    }

    if result.deleted.is_empty() && !result.failures.is_empty() {
        return Err(DeleteError::AllFailed);
    }

    Ok(result)
}

/// Permanently delete a file or directory
fn permanent_delete(path: &Path, is_dir: bool) -> Result<(), DeleteError> {
    if is_dir {
        fs::remove_dir_all(path).map_err(|e| {
            if e.kind() == io::ErrorKind::PermissionDenied {
                DeleteError::PermissionDenied(path.to_path_buf())
            } else {
                DeleteError::Io {
                    path: path.to_path_buf(),
                    source: e,
                }
            }
        })
    } else {
        fs::remove_file(path).map_err(|e| {
            if e.kind() == io::ErrorKind::PermissionDenied {
                DeleteError::PermissionDenied(path.to_path_buf())
            } else {
                DeleteError::Io {
                    path: path.to_path_buf(),
                    source: e,
                }
            }
        })
    }
}

/// Move a file or directory to system trash (macOS)
///
/// Uses AppleScript/osascript to move items to trash, which:
/// - Properly integrates with Finder
/// - Allows "Put Back" functionality
/// - Handles name conflicts in Trash
#[cfg(target_os = "macos")]
fn move_to_trash(path: &Path) -> Result<(), DeleteError> {
    let abs_path = path.canonicalize().map_err(|e| DeleteError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    // Use POSIX path for AppleScript
    let posix_path = abs_path.to_string_lossy();

    // AppleScript to move item to trash
    // This is the most reliable method on macOS as it uses Finder's trash mechanism
    let script = format!(
        r#"tell application "Finder"
            delete POSIX file "{}"
        end tell"#,
        posix_path.replace('"', "\\\"")
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| DeleteError::TrashError(format!("Failed to run osascript: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DeleteError::TrashError(format!(
            "Finder trash failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

/// Fallback for non-macOS platforms - just do permanent delete
#[cfg(not(target_os = "macos"))]
fn move_to_trash(path: &Path) -> Result<(), DeleteError> {
    // On other platforms, we could use:
    // - Linux: freedesktop trash spec (XDG_DATA_HOME/Trash)
    // - Windows: SHFileOperation with FO_DELETE and FOF_ALLOWUNDO

    // For now, just warn and do permanent delete
    tracing::warn!(
        "Trash not implemented on this platform, permanently deleting: {}",
        path.display()
    );

    let is_dir = path.is_dir();
    permanent_delete(path, is_dir)
}

/// Calculate total size and item count for a directory
fn calculate_dir_stats(path: &Path) -> (u64, usize) {
    let mut total_size = 0u64;
    let mut item_count = 0usize;

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                item_count += 1;
                if metadata.is_file() {
                    total_size += metadata.len();
                } else if metadata.is_dir() {
                    let (sub_size, sub_count) = calculate_dir_stats(&entry.path());
                    total_size += sub_size;
                    item_count += sub_count;
                }
            }
        }
    }

    (total_size, item_count)
}

/// Preview what would be deleted (dry-run)
pub fn preview_delete(paths: &[&Path], recursive: bool) -> Vec<PreviewItem> {
    paths
        .iter()
        .map(|path| {
            if !path.exists() {
                return PreviewItem {
                    path: path.to_path_buf(),
                    exists: false,
                    is_dir: false,
                    size: 0,
                    item_count: 0,
                    can_delete: false,
                    reason: Some("Path does not exist".to_string()),
                };
            }

            let metadata = match path.metadata() {
                Ok(m) => m,
                Err(e) => {
                    return PreviewItem {
                        path: path.to_path_buf(),
                        exists: true,
                        is_dir: false,
                        size: 0,
                        item_count: 0,
                        can_delete: false,
                        reason: Some(format!("Cannot read metadata: {}", e)),
                    };
                }
            };

            let is_dir = metadata.is_dir();

            // Check if we can delete directories
            if is_dir && !recursive {
                return PreviewItem {
                    path: path.to_path_buf(),
                    exists: true,
                    is_dir: true,
                    size: 0,
                    item_count: 0,
                    can_delete: false,
                    reason: Some("Directory requires --recursive flag".to_string()),
                };
            }

            let (size, item_count) = if is_dir {
                calculate_dir_stats(path)
            } else {
                (metadata.len(), 1)
            };

            PreviewItem {
                path: path.to_path_buf(),
                exists: true,
                is_dir,
                size,
                item_count,
                can_delete: true,
                reason: None,
            }
        })
        .collect()
}

/// Information about an item to be deleted (for dry-run preview)
#[derive(Debug, Clone)]
pub struct PreviewItem {
    /// Path to the item
    pub path: PathBuf,
    /// Whether the path exists
    pub exists: bool,
    /// Whether it's a directory
    pub is_dir: bool,
    /// Size in bytes
    pub size: u64,
    /// Number of items (1 for files, count for directories)
    pub item_count: usize,
    /// Whether it can be deleted
    pub can_delete: bool,
    /// Reason if it cannot be deleted
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_delete_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        assert!(file_path.exists());

        let result = delete_path(&file_path, &DeleteOptions::default()).unwrap();

        assert!(!file_path.exists());
        assert!(!result.is_dir);
    }

    #[test]
    fn test_delete_directory() {
        let dir = tempdir().unwrap();
        let sub_dir = dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        File::create(sub_dir.join("file.txt")).unwrap();

        assert!(sub_dir.exists());

        let result = delete_path(&sub_dir, &DeleteOptions::default()).unwrap();

        assert!(!sub_dir.exists());
        assert!(result.is_dir);
        assert_eq!(result.item_count, 1);
    }

    #[test]
    fn test_delete_nonexistent() {
        let path = Path::new("/nonexistent/path/file.txt");
        let result = delete_path(path, &DeleteOptions::default());

        assert!(matches!(result, Err(DeleteError::NotFound(_))));
    }

    #[test]
    fn test_preview_delete() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        let preview = preview_delete(&[file_path.as_path()], true);

        assert_eq!(preview.len(), 1);
        assert!(preview[0].exists);
        assert!(preview[0].can_delete);
        assert!(!preview[0].is_dir);
    }

    #[test]
    fn test_batch_delete() {
        let dir = tempdir().unwrap();
        let file1 = dir.path().join("file1.txt");
        let file2 = dir.path().join("file2.txt");
        let file3 = dir.path().join("file3.txt");

        File::create(&file1).unwrap();
        File::create(&file2).unwrap();
        File::create(&file3).unwrap();

        let paths: Vec<&Path> = vec![file1.as_path(), file2.as_path(), file3.as_path()];
        let result = delete_paths(&paths, DeleteOptions::default()).unwrap();

        assert_eq!(result.files_deleted, 3);
        assert!(result.success());
        assert!(!file1.exists());
        assert!(!file2.exists());
        assert!(!file3.exists());
    }
}
