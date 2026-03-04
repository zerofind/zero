//! Duplicate file finder and remover
//!
//! Algorithm:
//! 1. Scan directory, collect all files
//! 2. Group by size (files with different sizes can't be duplicates)
//! 3. For groups with 2+ files, compute hash
//! 4. Group by hash - these are true duplicates
//! 5. Optionally delete duplicates (keep shortest path)
//!
//! ## Progress Tracking
//!
//! Use `find_duplicates_with_progress` for real-time progress updates:
//! - Phase 1: Scanning files (files_found increments)
//! - Phase 2: Hashing potential duplicates (bytes_hashed increments)
//!
//! ## Panic Safety (Early Exit Strategy)
//!
//! Parallel operations are wrapped in catch_unwind to ensure
//! clean process exit on fatal errors. This prevents zombie rayon threads
//! from hanging around consuming CPU after a crash.

use std::collections::HashMap;
use std::fs;
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use rayon::prelude::*;

use crate::hasher::{HashAlgorithm, hash_file_with_buffer};
use crate::index::FileTypeCategory;
use crate::scanner::{CrawlProgress, FileEntry, ScanOptions, scan_with_progress};

/// Options for duplicate finding
#[derive(Debug, Clone)]
pub struct DedupOptions {
    /// Hash algorithm to use
    pub hash_algorithm: HashAlgorithm,
    /// Maximum depth to scan
    pub max_depth: Option<usize>,
    /// Minimum file size to consider (skip tiny files)
    pub min_size: u64,
    /// Verify duplicates by reading full content (slower but certain)
    pub verify: bool,
    /// Filter by file type category (None = all files)
    pub type_filter: Option<FileTypeCategory>,
    /// Skip hash verification - use size + name only (blazing fast but may have false positives)
    pub skip_hash: bool,
}

impl Default for DedupOptions {
    fn default() -> Self {
        Self {
            hash_algorithm: HashAlgorithm::default(),
            max_depth: None,
            min_size: 1, // Skip empty files by default
            verify: false,
            type_filter: None,
            skip_hash: false,
        }
    }
}

impl DedupOptions {
    /// Create options for finding duplicate images
    pub fn images() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Images),
            ..Default::default()
        }
    }

    /// Create options for finding duplicate videos
    pub fn videos() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Videos),
            ..Default::default()
        }
    }

    /// Create options for finding duplicate audio files
    pub fn audio() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Audio),
            ..Default::default()
        }
    }

    /// Create options for finding duplicate documents
    pub fn documents() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Documents),
            ..Default::default()
        }
    }

    /// Create options for finding duplicate code files
    pub fn code() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Code),
            ..Default::default()
        }
    }

    /// Create options for finding duplicate archives
    pub fn archives() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Archives),
            ..Default::default()
        }
    }

    /// Create options for finding duplicate executables/binaries
    pub fn executables() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Executables),
            ..Default::default()
        }
    }

    /// Set the type filter
    pub fn with_type(mut self, type_filter: FileTypeCategory) -> Self {
        self.type_filter = Some(type_filter);
        self
    }

    /// Set minimum file size
    pub fn with_min_size(mut self, min_size: u64) -> Self {
        self.min_size = min_size;
        self
    }

    /// Set maximum depth
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = Some(max_depth);
        self
    }

    /// Enable verification mode
    pub fn with_verify(mut self) -> Self {
        self.verify = true;
        self
    }

    /// Skip hash verification (quick scan mode)
    pub fn with_skip_hash(mut self) -> Self {
        self.skip_hash = true;
        self
    }
}

/// Progress tracking for dedup operations
#[derive(Debug)]
pub struct DedupProgress {
    /// Current phase of the operation
    pub phase: AtomicUsize, // 0=scanning, 1=hashing, 2=complete
    /// Files found during scan
    pub files_found: AtomicUsize,
    /// Bytes found during scan
    pub bytes_found: AtomicU64,
    /// Files that are potential duplicates (same size as another)
    pub potential_duplicates: AtomicUsize,
    /// Files hashed so far
    pub files_hashed: AtomicUsize,
    /// Bytes hashed so far
    pub bytes_hashed: AtomicU64,
    /// Total bytes to hash (potential duplicates only)
    pub bytes_to_hash: AtomicU64,
    /// Cancellation flag
    pub cancelled: AtomicBool,
}

