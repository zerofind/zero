//! Simple transfer job - copy all files from source to destination
//!
//! This is a "dumb" copy that transfers everything without diffing.
//! For smart incremental sync, use the `sync` module instead.
//!
//! ## Design
//!
//! - Scans source directory
//! - Copies ALL files to destination (no diff, no state tracking)
//! - Parallel copying for performance
//! - Optional verification after copy
//!
//! ## Panic Safety (Early Exit Strategy)
//!
//! Parallel operations are wrapped in catch_unwind to ensure
//! clean process exit on fatal errors. This prevents zombie rayon threads
//! from hanging around consuming CPU after a crash.

use std::fs;
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use rayon::prelude::*;

use crate::hasher::HashAlgorithm;
use crate::scanner::{ScanOptions, scan_collect};

use super::chunk::{CopyOptions, copy_file};
use super::permissions::sync_dir_permissions;

/// Options for a transfer job
#[derive(Debug, Clone, Default)]
pub struct TransferOptions {
    /// Verify checksums after each file copy
    pub verify: bool,
    /// Hash algorithm for verification
    pub hash_algorithm: HashAlgorithm,
    /// Maximum depth to scan
    pub max_depth: Option<usize>,
    /// Number of parallel copy threads (0 = auto)
    pub parallel_copies: usize,
    /// Preserve directory permissions (file permissions are always preserved)
    pub preserve_permissions: bool,
}

/// Progress of a transfer job
#[derive(Debug, Clone)]
pub struct TransferProgress {
    /// Total files to transfer
    pub total_files: usize,
    /// Files completed
    pub files_done: usize,
    /// Total bytes to transfer
    pub total_bytes: u64,
    /// Bytes transferred so far
    pub bytes_done: u64,
    /// Current file being transferred (if any)
    pub current_file: Option<PathBuf>,
    /// Number of errors encountered
    pub errors: usize,
}

impl TransferProgress {
    /// Overall percentage complete (0.0 to 100.0)
    pub fn percent(&self) -> f64 {
        if self.total_bytes == 0 {
            100.0
        } else {
            (self.bytes_done as f64 / self.total_bytes as f64) * 100.0
        }
    }
}

/// Result of a completed transfer job
#[must_use]
#[derive(Debug)]
pub struct TransferResult {
    /// Number of files successfully transferred
    pub files_transferred: usize,
    /// Total bytes transferred
    pub bytes_transferred: u64,
    /// Number of files that failed
    pub files_failed: usize,
    /// Total duration
    pub duration: std::time::Duration,
    /// Number of directories whose permissions were synced (if preserve_permissions enabled)
    pub dirs_permissions_synced: usize,
}

/// Error type for transfer jobs
#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("Scan error: {0}")]
    ScanError(#[from] crate::scanner::ScanError),

    #[error("Copy error for {path}: {source}")]
    CopyFailed {
        path: String,
        #[source]
        source: super::chunk::CopyError,
    },

    #[error("Source directory does not exist: {0}")]
    SourceNotFound(PathBuf),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// A simple transfer job that copies all files from source to destination
pub struct TransferJob {
    source: PathBuf,
    dest: PathBuf,
    options: TransferOptions,
}

impl TransferJob {
    /// Create a new transfer job
    pub fn new(
        source: impl AsRef<Path>,
        dest: impl AsRef<Path>,
        options: TransferOptions,
    ) -> Result<Self, TransferError> {
        let source = source.as_ref().to_path_buf();
        let dest = dest.as_ref().to_path_buf();

        if !source.exists() {
            return Err(TransferError::SourceNotFound(source));
        }

        // Create destination if it doesn't exist
        if !dest.exists() {
            fs::create_dir_all(&dest)?;
        }

        Ok(Self {
            source,
            dest,
            options,
        })
    }

    /// Run the transfer job - copies ALL files from source to dest
    pub fn run(
        &self,
        on_progress: impl Fn(&TransferProgress),
    ) -> Result<TransferResult, TransferError> {
        let start_time = Instant::now();

        // Scan source
        let scan_options = ScanOptions {
            max_depth: self.options.max_depth.unwrap_or(usize::MAX),
            ..Default::default()
        };
        let source_files = scan_collect(&self.source, scan_options)?;

        let total_files = source_files.len();
        let total_bytes: u64 = source_files.iter().map(|f| f.size).sum();

        // Initial progress
        let progress = TransferProgress {
            total_files,
            files_done: 0,
            total_bytes,
            bytes_done: 0,
            current_file: None,
            errors: 0,
        };
        on_progress(&progress);

        // Copy options
        let copy_options = CopyOptions {
            verify: self.options.verify,
            hash_algorithm: self.options.hash_algorithm,
        };

        // Counters for parallel transfer
        let files_transferred = AtomicUsize::new(0);
        let bytes_transferred = AtomicU64::new(0);
        let errors = AtomicUsize::new(0);

        // Copy all files in parallel
        // Wrap in catch_unwind for panic safety (early exit strategy)
        let dest_ref = &self.dest;
        let parallel_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            source_files.par_iter().for_each(|file| {
                let dest_path = dest_ref.join(&file.path);

                // Create parent directory if needed
                if let Some(parent) = dest_path.parent()
                    && !parent.exists() {
                        let _ = fs::create_dir_all(parent);
                    }

                // Pre-allocate destination file
                if file.size > 0 {
                    let _ = preallocate_file(&dest_path, file.size);
                }

                // Copy the file
                match copy_file(&file.absolute_path, &dest_path, &copy_options) {
                    Ok(_) => {
                        files_transferred.fetch_add(1, Ordering::Relaxed);
                        bytes_transferred.fetch_add(file.size, Ordering::Relaxed);
                    }
                    Err(e) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        tracing::warn!("Failed to copy {}: {}", file.path.display(), e);
                    }
                }
            });
        }));

        // If a panic occurred during parallel copy, propagate it
        // The global panic handler will ensure clean process exit
        if let Err(panic_payload) = parallel_result {
            eprintln!("\n⚠ Fatal error during parallel file transfer");
            panic::resume_unwind(panic_payload);
        }

        let final_files = files_transferred.load(Ordering::Relaxed);
        let final_bytes = bytes_transferred.load(Ordering::Relaxed);
        let final_errors = errors.load(Ordering::Relaxed);

        // Final progress
        let final_progress = TransferProgress {
            total_files,
            files_done: final_files,
            total_bytes,
            bytes_done: final_bytes,
            current_file: None,
            errors: final_errors,
        };
        on_progress(&final_progress);

        // Sync directory permissions if requested
        let dirs_permissions_synced = if self.options.preserve_permissions {
            match sync_dir_permissions(&self.source, &self.dest) {
                Ok(result) => result.dirs_synced,
                Err(e) => {
                    tracing::warn!("Failed to sync directory permissions: {}", e);
                    0
                }
            }
        } else {
            0
        };

        Ok(TransferResult {
            files_transferred: final_files,
            bytes_transferred: final_bytes,
            files_failed: final_errors,
            duration: start_time.elapsed(),
            dirs_permissions_synced,
        })
    }
}

