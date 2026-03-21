//! `LocalStorage` implementation - wraps `std::fs` for local filesystem operations
//!
//! This is the default storage backend for zero, providing fast local file
//! operations using the standard library's filesystem APIs.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::backend::{BoxFuture, StorageBackend, StorageResult};
use super::types::{
    ListOptions, ReadOptions, StorageEntry, StorageError, StorageMetadata, WriteOptions,
};

/// Local filesystem storage backend
///
/// Provides storage operations on the local filesystem, using the given
/// root directory as the base for all relative paths.
///
/// # Example
///
/// ```ignore
/// use zero::storage::LocalStorage;
///
/// let storage = LocalStorage::new("/home/user/backup");
///
/// // Read a file
/// let data = storage.read("documents/file.txt").await?;
///
/// // Write a file
/// storage.write("backup/data.bin", &data).await?;
/// ```
#[derive(Debug, Clone)]
pub struct LocalStorage {
    /// Root directory for this storage
    root: PathBuf,
}

impl LocalStorage {
    /// Create a new local storage backend rooted at the given path
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Get the full path for a relative path
    fn full_path(&self, path: &str) -> PathBuf {
        if path.is_empty() {
            self.root.clone()
        } else {
            self.root.join(path)
        }
    }

    /// Convert `std::fs::Metadata` to `StorageMetadata`
    fn to_storage_metadata(meta: fs::Metadata) -> StorageMetadata {
        let mut storage_meta = if meta.is_dir() {
            StorageMetadata::directory()
        } else {
            StorageMetadata::file(meta.len())
        };

        if let Ok(modified) = meta.modified() {
            storage_meta = storage_meta.with_modified(modified);
        }

        if let Ok(created) = meta.created() {
            storage_meta = storage_meta.with_created(created);
        }

        storage_meta
    }

    /// Add path context to storage errors
    fn with_path(err: StorageError, path: &str) -> StorageError {
        match err {
            StorageError::NotFound { .. } => StorageError::NotFound {
                path: path.to_string(),
            },
            StorageError::PermissionDenied { .. } => StorageError::PermissionDenied {
                path: path.to_string(),
            },
            StorageError::AlreadyExists { .. } => StorageError::AlreadyExists {
                path: path.to_string(),
            },
            other => other,
        }
    }
}