impl DedupProgress {
    /// Create a new progress tracker
    pub fn new() -> Self {
        Self {
            phase: AtomicUsize::new(0),
            files_found: AtomicUsize::new(0),
            bytes_found: AtomicU64::new(0),
            potential_duplicates: AtomicUsize::new(0),
            files_hashed: AtomicUsize::new(0),
            bytes_hashed: AtomicU64::new(0),
            bytes_to_hash: AtomicU64::new(0),
            cancelled: AtomicBool::new(false),
        }
    }

    /// Get current phase name
    pub fn phase_name(&self) -> &'static str {
        match self.phase.load(Ordering::Acquire) {
            0 => "Scanning",
            1 => "Hashing",
            2 => "Complete",
            _ => "Unknown",
        }
    }

    /// Check if cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Request cancellation
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Get hash progress as percentage (0-100)
    pub fn hash_percent(&self) -> f64 {
        let total = self.bytes_to_hash.load(Ordering::Acquire);
        if total == 0 {
            return 0.0;
        }
        let done = self.bytes_hashed.load(Ordering::Acquire);
        (done as f64 / total as f64) * 100.0
    }
}

impl Default for DedupProgress {
    fn default() -> Self {
        Self::new()
    }
}

/// A group of duplicate files
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// Hash of the files (hex string)
    pub hash: String,
    /// Size of each file in bytes
    pub size: u64,
    /// Paths to all duplicate files
    pub files: Vec<PathBuf>,
}

impl DuplicateGroup {
    /// Total wasted space (size * (count - 1))
    pub fn wasted_bytes(&self) -> u64 {
        if self.files.len() > 1 {
            self.size * (self.files.len() - 1) as u64
        } else {
            0
        }
    }

    /// Get the file to keep (shortest path)
    pub fn keeper(&self) -> Option<&PathBuf> {
        self.files.iter().min_by_key(|p| p.to_string_lossy().len())
    }

    /// Get files to delete (all except keeper)
    pub fn duplicates_to_delete(&self) -> Vec<&PathBuf> {
        let keeper = self.keeper();
        self.files.iter().filter(|p| Some(*p) != keeper).collect()
    }
}

/// Result of duplicate finding
#[derive(Debug)]
pub struct DedupResult {
    /// Groups of duplicate files
    pub groups: Vec<DuplicateGroup>,
    /// Total files scanned
    pub files_scanned: usize,
    /// Total bytes scanned
    pub bytes_scanned: u64,
    /// Number of files that are duplicates
    pub duplicate_count: usize,
    /// Total wasted space
    pub wasted_bytes: u64,
}

impl DedupResult {
    /// Check if any duplicates were found
    pub fn has_duplicates(&self) -> bool {
        !self.groups.is_empty()
    }
}

/// Result of duplicate deletion
#[derive(Debug)]
pub struct DeleteResult {
    /// Number of files deleted
    pub files_deleted: usize,
    /// Bytes reclaimed
    pub bytes_reclaimed: u64,
    /// Number of errors
    pub errors: usize,
}

