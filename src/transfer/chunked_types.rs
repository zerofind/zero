//! Types and constants for chunked file transfer
//!
//! This module contains the shared types, error definitions, and constants
//! used by the chunked transfer implementation.

use crate::hasher::HashAlgorithm;

/// Default chunk size: 4MB
/// Balance between progress granularity and syscall overhead
pub const DEFAULT_CHUNK_SIZE: usize = 4 * 1024 * 1024;

/// Threshold for using chunked transfer: 100MB
/// Below this, restart cost is low enough that simple copy is fine
pub const DEFAULT_CHUNK_THRESHOLD: u64 = 100 * 1024 * 1024;

/// Buffer size for I/O operations: 128KB
pub const IO_BUFFER_SIZE: usize = 128 * 1024;

/// Progress information for chunked copy
#[derive(Debug, Clone)]
pub struct ChunkedProgress {
    /// Total bytes to copy
    pub total_bytes: u64,
    /// Bytes copied so far
    pub bytes_copied: u64,
    /// Current chunk number (0-indexed)
    pub current_chunk: usize,
    /// Total number of chunks
    pub total_chunks: usize,
    /// Chunk we resumed from (0 if fresh copy)
    pub resumed_from_chunk: usize,
    /// Whether the copy is complete
    pub complete: bool,
}

impl ChunkedProgress {
    /// Percentage complete (0.0 to 100.0)
    pub fn percent(&self) -> f64 {
        if self.total_bytes == 0 {
            100.0
        } else {
            (self.bytes_copied as f64 / self.total_bytes as f64) * 100.0
        }
    }

    /// Bytes saved by resuming (not re-transferred)
    pub fn bytes_saved(&self) -> u64 {
        (self.resumed_from_chunk * DEFAULT_CHUNK_SIZE) as u64
    }
}

/// Error type for chunked copy operations
#[derive(Debug, thiserror::Error)]
pub enum ChunkedCopyError {
    #[error("Failed to open source file: {path}")]
    SourceOpenError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to create destination file: {path}")]
    DestCreateError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to read from source: {path}")]
    ReadError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to write to destination: {path}")]
    WriteError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to get file metadata: {path}")]
    MetadataError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to seek in file: {path}")]
    SeekError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to truncate file: {path}")]
    TruncateError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Checksum mismatch after copy: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("Source file changed during transfer (size: {original} -> {current})")]
    SourceChanged { original: u64, current: u64 },

    #[error("Destination is larger than source ({dest_size} > {src_size})")]
    DestLargerThanSource { dest_size: u64, src_size: u64 },
}

/// Options for chunked copy
#[derive(Debug, Clone)]
pub struct ChunkedCopyOptions {
    /// Size of each chunk in bytes
    pub chunk_size: usize,
    /// Hash algorithm for verification
    pub hash_algorithm: HashAlgorithm,
    /// Whether to verify final file hash after copy
    pub verify_after_copy: bool,
    /// Whether to attempt resume (find resume point via chunk hashing)
    pub enable_resume: bool,
}

impl Default for ChunkedCopyOptions {
    fn default() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_SIZE,
            hash_algorithm: HashAlgorithm::Xxh3,
            verify_after_copy: true,
            enable_resume: true,
        }
    }
}

/// Result of a chunked copy operation
#[derive(Debug)]
pub struct ChunkedCopyResult {
    /// Total bytes in the file
    pub total_bytes: u64,
    /// Bytes actually transferred (after resume)
    pub bytes_transferred: u64,
    /// Hash of the complete file (if verified)
    pub file_hash: Option<String>,
    /// Chunk we resumed from (0 if fresh copy)
    pub resumed_from_chunk: usize,
    /// Total number of chunks
    pub total_chunks: usize,
    /// Number of chunks actually transferred
    pub chunks_transferred: usize,
}

impl ChunkedCopyResult {
    /// Bytes saved by resuming
    pub fn bytes_saved(&self) -> u64 {
        self.total_bytes.saturating_sub(self.bytes_transferred)
    }

    /// Whether any data was resumed (not a fresh copy)
    pub fn was_resumed(&self) -> bool {
        self.resumed_from_chunk > 0
    }
}