impl StorageBackend for LocalStorage {
    fn name(&self) -> &'static str {
        "local"
    }

    fn root(&self) -> &str {
        self.root.to_str().unwrap_or("")
    }

    fn scheme(&self) -> &'static str {
        "file"
    }

    fn exists<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<bool>> {
        Box::pin(async move {
            let full_path = self.full_path(path);
            Ok(full_path.exists())
        })
    }

    fn stat<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<StorageMetadata>> {
        Box::pin(async move {
            let full_path = self.full_path(path);
            let meta = fs::metadata(&full_path).map_err(|e| Self::with_path(e.into(), path))?;
            Ok(Self::to_storage_metadata(meta))
        })
    }

    fn read<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<Vec<u8>>> {
        Box::pin(async move {
            let full_path = self.full_path(path);
            fs::read(&full_path).map_err(|e| Self::with_path(e.into(), path))
        })
    }

    fn read_with_options<'a>(
        &'a self,
        path: &'a str,
        options: ReadOptions,
    ) -> BoxFuture<'a, StorageResult<Vec<u8>>> {
        Box::pin(async move {
            let full_path = self.full_path(path);

            let mut file = File::open(&full_path).map_err(|e| Self::with_path(e.into(), path))?;

            let metadata = file
                .metadata()
                .map_err(|e| Self::with_path(e.into(), path))?;
            let total_size = metadata.len();

            // Handle range reads
            let (start, length) = match options.range {
                Some((start, end)) => {
                    use std::io::Seek;
                    file.seek(std::io::SeekFrom::Start(start))
                        .map_err(|e| Self::with_path(e.into(), path))?;
                    (start, (end - start) as usize)
                }
                None => (0, total_size as usize),
            };

            let mut buffer = vec![0u8; length];
            let mut bytes_read = 0;

            // Read in chunks for progress reporting
            const CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks

            while bytes_read < length {
                let chunk_len = std::cmp::min(CHUNK_SIZE, length - bytes_read);
                let n = file
                    .read(&mut buffer[bytes_read..bytes_read + chunk_len])
                    .map_err(|e| Self::with_path(e.into(), path))?;

                if n == 0 {
                    break; // EOF
                }

                bytes_read += n;

                // Report progress
                if let Some(ref callback) = options.on_progress {
                    callback(start + bytes_read as u64, total_size);
                }
            }

            buffer.truncate(bytes_read);
            Ok(buffer)
        })
    }

    fn write<'a>(
        &'a self,
        path: &'a str,
        data: &'a [u8],
    ) -> BoxFuture<'a, StorageResult<StorageMetadata>> {
        Box::pin(async move {
            let full_path = self.full_path(path);

            // Create parent directories if needed
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).map_err(|e| Self::with_path(e.into(), path))?;
            }

            // Write the file
            fs::write(&full_path, data).map_err(|e| Self::with_path(e.into(), path))?;

            // Return metadata
            let meta = fs::metadata(&full_path).map_err(|e| Self::with_path(e.into(), path))?;
            Ok(Self::to_storage_metadata(meta))
        })
    }

    fn write_with_options<'a>(
        &'a self,
        path: &'a str,
        data: &'a [u8],
        options: WriteOptions,
    ) -> BoxFuture<'a, StorageResult<StorageMetadata>> {
        Box::pin(async move {
            let full_path = self.full_path(path);

            // Check if file exists and overwrite is disabled
            if !options.overwrite && full_path.exists() {
                return Err(StorageError::AlreadyExists {
                    path: path.to_string(),
                });
            }

            // Create parent directories if needed
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).map_err(|e| Self::with_path(e.into(), path))?;
            }

            // Write the file
            let mut file = File::create(&full_path).map_err(|e| Self::with_path(e.into(), path))?;

            file.write_all(data)
                .map_err(|e| Self::with_path(e.into(), path))?;

            file.flush().map_err(|e| Self::with_path(e.into(), path))?;

            // Return metadata
            let meta = fs::metadata(&full_path).map_err(|e| Self::with_path(e.into(), path))?;
            Ok(Self::to_storage_metadata(meta))
        })
    }

    fn delete<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<()>> {
        Box::pin(async move {
            let full_path = self.full_path(path);

            if full_path.is_dir() {
                fs::remove_dir(&full_path).map_err(|e| Self::with_path(e.into(), path))
            } else {
                fs::remove_file(&full_path).map_err(|e| Self::with_path(e.into(), path))
            }
        })
    }

    fn delete_recursive<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<()>> {
        Box::pin(async move {
            let full_path = self.full_path(path);

            if full_path.is_dir() {
                fs::remove_dir_all(&full_path).map_err(|e| Self::with_path(e.into(), path))
            } else {
                fs::remove_file(&full_path).map_err(|e| Self::with_path(e.into(), path))
            }
        })
    }

    fn create_dir<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<()>> {
        Box::pin(async move {
            let full_path = self.full_path(path);
            fs::create_dir_all(&full_path).map_err(|e| Self::with_path(e.into(), path))
        })
    }

    fn list<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<Vec<StorageEntry>>> {
        Box::pin(async move { self.list_with_options(path, ListOptions::default()).await })
    }

    fn list_with_options<'a>(
        &'a self,
        path: &'a str,
        options: ListOptions,
    ) -> BoxFuture<'a, StorageResult<Vec<StorageEntry>>> {
        Box::pin(async move {
            let full_path = self.full_path(path);

            if !full_path.exists() {
                return Err(StorageError::NotFound {
                    path: path.to_string(),
                });
            }

            let mut entries = Vec::new();

            if options.recursive {
                self.list_recursive(&full_path, &full_path, &options, &mut entries)?;
            } else {
                self.list_single_dir(&full_path, &full_path, &options, &mut entries)?;
            }

            // Apply limit if specified
            if let Some(limit) = options.limit {
                entries.truncate(limit);
            }

            Ok(entries)
        })
    }

    fn copy<'a>(
        &'a self,
        from: &'a str,
        to: &'a str,
    ) -> BoxFuture<'a, StorageResult<StorageMetadata>> {
        Box::pin(async move {
            let from_path = self.full_path(from);
            let to_path = self.full_path(to);

            // Create parent directories if needed
            if let Some(parent) = to_path.parent() {
                fs::create_dir_all(parent).map_err(|e| Self::with_path(e.into(), to))?;
            }

            // Use std::fs::copy for efficient copying (uses OS optimizations)
            fs::copy(&from_path, &to_path).map_err(|e| Self::with_path(e.into(), from))?;

            // Return metadata of the new file
            let meta = fs::metadata(&to_path).map_err(|e| Self::with_path(e.into(), to))?;
            Ok(Self::to_storage_metadata(meta))
        })
    }

    fn rename<'a>(&'a self, from: &'a str, to: &'a str) -> BoxFuture<'a, StorageResult<()>> {
        Box::pin(async move {
            let from_path = self.full_path(from);
            let to_path = self.full_path(to);

            // Create parent directories if needed
            if let Some(parent) = to_path.parent() {
                fs::create_dir_all(parent).map_err(|e| Self::with_path(e.into(), to))?;
            }

            fs::rename(&from_path, &to_path).map_err(|e| Self::with_path(e.into(), from))
        })
    }
}

