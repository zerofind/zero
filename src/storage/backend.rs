//! StorageBackend trait - the core abstraction for all storage backends
//!
//! This trait defines the interface that all storage backends must implement,
//! whether local filesystem, S3, B2, Dropbox, or any other storage service.

use std::future::Future;
use std::pin::Pin;

use super::types::{
    ListOptions, ReadOptions, StorageEntry, StorageError, StorageMetadata, WriteOptions,
};

/// Type alias for async results
pub type StorageResult<T> = Result<T, StorageError>;

/// Type alias for boxed futures (for trait object compatibility)
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Core storage backend trait
///
/// This trait provides a unified interface for all storage backends.
/// All operations are async to support both local and remote storage.
///
/// # Path Conventions
///
/// - Paths are relative to the storage root
/// - Use forward slashes `/` as separators (even on Windows)
/// - Paths should not start with `/` (they're relative)
/// - Directory paths should end with `/` for clarity
///
/// # Examples
///
/// ```ignore
/// use zero::storage::{StorageBackend, LocalStorage};
///
/// let storage = LocalStorage::new("/backup");
///
/// // Check if file exists
/// let exists = storage.exists("photos/vacation.jpg").await?;
///
/// // Read file contents
/// let data = storage.read("documents/report.pdf").await?;
///
/// // Write file
/// storage.write("backup/data.txt", b"hello world").await?;
///
/// // List directory
/// let entries = storage.list("photos/").await?;
/// ```
pub trait StorageBackend: Send + Sync {
    /// Get the name/identifier of this storage backend
    fn name(&self) -> &str;

    /// Get the root path of this storage
    fn root(&self) -> &str;

    /// Get the scheme (e.g., "file", "s3", "b2")
    fn scheme(&self) -> &str;

    /// Check if a path exists
    fn exists<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<bool>>;

    /// Get metadata for a path
    fn stat<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<StorageMetadata>>;

    /// Read entire file contents
    fn read<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<Vec<u8>>>;

    /// Read file contents with options (range, progress)
    fn read_with_options<'a>(
        &'a self,
        path: &'a str,
        options: ReadOptions,
    ) -> BoxFuture<'a, StorageResult<Vec<u8>>>;

    /// Write data to a file
    fn write<'a>(
        &'a self,
        path: &'a str,
        data: &'a [u8],
    ) -> BoxFuture<'a, StorageResult<StorageMetadata>>;

    /// Write data to a file with options
    fn write_with_options<'a>(
        &'a self,
        path: &'a str,
        data: &'a [u8],
        options: WriteOptions,
    ) -> BoxFuture<'a, StorageResult<StorageMetadata>>;

    /// Delete a file or empty directory
    fn delete<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<()>>;

    /// Delete a directory and all its contents recursively
    fn delete_recursive<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<()>>;

    /// Create a directory (and parents if needed)
    fn create_dir<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<()>>;

    /// List entries in a directory
    fn list<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<Vec<StorageEntry>>>;

    /// List entries with options (recursive, filters)
    fn list_with_options<'a>(
        &'a self,
        path: &'a str,
        options: ListOptions,
    ) -> BoxFuture<'a, StorageResult<Vec<StorageEntry>>>;

    /// Copy a file within the same storage backend
    fn copy<'a>(
        &'a self,
        from: &'a str,
        to: &'a str,
    ) -> BoxFuture<'a, StorageResult<StorageMetadata>>;

    /// Move/rename a file within the same storage backend
    fn rename<'a>(&'a self, from: &'a str, to: &'a str) -> BoxFuture<'a, StorageResult<()>>;

    // =========================================================================
    // Default implementations for convenience methods
    // =========================================================================

    /// Check if path is a file
    fn is_file<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<bool>> {
        Box::pin(async move {
            match self.stat(path).await {
                Ok(meta) => Ok(!meta.is_dir),
                Err(StorageError::NotFound { .. }) => Ok(false),
                Err(e) => Err(e),
            }
        })
    }

    /// Check if path is a directory
    fn is_dir<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<bool>> {
        Box::pin(async move {
            match self.stat(path).await {
                Ok(meta) => Ok(meta.is_dir),
                Err(StorageError::NotFound { .. }) => Ok(false),
                Err(e) => Err(e),
            }
        })
    }

    /// Get file size (returns 0 for directories or non-existent)
    fn size<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<u64>> {
        Box::pin(async move {
            match self.stat(path).await {
                Ok(meta) => Ok(meta.size),
                Err(StorageError::NotFound { .. }) => Ok(0),
                Err(e) => Err(e),
            }
        })
    }

    /// Read file as UTF-8 string
    fn read_string<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<String>> {
        Box::pin(async move {
            let data = self.read(path).await?;
            String::from_utf8(data).map_err(|e| StorageError::Io {
                message: format!("Invalid UTF-8: {}", e),
                source: None,
            })
        })
    }

    /// Write string to file
    fn write_string<'a>(
        &'a self,
        path: &'a str,
        content: &'a str,
    ) -> BoxFuture<'a, StorageResult<StorageMetadata>> {
        Box::pin(async move { self.write(path, content.as_bytes()).await })
    }
}

/// Extension trait for cross-storage operations
#[allow(dead_code)]
pub trait StorageBackendExt: StorageBackend {
    /// Copy a file from this storage to another storage backend
    fn copy_to<'a>(
        &'a self,
        from_path: &'a str,
        to_storage: &'a dyn StorageBackend,
        to_path: &'a str,
    ) -> BoxFuture<'a, StorageResult<StorageMetadata>> {
        Box::pin(async move {
            let data = self.read(from_path).await?;
            to_storage.write(to_path, &data).await
        })
    }

    /// Copy a file from another storage backend to this one
    fn copy_from<'a>(
        &'a self,
        from_storage: &'a dyn StorageBackend,
        from_path: &'a str,
        to_path: &'a str,
    ) -> BoxFuture<'a, StorageResult<StorageMetadata>> {
        Box::pin(async move {
            let data = from_storage.read(from_path).await?;
            self.write(to_path, &data).await
        })
    }
}

// Blanket implementation
impl<T: StorageBackend + ?Sized> StorageBackendExt for T {}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time check that the trait is object-safe
    fn _assert_object_safe(_: &dyn StorageBackend) {}
}