/// Error type for dedup operations
#[derive(Debug, thiserror::Error)]
pub enum DedupError {
    #[error("Scan error: {0}")]
    ScanError(#[from] crate::scanner::ScanError),

    #[error("Hash error: {0}")]
    HashError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Find duplicate files in a directory
pub fn find_duplicates(path: &Path, options: DedupOptions) -> Result<DedupResult, DedupError> {
    find_duplicates_with_progress(path, options, None)
}

/// Find duplicate files with real-time progress tracking
///
/// Progress updates:
/// - Phase 0 (Scanning): files_found, bytes_found increment as files are discovered
/// - Phase 1 (Hashing): files_hashed, bytes_hashed increment as files are hashed
/// - Phase 2 (Complete): operation finished
///
/// The progress can be polled from another thread to get real-time updates.
/// Set progress.cancelled to true to abort the operation early.
pub fn find_duplicates_with_progress(
    path: &Path,
    options: DedupOptions,
    progress: Option<Arc<DedupProgress>>,
) -> Result<DedupResult, DedupError> {
    // Create scan progress that mirrors to our dedup progress
    let scan_progress = progress.as_ref().map(|_p| Arc::new(CrawlProgress::new()));

    // Scan directory with progress
    let scan_options = ScanOptions {
        max_depth: options.max_depth.unwrap_or(usize::MAX),
        ..Default::default()
    };

    // Check cancellation before scan
    if let Some(ref p) = progress
        && p.is_cancelled()
    {
        return Ok(DedupResult {
            groups: vec![],
            files_scanned: 0,
            bytes_scanned: 0,
            duplicate_count: 0,
            wasted_bytes: 0,
        });
    }

    // Stream entries and update progress in real-time (instead of blocking on collect)
    let iter = scan_with_progress(path, scan_options, scan_progress.clone())?;
    let mut entries = Vec::new();

    for result in iter {
        match result {
            Ok(entry) => {
                if let Some(ref p) = progress {
                    p.files_found.fetch_add(1, Ordering::Relaxed);
                    p.bytes_found.fetch_add(entry.size, Ordering::Relaxed);
                }
                entries.push(entry);
            }
            Err(e) => {
                tracing::warn!("Dedup scan: skipping inaccessible path: {}", e);
            }
        }
    }

    let files_scanned = entries.len();
    let bytes_scanned: u64 = entries.iter().map(|e| e.size).sum();

    // Check cancellation after scan
    if let Some(ref p) = progress
        && p.is_cancelled()
    {
        return Ok(DedupResult {
            groups: vec![],
            files_scanned,
            bytes_scanned,
            duplicate_count: 0,
            wasted_bytes: 0,
        });
    }

    // Filter by minimum size and type
    let type_filter = options.type_filter;
    let entries: Vec<_> = entries
        .into_iter()
        .filter(|e| e.size >= options.min_size)
        .filter(|e| {
            // If no type filter, include all files
            let Some(filter) = type_filter else {
                return true;
            };
            // Check if file matches the type filter
            matches_type_filter(&e.absolute_path, filter)
        })
        .collect();

    // Group by size (fast filter)
    let mut size_groups: HashMap<u64, Vec<FileEntry>> = HashMap::new();
    for entry in entries {
        size_groups.entry(entry.size).or_default().push(entry);
    }

    // Keep only groups with 2+ files (potential duplicates)
    let potential_duplicates: Vec<Vec<FileEntry>> = size_groups
        .into_values()
        .filter(|group| group.len() > 1)
        .collect();

    // Calculate total bytes to hash and update progress
    let total_potential: usize = potential_duplicates.iter().map(|g| g.len()).sum();
    let total_bytes_to_hash: u64 = potential_duplicates
        .iter()
        .flat_map(|g| g.iter())
        .map(|e| e.size)
        .sum();

    if let Some(ref p) = progress {
        p.potential_duplicates
            .store(total_potential, Ordering::Release);
        p.bytes_to_hash
            .store(total_bytes_to_hash, Ordering::Release);
        p.phase.store(1, Ordering::Release); // Switch to hashing phase
    }

    // If skip_hash is enabled, group by size only (blazing fast mode)
    let groups: Vec<DuplicateGroup> = if options.skip_hash {
        // Quick mode: group by size only, use size as "hash" identifier
        potential_duplicates
            .into_iter()
            .map(|entries| {
                let size = entries.first().map(|e| e.size).unwrap_or(0);
                let files = entries.into_iter().map(|e| e.absolute_path).collect();
                // Use size as hash identifier for quick mode
                DuplicateGroup {
                    hash: format!("size:{}", size),
                    size,
                    files,
                }
            })
            .collect()
    } else {
        // Full mode: hash files in parallel and group by hash
        let hash_algorithm = options.hash_algorithm;
        let verify = options.verify;
        let progress_for_hash = progress.clone();

        // Hash files in parallel - wrap in catch_unwind for panic safety (early exit strategy)
        let hash_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            potential_duplicates
                .into_par_iter()
                .flat_map(|group| {
                    group
                        .into_par_iter()
                        .filter_map(|entry| {
                            // Check cancellation
                            if let Some(ref p) = progress_for_hash
                                && p.is_cancelled()
                            {
                                return None;
                            }

                            let size = entry.size;

                            hash_file_for_dedup(&entry, hash_algorithm, verify).map(|hash| {
                                // Update progress after successful hash
                                if let Some(ref p) = progress_for_hash {
                                    p.files_hashed.fetch_add(1, Ordering::Release);
                                    p.bytes_hashed.fetch_add(size, Ordering::Release);
                                }
                                (hash, entry)
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<(String, FileEntry)>>()
        }));

        // If a panic occurred during parallel hashing, propagate it
        // The global panic handler will ensure clean process exit
        let hash_results: Vec<(String, FileEntry)> = match hash_result {
            Ok(results) => results,
            Err(panic_payload) => {
                eprintln!("\n⚠ Fatal error during parallel dedup hashing");
                panic::resume_unwind(panic_payload);
            }
        };

        // Group by hash
        let mut hash_groups: HashMap<String, Vec<FileEntry>> = HashMap::new();
        for (hash, entry) in hash_results {
            hash_groups.entry(hash).or_default().push(entry);
        }

        // Convert to DuplicateGroups (only groups with 2+ files)
        hash_groups
            .into_iter()
            .filter(|(_, entries)| entries.len() > 1)
            .map(|(hash, entries)| {
                let size = entries.first().map(|e| e.size).unwrap_or(0);
                let files = entries.into_iter().map(|e| e.absolute_path).collect();
                DuplicateGroup { hash, size, files }
            })
            .collect()
    };

    // Calculate stats
    let duplicate_count: usize = groups.iter().map(|g| g.files.len() - 1).sum();
    let wasted_bytes: u64 = groups.iter().map(|g| g.wasted_bytes()).sum();

    // Mark complete
    if let Some(ref p) = progress {
        p.phase.store(2, Ordering::Release);
    }

    Ok(DedupResult {
        groups,
        files_scanned,
        bytes_scanned,
        duplicate_count,
        wasted_bytes,
    })
}

/// Hash a file for dedup purposes
fn hash_file_for_dedup(
    entry: &FileEntry,
    algorithm: HashAlgorithm,
    verify: bool,
) -> Option<String> {
    // Use thread-local buffer
    thread_local! {
        static BUFFER: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(vec![0u8; 128 * 1024]);
    }

    BUFFER.with(|buf| {
        let mut buffer = buf.borrow_mut();

        let result = hash_file_with_buffer(&entry.absolute_path, algorithm, &mut buffer);

        match result {
            Ok(hash_result) => {
                if verify {
                    // Double-check by hashing again (paranoid mode)
                    let verify_result =
                        hash_file_with_buffer(&entry.absolute_path, algorithm, &mut buffer);
                    match verify_result {
                        Ok(verify_hash) if verify_hash.hash == hash_result.hash => {
                            Some(hash_result.to_hex())
                        }
                        _ => None, // Hash mismatch or error, skip this file
                    }
                } else {
                    Some(hash_result.to_hex())
                }
            }
            Err(e) => {
                tracing::warn!("Failed to hash {}: {}", entry.absolute_path.display(), e);
                None
            }
        }
    })
}

/// Delete duplicate files (keeps one copy per group - shortest path)
pub fn delete_duplicates(result: &DedupResult) -> DeleteResult {
    let files_to_delete: Vec<(&PathBuf, u64)> = result
        .groups
        .iter()
        .flat_map(|group| {
            group
                .duplicates_to_delete()
                .into_iter()
                .map(|p| (p, group.size))
        })
        .collect();

    // Parallel deletion - wrap in catch_unwind for panic safety (early exit strategy)
    let delete_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        files_to_delete
            .par_iter()
            .map(|(path, size)| match fs::remove_file(path) {
                Ok(()) => Ok(*size),
                Err(e) => {
                    tracing::warn!("Failed to delete {}: {}", path.display(), e);
                    Err(())
                }
            })
            .collect::<Vec<Result<u64, ()>>>()
    }));

    // If a panic occurred during parallel deletion, propagate it
    // The global panic handler will ensure clean process exit
    let delete_results: Vec<Result<u64, ()>> = match delete_result {
        Ok(results) => results,
        Err(panic_payload) => {
            eprintln!("\n⚠ Fatal error during parallel duplicate deletion");
            panic::resume_unwind(panic_payload);
        }
    };

    let mut files_deleted = 0;
    let mut bytes_reclaimed = 0;
    let mut errors = 0;

    for result in delete_results {
        match result {
            Ok(size) => {
                files_deleted += 1;
                bytes_reclaimed += size;
            }
            Err(()) => errors += 1,
        }
    }

    DeleteResult {
        files_deleted,
        bytes_reclaimed,
        errors,
    }
}

/// Check if a file path matches a type filter
fn matches_type_filter(path: &std::path::Path, filter: FileTypeCategory) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    let ext = match ext {
        Some(e) => e,
        None => return false, // No extension, can't match type filters
    };

    match filter {
        FileTypeCategory::Images => matches!(
            ext.as_str(),
            "jpg"
                | "jpeg"
                | "png"
                | "gif"
                | "bmp"
                | "svg"
                | "webp"
                | "ico"
                | "tiff"
                | "tif"
                | "heic"
                | "heif"
                | "raw"
                | "cr2"
                | "nef"
                | "arw"
                | "dng"
                | "psd"
                | "ai"
                | "eps"
        ),
        FileTypeCategory::Videos => matches!(
            ext.as_str(),
            "mp4"
                | "mov"
                | "avi"
                | "mkv"
                | "wmv"
                | "flv"
                | "webm"
                | "m4v"
                | "mpeg"
                | "mpg"
                | "3gp"
                | "ogv"
                | "ts"
                | "mts"
                | "m2ts"
        ),
        FileTypeCategory::Audio => matches!(
            ext.as_str(),
            "mp3"
                | "wav"
                | "flac"
                | "aac"
                | "ogg"
                | "wma"
                | "m4a"
                | "aiff"
                | "opus"
                | "mid"
                | "midi"
                | "ape"
                | "alac"
        ),
        FileTypeCategory::Documents => matches!(
            ext.as_str(),
            "pdf"
                | "doc"
                | "docx"
                | "xls"
                | "xlsx"
                | "ppt"
                | "pptx"
                | "odt"
                | "ods"
                | "odp"
                | "rtf"
                | "txt"
                | "md"
                | "markdown"
                | "csv"
                | "pages"
                | "numbers"
                | "key"
                | "epub"
                | "mobi"
        ),
        FileTypeCategory::Code => matches!(
            ext.as_str(),
            "rs" | "py"
                | "js"
                | "ts"
                | "jsx"
                | "tsx"
                | "java"
                | "c"
                | "cpp"
                | "h"
                | "hpp"
                | "go"
                | "rb"
                | "php"
                | "swift"
                | "kt"
                | "scala"
                | "cs"
                | "vb"
                | "lua"
                | "pl"
                | "sh"
                | "bash"
                | "zsh"
                | "fish"
                | "ps1"
                | "bat"
                | "cmd"
                | "html"
                | "htm"
                | "css"
                | "scss"
                | "sass"
                | "less"
                | "vue"
                | "svelte"
                | "sql"
                | "r"
                | "m"
                | "mm"
                | "zig"
                | "nim"
                | "d"
                | "ex"
                | "exs"
                | "erl"
                | "clj"
                | "hs"
                | "ml"
        ),
        FileTypeCategory::Archives => matches!(
            ext.as_str(),
            "zip"
                | "tar"
                | "gz"
                | "bz2"
                | "xz"
                | "7z"
                | "rar"
                | "tgz"
                | "tbz2"
                | "txz"
                | "lz"
                | "lzma"
                | "cab"
                | "iso"
                | "dmg"
                | "pkg"
                | "deb"
                | "rpm"
        ),
        FileTypeCategory::Config => matches!(
            ext.as_str(),
            "json"
                | "yaml"
                | "yml"
                | "toml"
                | "xml"
                | "ini"
                | "conf"
                | "cfg"
                | "env"
                | "properties"
                | "plist"
        ),
        FileTypeCategory::Executables => matches!(
            ext.as_str(),
            "exe"
                | "dll"
                | "so"
                | "dylib"
                | "app"
                | "bin"
                | "msi"
                | "apk"
                | "ipa"
                | "deb"
                | "rpm"
                | "appimage"
                | "snap"
                | "flatpak"
                | "wasm"
        ),
        FileTypeCategory::Fonts => matches!(
            ext.as_str(),
            "ttf" | "otf" | "woff" | "woff2" | "eot" | "fon" | "fnt"
        ),
        FileTypeCategory::Files => true,        // All files match
        FileTypeCategory::Directories => false, // Files never match directory filter
        // Language-specific filters
        FileTypeCategory::Rust => ext == "rs",
        FileTypeCategory::Go => ext == "go",
        FileTypeCategory::Swift => ext == "swift",
        FileTypeCategory::TypeScript => matches!(ext.as_str(), "ts" | "tsx"),
        FileTypeCategory::JavaScript => matches!(ext.as_str(), "js" | "jsx" | "mjs" | "cjs"),
        FileTypeCategory::Python => ext == "py",
    }
}

#[cfg(test)]
#[path = "finder_test.rs"]
mod finder_test;
