//! Storage types for the storage backend abstraction

use std::path::PathBuf;
use std::time::SystemTime;

/// Error type for storage operations
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// File or directory not found
    #[error("Not found: {path}")]
    NotFound { path: String },

    /// Permission denied
    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    /// Path already exists (for create operations)
    #[error("Already exists: {path}")]
    AlreadyExists { path: String },

    /// Invalid path format
    #[error("Invalid path: {path} - {reason}")]
    InvalidPath { path: String, reason: String },

    /// Storage scheme not supported
    #[error("Unsupported storage scheme '{scheme}': {hint}")]
    UnsupportedScheme { scheme: String, hint: String },

    /// Configuration error
    #[error("Configuration error: {message}")]
    ConfigError { message: String },

    /// Authentication/authorization error
    #[error("Authentication failed: {message}")]
    AuthError { message: String },

    /// Network error (for remote storage)
    #[error("Network error: {message}")]
    NetworkError { message: String },

    /// Rate limited by remote service
    #[error("Rate limited: retry after {retry_after_secs:?} seconds")]
    RateLimited { retry_after_secs: Option<u64> },

    /// Storage quota exceeded
    #[error("Quota exceeded: {message}")]
    QuotaExceeded { message: String },

    /// Generic I/O error
    #[error("I/O error: {message}")]
    Io {
        message: String,
        #[source]
        source: Option<std::io::Error>,
    },

    /// Operation not supported by this backend
    #[error("Operation not supported: {operation}")]
    Unsupported { operation: String },

    /// Backend-specific error
    #[error("Backend error: {message}")]
    Backend { message: String },
}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        use std::io::ErrorKind;

        match err.kind() {
            ErrorKind::NotFound => StorageError::NotFound {
                path: String::new(),
            },
            ErrorKind::PermissionDenied => StorageError::PermissionDenied {
                path: String::new(),
            },
            ErrorKind::AlreadyExists => StorageError::AlreadyExists {
                path: String::new(),
            },
            _ => StorageError::Io {
                message: err.to_string(),
                source: Some(err),
            },
        }
    }
}

/// Metadata about a file or directory in storage
#[derive(Debug, Clone)]
pub struct StorageMetadata {
    /// Size in bytes (0 for directories)
    pub size: u64,

    /// Whether this is a directory
    pub is_dir: bool,

    /// Last modified time (if available)
    pub modified: Option<SystemTime>,

    /// Creation time (if available)
    pub created: Option<SystemTime>,

    /// Content type / MIME type (if available, mainly for cloud storage)
    pub content_type: Option<String>,

    /// `ETag` or version identifier (if available, mainly for cloud storage)
    pub etag: Option<String>,

    /// Content hash (if available without additional computation)
    pub content_hash: Option<String>,
}

impl StorageMetadata {
    /// Create metadata for a file
    pub fn file(size: u64) -> Self {
        Self {
            size,
            is_dir: false,
            modified: None,
            created: None,
            content_type: None,
            etag: None,
            content_hash: None,
        }
    }

    /// Create metadata for a directory
    pub fn directory() -> Self {
        Self {
            size: 0,
            is_dir: true,
            modified: None,
            created: None,
            content_type: None,
            etag: None,
            content_hash: None,
        }
    }

    /// Set the modified time
    pub fn with_modified(mut self, time: SystemTime) -> Self {
        self.modified = Some(time);
        self
    }

    /// Set the created time
    pub fn with_created(mut self, time: SystemTime) -> Self {
        self.created = Some(time);
        self
    }

    /// Set the content type
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Set the etag
    pub fn with_etag(mut self, etag: impl Into<String>) -> Self {
        self.etag = Some(etag.into());
        self
    }

    /// Get modified time as Unix timestamp (seconds)
    pub fn modified_secs(&self) -> Option<i64> {
        self.modified.and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs() as i64)
        })
    }
}

impl Default for StorageMetadata {
    fn default() -> Self {
        Self::file(0)
    }
}

/// An entry in a directory listing
#[derive(Debug, Clone)]
pub struct StorageEntry {
    /// Path relative to the listing root
    pub path: PathBuf,

    /// Name of the file/directory (last component of path)
    pub name: String,

    /// Metadata about the entry
    pub metadata: StorageMetadata,
}

impl StorageEntry {
    /// Create a new storage entry
    pub fn new(path: impl Into<PathBuf>, metadata: StorageMetadata) -> Self {
        let path = path.into();
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        Self {
            path,
            name,
            metadata,
        }
    }

    /// Check if this entry is a directory
    pub fn is_dir(&self) -> bool {
        self.metadata.is_dir
    }

    /// Check if this entry is a file
    pub fn is_file(&self) -> bool {
        !self.metadata.is_dir
    }

    /// Get the size of the entry
    pub fn size(&self) -> u64 {
        self.metadata.size
    }
}

/// Options for write operations
#[derive(Debug, Clone, Default)]
pub struct WriteOptions {
    /// Content type / MIME type
    pub content_type: Option<String>,

    /// Whether to overwrite existing files
    pub overwrite: bool,

    /// Custom metadata (key-value pairs)
    pub metadata: Vec<(String, String)>,
}

impl WriteOptions {
    /// Create default write options (overwrite enabled)
    pub fn new() -> Self {
        Self {
            overwrite: true,
            ..Default::default()
        }
    }

    /// Set content type
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Set overwrite behavior
    pub fn with_overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// Add custom metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.push((key.into(), value.into()));
        self
    }
}

/// Options for list operations
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// Whether to list recursively
    pub recursive: bool,

    /// Maximum number of entries to return
    pub limit: Option<usize>,

    /// Only include files (not directories)
    pub files_only: bool,

    /// Only include directories (not files)
    pub dirs_only: bool,
}

impl ListOptions {
    /// Create default list options
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable recursive listing
    pub fn recursive(mut self) -> Self {
        self.recursive = true;
        self
    }

    /// Set a limit on results
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Only list files
    pub fn files_only(mut self) -> Self {
        self.files_only = true;
        self.dirs_only = false;
        self
    }

    /// Only list directories
    pub fn dirs_only(mut self) -> Self {
        self.dirs_only = true;
        self.files_only = false;
        self
    }
}

/// Progress callback for read/write operations
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Options for read operations
#[derive(Default)]
pub struct ReadOptions {
    /// Byte range to read (start, end exclusive)
    pub range: Option<(u64, u64)>,

    /// Progress callback (`bytes_read`, `total_bytes`)
    pub on_progress: Option<ProgressCallback>,
}

impl ReadOptions {
    /// Create default read options
    pub fn new() -> Self {
        Self::default()
    }

    /// Set byte range to read
    pub fn with_range(mut self, start: u64, end: u64) -> Self {
        self.range = Some((start, end));
        self
    }

    /// Set progress callback
    pub fn with_progress<F>(mut self, callback: F) -> Self
    where
        F: Fn(u64, u64) + Send + Sync + 'static,
    {
        self.on_progress = Some(Box::new(callback));
        self
    }
}

impl std::fmt::Debug for ReadOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadOptions")
            .field("range", &self.range)
            .field("on_progress", &self.on_progress.is_some())
            .finish()
    }
}
