//! Parallel directory walker using jwalk
//!
//! This module provides fast parallel directory traversal with configurable
//! filtering and the ability to stream results without collecting all entries
//! into memory.
//!
//! ## Progress Tracking
//!
//! For real-time progress during crawls, use `scan_with_progress` which updates
//! atomic counters as files are discovered. The caller can poll these counters
//! or use `ProgressDisplay` for automatic display updates.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::SystemTime;

use jwalk::WalkDir;
use tracing::instrument;

use super::entry::FileEntry;
use super::filter::ScanFilter;

/// Configuration for scanning
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Follow symbolic links (default: false)
    pub follow_symlinks: bool,
    /// Skip hidden files and directories (default: false)
    /// For backup tools, this should be false to ensure all files are captured.
    pub skip_hidden: bool,
    /// Maximum depth to traverse (default: `usize::MAX`)
    pub max_depth: usize,
    /// Filter for excluding files/directories
    pub filter: ScanFilter,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            follow_symlinks: false,
            skip_hidden: false, // Include hidden files by default (important for backups)
            max_depth: usize::MAX,
            filter: ScanFilter::default(),
        }
    }
}

/// Progress counters for crawl operations
///
/// These counters are updated atomically during the crawl and can be
/// polled by a display thread for real-time progress updates.
#[derive(Debug)]
pub struct CrawlProgress {
    /// Number of files discovered so far
    pub files_found: AtomicUsize,
    /// Number of directories discovered so far
    pub dirs_found: AtomicUsize,
    /// Total bytes discovered so far
    pub bytes_found: AtomicU64,
    /// Cancellation flag - set to true to stop the crawl
    pub cancelled: AtomicBool,
}

use std::sync::atomic::AtomicU64;

impl CrawlProgress {
    /// Create a new progress tracker
    pub fn new() -> Self {
        Self {
            files_found: AtomicUsize::new(0),
            dirs_found: AtomicUsize::new(0),
            bytes_found: AtomicU64::new(0),
            cancelled: AtomicBool::new(false),
        }
    }

    /// Get current file count
    #[inline]
    pub fn files(&self) -> usize {
        self.files_found.load(Ordering::Relaxed)
    }

    /// Get current directory count
    #[inline]
    pub fn dirs(&self) -> usize {
        self.dirs_found.load(Ordering::Relaxed)
    }

    /// Get current bytes total
    #[inline]
    pub fn bytes(&self) -> u64 {
        self.bytes_found.load(Ordering::Relaxed)
    }

    /// Check if crawl was cancelled
    #[inline]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Cancel the crawl
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Increment file count
    #[inline]
    pub fn add_file(&self, size: u64) {
        self.files_found.fetch_add(1, Ordering::Relaxed);
        self.bytes_found.fetch_add(size, Ordering::Relaxed);
    }

