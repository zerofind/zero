//! Storage backend abstraction for local and cloud storage
//!
//! This module provides a unified interface for different storage backends:
//! - Local filesystem (default, uses `std::fs`)
//! - Cloud storage via `OpenDAL` (S3, B2, Dropbox, GCS, etc.)
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                     StorageBackend trait                        │
//! │  read(), write(), list(), stat(), delete(), exists()           │
//! └─────────────────────────────────────────────────────────────────┘
//!                          │
//!          ┌───────────────┴───────────────┐
//!          ▼                               ▼
//! ┌─────────────────┐           ┌─────────────────────┐
//! │  LocalStorage   │           │  OpenDalStorage     │
//! │  (std::fs)      │           │  (S3, B2, etc.)     │
//! └─────────────────┘           └─────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use storage::{StorageBackend, LocalStorage};
//!
//! // Local storage
//! let local = LocalStorage::new("/path/to/root");
//! let data = local.read("file.txt").await?;
//!
//! // Cloud storage (with cloud feature)
//! let cloud = OpenDalStorage::s3("bucket", "region", credentials)?;
//! cloud.write("backup/file.txt", data).await?;
//! ```

mod backend;
mod local;
mod types;

#[cfg(feature = "cloud")]
mod opendal_backend;

pub use backend::StorageBackend;
pub use local::LocalStorage;
pub use types::{
    ListOptions, ReadOptions, StorageEntry, StorageError, StorageMetadata, WriteOptions,
};

#[cfg(feature = "cloud")]
pub use opendal_backend::OpenDalStorage;

/// Parse a storage URL into a backend and path
///
/// Supported formats:
/// - `/path/to/dir` or `./relative` - Local filesystem
/// - `s3://bucket/path` - Amazon S3
/// - `b2://bucket/path` - Backblaze B2
/// - `gs://bucket/path` - Google Cloud Storage
/// - `file:///path` - Explicit local filesystem
///
/// Returns the appropriate storage backend and the path within that backend.
#[cfg(feature = "cloud")]
pub fn parse_storage_url(
    url: &str,
) -> Result<(Box<dyn StorageBackend + Send + Sync>, String), StorageError> {
    use std::path::Path;

    // Check for scheme
    if let Some((scheme, rest)) = url.split_once("://") {
        match scheme {
            "file" => {
                // file:///path/to/dir -> local storage
                let path = rest.trim_start_matches('/');
                let (root, subpath) = split_root_and_path(path);
                Ok((Box::new(LocalStorage::new(root)), subpath))
            }
            _ => {
                // Cloud storage - delegate to OpenDAL
                OpenDalStorage::from_url(url).map(|(backend, path)| {
                    (
                        Box::new(backend) as Box<dyn StorageBackend + Send + Sync>,
                        path,
                    )
                })
            }
        }
    } else {
        // No scheme - treat as local path
        let path = Path::new(url);
        let canonical = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };

        Ok((Box::new(LocalStorage::new(&canonical)), String::new()))
    }
}

/// Non-cloud version - only supports local storage
#[cfg(not(feature = "cloud"))]
pub fn parse_storage_url(
    url: &str,
) -> Result<(Box<dyn StorageBackend + Send + Sync>, String), StorageError> {
    use std::path::Path;

    // Check for unsupported cloud schemes
    if url.contains("://") && !url.starts_with("file://") {
        return Err(StorageError::UnsupportedScheme {
            scheme: url.split("://").next().unwrap_or("unknown").to_string(),
            hint: "Enable the 'cloud' feature for cloud storage support".to_string(),
        });
    }

    // Handle file:// or plain paths
    let path_str = url.strip_prefix("file://").unwrap_or(url);
    let path = Path::new(path_str);
    let canonical = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };

    Ok((Box::new(LocalStorage::new(&canonical)), String::new()))
}

/// Split a path into root directory and subpath
#[allow(dead_code)]
fn split_root_and_path(path: &str) -> (String, String) {
    // For now, use the entire path as root
    // In future, could be smarter about bucket/container boundaries
    (format!("/{path}"), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_local_path() {
        let (_, path) = parse_storage_url("/tmp/test").unwrap();
        assert_eq!(path, "");
    }

    #[test]
    fn test_parse_file_url() {
        let (_, path) = parse_storage_url("file:///tmp/test").unwrap();
        assert_eq!(path, "");
    }

    #[cfg(not(feature = "cloud"))]
    #[test]
    fn test_unsupported_scheme_without_cloud() {
        let result = parse_storage_url("s3://bucket/path");
        assert!(matches!(
            result,
            Err(StorageError::UnsupportedScheme { .. })
        ));
    }
}