/// Pre-allocate a file to the given size
fn preallocate_file(path: &Path, size: u64) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    let file = fs::File::create(path)?;
    file.set_len(size)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_source(dir: &Path) {
        fs::create_dir_all(dir.join("subdir")).unwrap();

        File::create(dir.join("file1.txt"))
            .unwrap()
            .write_all(b"hello world")
            .unwrap();

        File::create(dir.join("file2.txt"))
            .unwrap()
            .write_all(b"goodbye world")
            .unwrap();

        File::create(dir.join("subdir/nested.txt"))
            .unwrap()
            .write_all(b"nested content")
            .unwrap();
    }

    #[test]
    fn test_transfer_job_fresh() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source");
        let dest = dir.path().join("dest");

        create_test_source(&source);

        let options = TransferOptions::default();
        let job = TransferJob::new(&source, &dest, options).unwrap();

        let result = job.run(|_| {}).unwrap();

        assert_eq!(result.files_transferred, 3);
        assert_eq!(result.files_failed, 0);

        // Verify files exist
        assert!(dest.join("file1.txt").exists());
        assert!(dest.join("file2.txt").exists());
        assert!(dest.join("subdir/nested.txt").exists());

        // Verify content
        let content = fs::read_to_string(dest.join("file1.txt")).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_transfer_job_empty_source() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source");
        let dest = dir.path().join("dest");

        fs::create_dir_all(&source).unwrap();

        let job = TransferJob::new(&source, &dest, TransferOptions::default()).unwrap();
        let result = job.run(|_| {}).unwrap();

        assert_eq!(result.files_transferred, 0);
        assert_eq!(result.files_failed, 0);
    }

    #[test]
    fn test_transfer_job_source_not_found() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("nonexistent");
        let dest = dir.path().join("dest");

        let result = TransferJob::new(&source, &dest, TransferOptions::default());
        assert!(matches!(result, Err(TransferError::SourceNotFound(_))));
    }

    #[test]
    fn test_transfer_progress_percent() {
        let progress = TransferProgress {
            total_files: 10,
            files_done: 5,
            total_bytes: 1000,
            bytes_done: 500,
            current_file: None,
            errors: 0,
        };

        assert!((progress.percent() - 50.0).abs() < 0.001);
    }

    #[test]
    fn test_transfer_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source");
        let dest = dir.path().join("dest");

        // Create source
        fs::create_dir_all(&source).unwrap();
        File::create(source.join("file.txt"))
            .unwrap()
            .write_all(b"new content")
            .unwrap();

        // Create dest with old content
        fs::create_dir_all(&dest).unwrap();
        File::create(dest.join("file.txt"))
            .unwrap()
            .write_all(b"old content")
            .unwrap();

        let job = TransferJob::new(&source, &dest, TransferOptions::default()).unwrap();
        let result = job.run(|_| {}).unwrap();

        assert_eq!(result.files_transferred, 1);

        // Should have new content
        let content = fs::read_to_string(dest.join("file.txt")).unwrap();
        assert_eq!(content, "new content");
    }

    #[test]
    fn test_transfer_with_verification() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source");
        let dest = dir.path().join("dest");

        create_test_source(&source);

        let options = TransferOptions {
            verify: true,
            ..Default::default()
        };
        let job = TransferJob::new(&source, &dest, options).unwrap();
        let result = job.run(|_| {}).unwrap();

        assert_eq!(result.files_transferred, 3);
        assert_eq!(result.files_failed, 0);
    }
}