    /// Increment directory count
    #[inline]
    pub fn add_dir(&self) {
        self.dirs_found.fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for CrawlProgress {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: All fields are atomic types (AtomicUsize, AtomicU64, AtomicBool)
// which are inherently Send+Sync.
unsafe impl Send for CrawlProgress {}
unsafe impl Sync for CrawlProgress {}

/// Error type for scanner operations
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("Failed to access path: {path}")]
    AccessError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Root path does not exist: {0}")]
    RootNotFound(PathBuf),

    #[error("Root path is not a directory: {0}")]
    NotADirectory(PathBuf),
}

/// Scan a directory tree and return an iterator of file entries.
///
/// This returns an iterator that yields results lazily, allowing processing
/// of large directory trees without loading everything into memory.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use scanner::{scan, ScanOptions};
///
/// let entries = scan(Path::new("/some/path"), ScanOptions::default()).unwrap();
/// for result in entries {
///     match result {
///         Ok(entry) => println!("{}: {} bytes", entry.path.display(), entry.size),
///         Err(e) => eprintln!("Error: {}", e),
///     }
/// }
/// ```
pub fn scan(
    root: &Path,
    options: ScanOptions,
) -> Result<impl Iterator<Item = Result<FileEntry, ScanError>>, ScanError> {
    scan_with_progress(root, options, None)
}

/// Scan a directory tree with progress tracking.
///
/// Like `scan`, but accepts an optional `CrawlProgress` to track progress
/// in real-time. The counters are updated atomically as files are discovered.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use std::sync::Arc;
/// use std::thread;
/// use std::time::Duration;
/// use scanner::{scan_with_progress, ScanOptions, CrawlProgress};
///
/// let progress = Arc::new(CrawlProgress::new());
/// let progress_clone = Arc::clone(&progress);
///
/// // Spawn a thread to monitor progress
/// thread::spawn(move || {
///     loop {
///         println!("Found {} files, {} dirs", progress_clone.files(), progress_clone.dirs());
///         thread::sleep(Duration::from_millis(100));
///     }
/// });
///
/// // Run the scan
/// let entries = scan_with_progress(
///     Path::new("/some/path"),
///     ScanOptions::default(),
///     Some(progress),
/// ).unwrap();
///
/// for result in entries {
///     // process entries...
/// }
/// ```
#[instrument(skip(options, progress), fields(root = %root.display()))]
pub fn scan_with_progress(
    root: &Path,
    options: ScanOptions,
    progress: Option<Arc<CrawlProgress>>,
) -> Result<impl Iterator<Item = Result<FileEntry, ScanError>>, ScanError> {
    // Validate root path
    if !root.exists() {
        return Err(ScanError::RootNotFound(root.to_path_buf()));
    }
    if !root.is_dir() {
        return Err(ScanError::NotADirectory(root.to_path_buf()));
    }

    // Canonicalize root for consistent relative paths
    let root = root.canonicalize().map_err(|e| ScanError::AccessError {
        path: root.to_path_buf(),
        source: e,
    })?;

    let filter = options.filter.clone();
    let progress_for_filter = progress.clone();

    let walk_dir = WalkDir::new(&root)
        .skip_hidden(options.skip_hidden)
        .follow_links(options.follow_symlinks)
        .max_depth(options.max_depth)
        .process_read_dir(move |_depth, _path, _state, children| {
            // Check for cancellation
            if let Some(ref p) = progress_for_filter
                && p.is_cancelled()
            {
                children.clear();
                return;
            }

            // Filter out entries we don't want (in parallel)
            children.retain(|entry| {
                entry.as_ref().map_or(true, |e| {
                    let name = e.file_name().to_string_lossy();
                    !filter.should_ignore(&name)
                })
            });
        });

    let root_for_iter = root.clone();
    let iter = walk_dir.into_iter().filter_map(move |result| {
        // Check for cancellation
        if let Some(ref p) = progress
            && p.is_cancelled()
        {
            return None;
        }

        match result {
            Ok(entry) => {
                let file_type = entry.file_type();

                // Track directories
                if file_type.is_dir() {
                    if let Some(ref p) = progress {
                        p.add_dir();
                    }
                    return None;
                }

                // Skip symlinks (not supported yet)
                if file_type.is_symlink() {
                    return None;
                }

                // Skip special files (devices, sockets, pipes) - can't/shouldn't be copied
                // On Unix, these are: block devices, char devices, fifos, sockets
                #[cfg(unix)]
                {
                    use std::os::unix::fs::FileTypeExt;
                    if file_type.is_block_device()
                        || file_type.is_char_device()
                        || file_type.is_fifo()
                        || file_type.is_socket()
                    {
                        return None;
                    }
                }

                // Get metadata
                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        return Some(Err(ScanError::AccessError {
                            path: entry.path(),
                            source: std::io::Error::other(e.to_string()),
                        }));
                    }
                };

                let size = metadata.len();

                // Track file progress
                if let Some(ref p) = progress {
                    p.add_file(size);
                }

                // Calculate relative path
                let absolute_path = entry.path();
                let relative_path = absolute_path
                    .strip_prefix(&root_for_iter)
                    .unwrap_or(&absolute_path)
                    .to_path_buf();

                // Get mtime as unix timestamp
                let mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());

                Some(Ok(FileEntry {
                    path: relative_path,
                    absolute_path,
                    size,
                    mtime,
                    hash: None,
                    hash_xxh3: None,
                }))
            }
            Err(e) => Some(Err(ScanError::AccessError {
                path: e
                    .path()
                    .map(std::path::Path::to_path_buf)
                    .unwrap_or_default(),
                source: std::io::Error::other(e.to_string()),
            })),
        }
    });

    Ok(iter)
}

/// Scan a directory tree and collect all entries into a Vec.
///
/// Convenience function when you need all entries in memory.
/// For large directories, prefer using `scan()` and processing entries
/// in a streaming fashion.
pub fn scan_collect(root: &Path, options: ScanOptions) -> Result<Vec<FileEntry>, ScanError> {
    scan_collect_with_progress(root, options, None)
}

/// Collect all entries from a scan with progress tracking.
///
/// Convenience function when you need all entries in memory.
/// For large directories, prefer using `scan_with_progress()` and processing
/// entries in a streaming fashion.
pub fn scan_collect_with_progress(
    root: &Path,
    options: ScanOptions,
    progress: Option<Arc<CrawlProgress>>,
) -> Result<Vec<FileEntry>, ScanError> {
    let iter = scan_with_progress(root, options, progress)?;
    let mut entries = Vec::new();

    for result in iter {
        entries.push(result?);
    }

    Ok(entries)
}

#[cfg(test)]
#[path = "walker_test.rs"]
mod walker_test;