impl LocalStorage {
    /// List entries in a single directory (non-recursive)
    fn list_single_dir(
        &self,
        dir_path: &Path,
        root: &Path,
        options: &ListOptions,
        entries: &mut Vec<StorageEntry>,
    ) -> StorageResult<()> {
        let read_dir = fs::read_dir(dir_path).map_err(|e| {
            Self::with_path(
                e.into(),
                dir_path
                    .strip_prefix(root)
                    .unwrap_or(dir_path)
                    .to_str()
                    .unwrap_or(""),
            )
        })?;

        for entry_result in read_dir {
            let entry = entry_result.map_err(|e| StorageError::Io {
                message: e.to_string(),
                source: Some(e),
            })?;

            let metadata = entry.metadata().map_err(|e| StorageError::Io {
                message: e.to_string(),
                source: Some(e),
            })?;

            let is_dir = metadata.is_dir();

            // Apply filters
            if options.files_only && is_dir {
                continue;
            }
            if options.dirs_only && !is_dir {
                continue;
            }

            let rel_path = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(&entry.path())
                .to_path_buf();

            let storage_meta = Self::to_storage_metadata(metadata);
            entries.push(StorageEntry::new(rel_path, storage_meta));
        }

        Ok(())
    }

    /// List entries recursively
    fn list_recursive(
        &self,
        dir_path: &Path,
        root: &Path,
        options: &ListOptions,
        entries: &mut Vec<StorageEntry>,
    ) -> StorageResult<()> {
        let read_dir = fs::read_dir(dir_path).map_err(|e| {
            Self::with_path(
                e.into(),
                dir_path
                    .strip_prefix(root)
                    .unwrap_or(dir_path)
                    .to_str()
                    .unwrap_or(""),
            )
        })?;

        for entry_result in read_dir {
            let entry = entry_result.map_err(|e| StorageError::Io {
                message: e.to_string(),
                source: Some(e),
            })?;

            let metadata = entry.metadata().map_err(|e| StorageError::Io {
                message: e.to_string(),
                source: Some(e),
            })?;

            let is_dir = metadata.is_dir();

            // Apply filters
            let include = if options.files_only {
                !is_dir
            } else if options.dirs_only {
                is_dir
            } else {
                true
            };

            if include {
                let rel_path = entry
                    .path()
                    .strip_prefix(root)
                    .unwrap_or(&entry.path())
                    .to_path_buf();

                let storage_meta = Self::to_storage_metadata(metadata);
                entries.push(StorageEntry::new(rel_path, storage_meta));
            }

            // Recurse into directories
            if is_dir {
                self.list_recursive(&entry.path(), root, options, entries)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, LocalStorage) {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path());
        (temp_dir, storage)
    }

    #[tokio::test]
    async fn test_write_and_read() {
        let (_temp, storage) = setup();

        let data = b"Hello, World!";
        storage.write("test.txt", data).await.unwrap();

        let read_data = storage.read("test.txt").await.unwrap();
        assert_eq!(read_data, data);
    }

    #[tokio::test]
    async fn test_exists() {
        let (_temp, storage) = setup();

        assert!(!storage.exists("nonexistent.txt").await.unwrap());

        storage.write("exists.txt", b"data").await.unwrap();
        assert!(storage.exists("exists.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_stat() {
        let (_temp, storage) = setup();

        let data = b"Test content";
        storage.write("file.txt", data).await.unwrap();

        let meta = storage.stat("file.txt").await.unwrap();
        assert!(!meta.is_dir);
        assert_eq!(meta.size, data.len() as u64);
    }

    #[tokio::test]
    async fn test_delete() {
        let (_temp, storage) = setup();

        storage.write("to_delete.txt", b"data").await.unwrap();
        assert!(storage.exists("to_delete.txt").await.unwrap());

        storage.delete("to_delete.txt").await.unwrap();
        assert!(!storage.exists("to_delete.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_create_dir() {
        let (_temp, storage) = setup();

        storage.create_dir("subdir/nested").await.unwrap();

        let meta = storage.stat("subdir/nested").await.unwrap();
        assert!(meta.is_dir);
    }

    #[tokio::test]
    async fn test_list() {
        let (_temp, storage) = setup();

        storage.write("file1.txt", b"1").await.unwrap();
        storage.write("file2.txt", b"2").await.unwrap();
        storage.create_dir("subdir").await.unwrap();

        let entries = storage.list("").await.unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[tokio::test]
    async fn test_list_recursive() {
        let (_temp, storage) = setup();

        storage.write("root.txt", b"root").await.unwrap();
        storage.write("subdir/nested.txt", b"nested").await.unwrap();
        storage
            .write("subdir/deep/file.txt", b"deep")
            .await
            .unwrap();

        let entries = storage
            .list_with_options("", ListOptions::new().recursive())
            .await
            .unwrap();

        // Should find all files and directories
        assert!(entries.len() >= 3);
    }

    #[tokio::test]
    async fn test_copy() {
        let (_temp, storage) = setup();

        let data = b"Copy me!";
        storage.write("original.txt", data).await.unwrap();

        storage.copy("original.txt", "copied.txt").await.unwrap();

        let copied_data = storage.read("copied.txt").await.unwrap();
        assert_eq!(copied_data, data);
    }

    #[tokio::test]
    async fn test_rename() {
        let (_temp, storage) = setup();

        storage.write("old_name.txt", b"data").await.unwrap();

        storage
            .rename("old_name.txt", "new_name.txt")
            .await
            .unwrap();

        assert!(!storage.exists("old_name.txt").await.unwrap());
        assert!(storage.exists("new_name.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_nested_write() {
        let (_temp, storage) = setup();

        // Should auto-create parent directories
        storage.write("a/b/c/deep.txt", b"nested").await.unwrap();

        assert!(storage.exists("a/b/c/deep.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_not_found_error() {
        let (_temp, storage) = setup();

        let result = storage.read("nonexistent.txt").await;
        assert!(matches!(result, Err(StorageError::NotFound { .. })));
    }

    #[tokio::test]
    async fn test_read_string() {
        let (_temp, storage) = setup();

        storage.write("text.txt", b"Hello").await.unwrap();

        let content = storage.read_string("text.txt").await.unwrap();
        assert_eq!(content, "Hello");
    }

    #[tokio::test]
    async fn test_list_files_only() {
        let (_temp, storage) = setup();

        storage.write("file.txt", b"data").await.unwrap();
        storage.create_dir("dir").await.unwrap();

        let entries = storage
            .list_with_options("", ListOptions::new().files_only())
            .await
            .unwrap();

        assert_eq!(entries.len(), 1);
        assert!(!entries[0].is_dir());
    }

    #[tokio::test]
    async fn test_list_dirs_only() {
        let (_temp, storage) = setup();

        storage.write("file.txt", b"data").await.unwrap();
        storage.create_dir("dir").await.unwrap();

        let entries = storage
            .list_with_options("", ListOptions::new().dirs_only())
            .await
            .unwrap();

        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_dir());
    }
}