/// State for tracking chunked transfer progress (for persistence)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkedTransferState {
    /// Relative path of the file being transferred
    pub path: std::path::PathBuf,
    /// Total size of the file
    pub total_bytes: u64,
    /// Bytes transferred so far
    pub bytes_done: u64,
    /// Hash algorithm used
    pub hash_algorithm: String,
}

impl ChunkedTransferState {
    /// Create new transfer state
    pub fn new(path: std::path::PathBuf, total_bytes: u64) -> Self {
        Self {
            path,
            total_bytes,
            bytes_done: 0,
            hash_algorithm: "xxh3".to_string(),
        }
    }

    /// Check if transfer is complete
    pub fn is_complete(&self) -> bool {
        self.bytes_done >= self.total_bytes
    }

    /// Percentage complete
    pub fn percent(&self) -> f64 {
        if self.total_bytes == 0 {
            100.0
        } else {
            (self.bytes_done as f64 / self.total_bytes as f64) * 100.0
        }
    }
}

/// Check if a file should use chunked transfer based on size
#[inline]
pub fn should_use_chunked(file_size: u64, threshold: u64) -> bool {
    file_size >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunked_progress_percent_empty() {
        let progress = ChunkedProgress {
            total_bytes: 0,
            bytes_copied: 0,
            current_chunk: 0,
            total_chunks: 0,
            resumed_from_chunk: 0,
            complete: true,
        };
        assert!((progress.percent() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_chunked_progress_percent_half() {
        let progress = ChunkedProgress {
            total_bytes: 100,
            bytes_copied: 50,
            current_chunk: 0,
            total_chunks: 1,
            resumed_from_chunk: 0,
            complete: false,
        };
        assert!((progress.percent() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_chunked_progress_bytes_saved() {
        let progress = ChunkedProgress {
            total_bytes: 100,
            bytes_copied: 100,
            current_chunk: 2,
            total_chunks: 3,
            resumed_from_chunk: 2,
            complete: true,
        };
        assert_eq!(progress.bytes_saved(), 2 * DEFAULT_CHUNK_SIZE as u64);
    }

    #[test]
    fn test_chunked_copy_options_default() {
        let options = ChunkedCopyOptions::default();
        assert_eq!(options.chunk_size, DEFAULT_CHUNK_SIZE);
        assert!(options.verify_after_copy);
        assert!(options.enable_resume);
    }

    #[test]
    fn test_chunked_copy_result_bytes_saved() {
        let result = ChunkedCopyResult {
            total_bytes: 1000,
            bytes_transferred: 400,
            file_hash: None,
            resumed_from_chunk: 2,
            total_chunks: 4,
            chunks_transferred: 2,
        };
        assert_eq!(result.bytes_saved(), 600);
        assert!(result.was_resumed());
    }

    #[test]
    fn test_chunked_copy_result_no_resume() {
        let result = ChunkedCopyResult {
            total_bytes: 1000,
            bytes_transferred: 1000,
            file_hash: Some("abc123".to_string()),
            resumed_from_chunk: 0,
            total_chunks: 4,
            chunks_transferred: 4,
        };
        assert_eq!(result.bytes_saved(), 0);
        assert!(!result.was_resumed());
    }

    #[test]
    fn test_chunked_transfer_state_new() {
        let state = ChunkedTransferState::new(std::path::PathBuf::from("test.txt"), 1000);
        assert_eq!(state.path, std::path::PathBuf::from("test.txt"));
        assert_eq!(state.total_bytes, 1000);
        assert_eq!(state.bytes_done, 0);
        assert!(!state.is_complete());
    }

    #[test]
    fn test_chunked_transfer_state_complete() {
        let mut state = ChunkedTransferState::new(std::path::PathBuf::from("test.txt"), 1000);
        state.bytes_done = 1000;
        assert!(state.is_complete());
        assert!((state.percent() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_chunked_transfer_state_percent() {
        let mut state = ChunkedTransferState::new(std::path::PathBuf::from("test.txt"), 100);
        state.bytes_done = 25;
        assert!((state.percent() - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_should_use_chunked() {
        assert!(!should_use_chunked(
            50 * 1024 * 1024,
            DEFAULT_CHUNK_THRESHOLD
        ));
        assert!(should_use_chunked(
            100 * 1024 * 1024,
            DEFAULT_CHUNK_THRESHOLD
        ));
        assert!(should_use_chunked(
            200 * 1024 * 1024,
            DEFAULT_CHUNK_THRESHOLD
        ));
    }
}
