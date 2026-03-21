//! Data types for the dedup module.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use search::FileTypeCategory;

/// Options for duplicate finding
#[derive(Debug, Clone)]
pub struct DedupOptions {
    /// Hash algorithm to use
    pub hash_algorithm: hasher::HashAlgorithm,
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
            hash_algorithm: hasher::HashAlgorithm::default(),
            max_depth: None,
            min_size: 4096, // Skip tiny files — negligible space savings, huge size-groups
            verify: false,
            type_filter: None,
            skip_hash: false,
        }
    }
}

impl DedupOptions {
    pub fn images() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Images),
            ..Default::default()
        }
    }

    pub fn videos() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Videos),
            ..Default::default()
        }
    }

    pub fn audio() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Audio),
            ..Default::default()
        }
    }

    pub fn documents() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Documents),
            ..Default::default()
        }
    }

    pub fn code() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Code),
            ..Default::default()
        }
    }

    pub fn archives() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Archives),
            ..Default::default()
        }
    }

    pub fn executables() -> Self {
        Self {
            type_filter: Some(FileTypeCategory::Executables),
            ..Default::default()
        }
    }

    pub fn with_type(mut self, type_filter: FileTypeCategory) -> Self {
        self.type_filter = Some(type_filter);
        self
    }

    pub fn with_min_size(mut self, min_size: u64) -> Self {
        self.min_size = min_size;
        self
    }

    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = Some(max_depth);
        self
    }

    pub fn with_verify(mut self) -> Self {
        self.verify = true;
        self
    }

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

    pub fn phase_name(&self) -> &'static str {
        match self.phase.load(Ordering::Acquire) {
            0 => "Scanning",
            1 => "Hashing",
            2 => "Complete",
            _ => "Unknown",
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

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
    ScanError(#[from] scanner::ScanError),

    #[error("Hash error: {0}")]
    HashError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}
