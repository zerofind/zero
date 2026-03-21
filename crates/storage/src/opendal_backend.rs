//! OpenDAL storage backend implementation
//!
//! This module provides cloud storage support via Apache OpenDAL,
//! enabling zero to work with S3, B2, GCS, Dropbox, and many other backends.
//!
//! # Supported Backends
//!
//! - Amazon S3 (and S3-compatible: MinIO, R2, etc.)
//! - Backblaze B2
//! - Google Cloud Storage
//! - Dropbox
//! - WebDAV
//! - SFTP
//! - And many more via OpenDAL
//!
//! # Example
//!
//! ```ignore
//! use zero::storage::OpenDalStorage;
//!
//! // Create S3 backend
//! let storage = OpenDalStorage::s3()
//!     .bucket("my-bucket")
//!     .region("us-east-1")
//!     .access_key_id("...")
//!     .secret_access_key("...")
//!     .build()?;
//!
//! // Read a file
//! let data = storage.read("path/to/file.txt").await?;
//! ```

use opendal::Operator;
use opendal::services;

use super::backend::{BoxFuture, StorageBackend, StorageResult};
use super::types::{
    ListOptions, ReadOptions, StorageEntry, StorageError, StorageMetadata, WriteOptions,
};

/// OpenDAL-based storage backend for cloud storage
///
/// Supports multiple cloud storage providers through Apache OpenDAL.
#[derive(Clone)]
pub struct OpenDalStorage {
    /// The OpenDAL operator
    operator: Operator,
    /// Storage scheme (s3, b2, gcs, etc.)
    scheme: String,
    /// Human-readable name
    name: String,
    /// Root path
    root: String,
}

impl std::fmt::Debug for OpenDalStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenDalStorage")
            .field("scheme", &self.scheme)
            .field("name", &self.name)
            .field("root", &self.root)
            .finish()
    }
}

impl OpenDalStorage {
    /// Create a new OpenDAL storage from an existing operator
    pub fn from_operator(op: Operator, scheme: &str, name: &str) -> Self {
        let info = op.info();
        Self {
            operator: op,
            scheme: scheme.to_string(),
            name: name.to_string(),
            root: info.root().to_string(),
        }
    }

    /// Create an S3 storage builder
    pub fn s3() -> S3Builder {
        S3Builder::new()
    }

    /// Create a B2 storage builder
    pub fn b2() -> B2Builder {
        B2Builder::new()
    }

    /// Create a GCS storage builder
    pub fn gcs() -> GcsBuilder {
        GcsBuilder::new()
    }

    /// Create a Dropbox storage builder
    pub fn dropbox() -> DropboxBuilder {
        DropboxBuilder::new()
    }

    /// Create a WebDAV storage builder
    pub fn webdav() -> WebDavBuilder {
        WebDavBuilder::new()
    }

    /// Create a memory storage (useful for testing)
    pub fn memory() -> StorageResult<Self> {
        let builder = services::Memory::default();
        let operator = Operator::new(builder)
            .map_err(|e| StorageError::ConfigError {
                message: e.to_string(),
            })?
            .finish();

        Ok(Self {
            operator,
            scheme: "memory".to_string(),
            name: "memory".to_string(),
            root: "/".to_string(),
        })
    }

    /// Parse a storage URL and return appropriate backend
    ///
    /// Supported URL formats:
    /// - `s3://bucket/path` - Amazon S3
    /// - `b2://bucket/path` - Backblaze B2
    /// - `gs://bucket/path` - Google Cloud Storage
    /// - `dropbox://path` - Dropbox
    ///
    /// Note: Credentials must be provided via environment variables or config.
    pub fn from_url(url: &str) -> StorageResult<(Self, String)> {
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| StorageError::InvalidPath {
                path: url.to_string(),
                reason: "Missing scheme (e.g., s3://, b2://)".to_string(),
            })?;

        match scheme {
            "s3" => {
                let (bucket, path) = parse_bucket_and_path(rest)?;
                let storage = Self::s3().bucket(&bucket).with_env().build()?;
                Ok((storage, path))
            }
            "b2" => {
                let (bucket, path) = parse_bucket_and_path(rest)?;
                let storage = Self::b2().bucket(&bucket).with_env().build()?;
                Ok((storage, path))
            }
            "gs" | "gcs" => {
                let (bucket, path) = parse_bucket_and_path(rest)?;
                let storage = Self::gcs().bucket(&bucket).with_env().build()?;
                Ok((storage, path))
            }
            "dropbox" => {
                let path = rest.to_string();
                let storage = Self::dropbox().with_env().build()?;
                Ok((storage, path))
            }
            "webdav" | "dav" => {
                // webdav://host/path
                let storage = Self::webdav()
                    .endpoint(&format!("https://{}", rest))
                    .build()?;
                Ok((storage, String::new()))
            }
            _ => Err(StorageError::UnsupportedScheme {
                scheme: scheme.to_string(),
                hint: format!(
                    "Supported schemes: s3, b2, gs, gcs, dropbox, webdav. Got: {}",
                    scheme
                ),
            }),
        }
    }

    /// Convert OpenDAL error to StorageError
    fn convert_error(err: opendal::Error, path: &str) -> StorageError {
        use opendal::ErrorKind;

        match err.kind() {
            ErrorKind::NotFound => StorageError::NotFound {
                path: path.to_string(),
            },
            ErrorKind::PermissionDenied => StorageError::PermissionDenied {
                path: path.to_string(),
            },
            ErrorKind::AlreadyExists => StorageError::AlreadyExists {
                path: path.to_string(),
            },
            ErrorKind::RateLimited => StorageError::RateLimited {
                retry_after_secs: None,
            },
            ErrorKind::Unsupported => StorageError::Unsupported {
                operation: err.to_string(),
            },
            ErrorKind::ConfigInvalid => StorageError::ConfigError {
                message: err.to_string(),
            },
            _ => StorageError::Backend {
                message: err.to_string(),
            },
        }
    }

    /// Convert OpenDAL metadata to StorageMetadata
    fn convert_metadata(meta: opendal::Metadata) -> StorageMetadata {
        let mut storage_meta = if meta.is_dir() {
            StorageMetadata::directory()
        } else {
            StorageMetadata::file(meta.content_length())
        };

        if let Some(modified) = meta.last_modified() {
            // Convert from chrono to SystemTime
            let timestamp = modified.timestamp();
            if let Some(system_time) =
                std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(timestamp as u64))
            {
                storage_meta = storage_meta.with_modified(system_time);
            }
        }

        if let Some(content_type) = meta.content_type() {
            storage_meta = storage_meta.with_content_type(content_type);
        }

        if let Some(etag) = meta.etag() {
            storage_meta = storage_meta.with_etag(etag);
        }

        storage_meta
    }
}

impl StorageBackend for OpenDalStorage {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &str {
        &self.root
    }

    fn scheme(&self) -> &str {
        &self.scheme
    }

    fn exists<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<bool>> {
        Box::pin(async move {
            self.operator
                .exists(path)
                .await
                .map_err(|e| Self::convert_error(e, path))
        })
    }

    fn stat<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<StorageMetadata>> {
        Box::pin(async move {
            let meta = self
                .operator
                .stat(path)
                .await
                .map_err(|e| Self::convert_error(e, path))?;
            Ok(Self::convert_metadata(meta))
        })
    }

    fn read<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<Vec<u8>>> {
        Box::pin(async move {
            let buffer = self
                .operator
                .read(path)
                .await
                .map_err(|e| Self::convert_error(e, path))?;
            Ok(buffer.to_vec())
        })
    }

    fn read_with_options<'a>(
        &'a self,
        path: &'a str,
        options: ReadOptions,
    ) -> BoxFuture<'a, StorageResult<Vec<u8>>> {
        Box::pin(async move {
            // Apply range if specified
            let buffer = if let Some((start, end)) = options.range {
                self.operator
                    .read_with(path)
                    .range(start..end)
                    .await
                    .map_err(|e| Self::convert_error(e, path))?
            } else {
                self.operator
                    .read(path)
                    .await
                    .map_err(|e| Self::convert_error(e, path))?
            };

            // Note: Progress callback is not easily supported with opendal's current API
            // Would need to use a streaming reader for that

            Ok(buffer.to_vec())
        })
    }

    fn write<'a>(
        &'a self,
        path: &'a str,
        data: &'a [u8],
    ) -> BoxFuture<'a, StorageResult<StorageMetadata>> {
        Box::pin(async move {
            self.operator
                .write(path, data.to_vec())
                .await
                .map_err(|e| Self::convert_error(e, path))?;

            // Get metadata of written file
            self.stat(path).await
        })
    }

    fn write_with_options<'a>(
        &'a self,
        path: &'a str,
        data: &'a [u8],
        options: WriteOptions,
    ) -> BoxFuture<'a, StorageResult<StorageMetadata>> {
        Box::pin(async move {
            // Check existence if overwrite is disabled
            if !options.overwrite && self.exists(path).await? {
                return Err(StorageError::AlreadyExists {
                    path: path.to_string(),
                });
            }

            let mut writer = self.operator.write_with(path, data.to_vec());

            // Apply content type if specified
            if let Some(ref content_type) = options.content_type {
                writer = writer.content_type(content_type);
            }

            writer.await.map_err(|e| Self::convert_error(e, path))?;

            // Get metadata of written file
            self.stat(path).await
        })
    }

    fn delete<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<()>> {
        Box::pin(async move {
            self.operator
                .delete(path)
                .await
                .map_err(|e| Self::convert_error(e, path))
        })
    }

    fn delete_recursive<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<()>> {
        Box::pin(async move {
            self.operator
                .remove_all(path)
                .await
                .map_err(|e| Self::convert_error(e, path))
        })
    }

    fn create_dir<'a>(&'a self, path: &'a str) -> BoxFuture<'a, StorageResult<()>> {
        Box::pin(async move {
            // Ensure path ends with /
            let dir_path = if path.ends_with('/') {
                path.to_string()
            } else {
                format!("{}/", path)
            };

            self.operator
                .create_dir(&dir_path)
                .await
                .map_err(|e| Self::convert_error(e, path))
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
            let mut list_builder = self.operator.list_with(path);

            if options.recursive {
                list_builder = list_builder.recursive(true);
            }

            let entries_list = list_builder
                .await
                .map_err(|e| Self::convert_error(e, path))?;

            let mut entries = Vec::new();
            let mut count = 0;

            for entry in entries_list {
                let meta = entry.metadata();
                let is_dir = meta.is_dir();

                // Apply filters
                if options.files_only && is_dir {
                    continue;
                }
                if options.dirs_only && !is_dir {
                    continue;
                }

                let storage_meta = Self::convert_metadata(meta.clone());
                entries.push(StorageEntry::new(entry.path(), storage_meta));

                count += 1;
                if options.limit.is_some_and(|limit| count >= limit) {
                    break;
                }
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
            self.operator
                .copy(from, to)
                .await
                .map_err(|e| Self::convert_error(e, from))?;

            self.stat(to).await
        })
    }

    fn rename<'a>(&'a self, from: &'a str, to: &'a str) -> BoxFuture<'a, StorageResult<()>> {
        Box::pin(async move {
            self.operator
                .rename(from, to)
                .await
                .map_err(|e| Self::convert_error(e, from))
        })
    }
}

// =============================================================================
// Builder types for each supported backend
// =============================================================================

/// Builder for S3 storage
#[derive(Default)]
pub struct S3Builder {
    bucket: Option<String>,
    region: Option<String>,
    endpoint: Option<String>,
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
    root: Option<String>,
}

impl S3Builder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bucket(mut self, bucket: &str) -> Self {
        self.bucket = Some(bucket.to_string());
        self
    }

    pub fn region(mut self, region: &str) -> Self {
        self.region = Some(region.to_string());
        self
    }

    pub fn endpoint(mut self, endpoint: &str) -> Self {
        self.endpoint = Some(endpoint.to_string());
        self
    }

    pub fn access_key_id(mut self, key: &str) -> Self {
        self.access_key_id = Some(key.to_string());
        self
    }

    pub fn secret_access_key(mut self, secret: &str) -> Self {
        self.secret_access_key = Some(secret.to_string());
        self
    }

    pub fn root(mut self, root: &str) -> Self {
        self.root = Some(root.to_string());
        self
    }

    /// Load credentials from environment variables
    ///
    /// Looks for:
    /// - AWS_ACCESS_KEY_ID
    /// - AWS_SECRET_ACCESS_KEY
    /// - AWS_REGION (optional)
    /// - AWS_ENDPOINT (optional)
    pub fn with_env(mut self) -> Self {
        if let Ok(key) = std::env::var("AWS_ACCESS_KEY_ID") {
            self.access_key_id = Some(key);
        }
        if let Ok(secret) = std::env::var("AWS_SECRET_ACCESS_KEY") {
            self.secret_access_key = Some(secret);
        }
        if let Ok(region) = std::env::var("AWS_REGION") {
            self.region = Some(region);
        }
        if let Ok(endpoint) = std::env::var("AWS_ENDPOINT") {
            self.endpoint = Some(endpoint);
        }
        self
    }

    pub fn build(self) -> StorageResult<OpenDalStorage> {
        let bucket = self.bucket.ok_or_else(|| StorageError::ConfigError {
            message: "S3 bucket is required".to_string(),
        })?;

        let mut builder = services::S3::default().bucket(&bucket);

        if let Some(region) = self.region {
            builder = builder.region(&region);
        }
        if let Some(endpoint) = self.endpoint {
            builder = builder.endpoint(&endpoint);
        }
        if let Some(key) = self.access_key_id {
            builder = builder.access_key_id(&key);
        }
        if let Some(secret) = self.secret_access_key {
            builder = builder.secret_access_key(&secret);
        }
        if let Some(root) = self.root {
            builder = builder.root(&root);
        }

        let operator = Operator::new(builder)
            .map_err(|e| StorageError::ConfigError {
                message: e.to_string(),
            })?
            .finish();

        Ok(OpenDalStorage::from_operator(operator, "s3", &bucket))
    }
}

/// Builder for Backblaze B2 storage
#[derive(Default)]
pub struct B2Builder {
    bucket: Option<String>,
    bucket_id: Option<String>,
    application_key_id: Option<String>,
    application_key: Option<String>,
    root: Option<String>,
}

impl B2Builder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bucket(mut self, bucket: &str) -> Self {
        self.bucket = Some(bucket.to_string());
        self
    }

    pub fn bucket_id(mut self, id: &str) -> Self {
        self.bucket_id = Some(id.to_string());
        self
    }

    pub fn application_key_id(mut self, key_id: &str) -> Self {
        self.application_key_id = Some(key_id.to_string());
        self
    }

    pub fn application_key(mut self, key: &str) -> Self {
        self.application_key = Some(key.to_string());
        self
    }

    pub fn root(mut self, root: &str) -> Self {
        self.root = Some(root.to_string());
        self
    }

    /// Load credentials from environment variables
    ///
    /// Looks for:
    /// - B2_APPLICATION_KEY_ID
    /// - B2_APPLICATION_KEY
    /// - B2_BUCKET_ID (optional)
    pub fn with_env(mut self) -> Self {
        if let Ok(key_id) = std::env::var("B2_APPLICATION_KEY_ID") {
            self.application_key_id = Some(key_id);
        }
        if let Ok(key) = std::env::var("B2_APPLICATION_KEY") {
            self.application_key = Some(key);
        }
        if let Ok(bucket_id) = std::env::var("B2_BUCKET_ID") {
            self.bucket_id = Some(bucket_id);
        }
        self
    }

    pub fn build(self) -> StorageResult<OpenDalStorage> {
        let bucket = self.bucket.ok_or_else(|| StorageError::ConfigError {
            message: "B2 bucket name is required".to_string(),
        })?;

        let mut builder = services::B2::default().bucket(&bucket);

        if let Some(bucket_id) = self.bucket_id {
            builder = builder.bucket_id(&bucket_id);
        }
        if let Some(key_id) = self.application_key_id {
            builder = builder.application_key_id(&key_id);
        }
        if let Some(key) = self.application_key {
            builder = builder.application_key(&key);
        }
        if let Some(root) = self.root {
            builder = builder.root(&root);
        }

        let operator = Operator::new(builder)
            .map_err(|e| StorageError::ConfigError {
                message: e.to_string(),
            })?
            .finish();

        Ok(OpenDalStorage::from_operator(operator, "b2", &bucket))
    }
}

/// Builder for Google Cloud Storage
#[derive(Default)]
pub struct GcsBuilder {
    bucket: Option<String>,
    credential: Option<String>,
    credential_path: Option<String>,
    root: Option<String>,
}

impl GcsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bucket(mut self, bucket: &str) -> Self {
        self.bucket = Some(bucket.to_string());
        self
    }

    pub fn credential(mut self, credential: &str) -> Self {
        self.credential = Some(credential.to_string());
        self
    }

    pub fn credential_path(mut self, path: &str) -> Self {
        self.credential_path = Some(path.to_string());
        self
    }

    pub fn root(mut self, root: &str) -> Self {
        self.root = Some(root.to_string());
        self
    }

    /// Load credentials from environment variables
    ///
    /// Looks for:
    /// - GOOGLE_APPLICATION_CREDENTIALS (path to credentials file)
    pub fn with_env(mut self) -> Self {
        if let Ok(path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
            self.credential_path = Some(path);
        }
        self
    }

    pub fn build(self) -> StorageResult<OpenDalStorage> {
        let bucket = self.bucket.ok_or_else(|| StorageError::ConfigError {
            message: "GCS bucket is required".to_string(),
        })?;

        let mut builder = services::Gcs::default().bucket(&bucket);

        if let Some(credential) = self.credential {
            builder = builder.credential(&credential);
        }
        if let Some(path) = self.credential_path {
            builder = builder.credential_path(&path);
        }
        if let Some(root) = self.root {
            builder = builder.root(&root);
        }

        let operator = Operator::new(builder)
            .map_err(|e| StorageError::ConfigError {
                message: e.to_string(),
            })?
            .finish();

        Ok(OpenDalStorage::from_operator(operator, "gcs", &bucket))
    }
}

/// Builder for Dropbox storage
#[derive(Default)]
pub struct DropboxBuilder {
    access_token: Option<String>,
    refresh_token: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    root: Option<String>,
}

impl DropboxBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn access_token(mut self, token: &str) -> Self {
        self.access_token = Some(token.to_string());
        self
    }

    pub fn refresh_token(mut self, token: &str) -> Self {
        self.refresh_token = Some(token.to_string());
        self
    }

    pub fn client_id(mut self, id: &str) -> Self {
        self.client_id = Some(id.to_string());
        self
    }

    pub fn client_secret(mut self, secret: &str) -> Self {
        self.client_secret = Some(secret.to_string());
        self
    }

    pub fn root(mut self, root: &str) -> Self {
        self.root = Some(root.to_string());
        self
    }

    /// Load credentials from environment variables
    ///
    /// Looks for:
    /// - DROPBOX_ACCESS_TOKEN
    /// - DROPBOX_REFRESH_TOKEN (optional)
    /// - DROPBOX_CLIENT_ID (optional)
    /// - DROPBOX_CLIENT_SECRET (optional)
    pub fn with_env(mut self) -> Self {
        if let Ok(token) = std::env::var("DROPBOX_ACCESS_TOKEN") {
            self.access_token = Some(token);
        }
        if let Ok(token) = std::env::var("DROPBOX_REFRESH_TOKEN") {
            self.refresh_token = Some(token);
        }
        if let Ok(id) = std::env::var("DROPBOX_CLIENT_ID") {
            self.client_id = Some(id);
        }
        if let Ok(secret) = std::env::var("DROPBOX_CLIENT_SECRET") {
            self.client_secret = Some(secret);
        }
        self
    }

    pub fn build(self) -> StorageResult<OpenDalStorage> {
        let mut builder = services::Dropbox::default();

        if let Some(token) = self.access_token {
            builder = builder.access_token(&token);
        }
        if let Some(token) = self.refresh_token {
            builder = builder.refresh_token(&token);
        }
        if let Some(id) = self.client_id {
            builder = builder.client_id(&id);
        }
        if let Some(secret) = self.client_secret {
            builder = builder.client_secret(&secret);
        }
        if let Some(root) = self.root {
            builder = builder.root(&root);
        }

        let operator = Operator::new(builder)
            .map_err(|e| StorageError::ConfigError {
                message: e.to_string(),
            })?
            .finish();

        Ok(OpenDalStorage::from_operator(
            operator, "dropbox", "dropbox",
        ))
    }
}

/// Builder for WebDAV storage
#[derive(Default)]
pub struct WebDavBuilder {
    endpoint: Option<String>,
    username: Option<String>,
    password: Option<String>,
    root: Option<String>,
}

impl WebDavBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn endpoint(mut self, endpoint: &str) -> Self {
        self.endpoint = Some(endpoint.to_string());
        self
    }

    pub fn username(mut self, username: &str) -> Self {
        self.username = Some(username.to_string());
        self
    }

    pub fn password(mut self, password: &str) -> Self {
        self.password = Some(password.to_string());
        self
    }

    pub fn root(mut self, root: &str) -> Self {
        self.root = Some(root.to_string());
        self
    }

    /// Load credentials from environment variables
    ///
    /// Looks for:
    /// - WEBDAV_ENDPOINT
    /// - WEBDAV_USERNAME
    /// - WEBDAV_PASSWORD
    pub fn with_env(mut self) -> Self {
        if let Ok(endpoint) = std::env::var("WEBDAV_ENDPOINT") {
            self.endpoint = Some(endpoint);
        }
        if let Ok(username) = std::env::var("WEBDAV_USERNAME") {
            self.username = Some(username);
        }
        if let Ok(password) = std::env::var("WEBDAV_PASSWORD") {
            self.password = Some(password);
        }
        self
    }

    pub fn build(self) -> StorageResult<OpenDalStorage> {
        let endpoint = self.endpoint.ok_or_else(|| StorageError::ConfigError {
            message: "WebDAV endpoint is required".to_string(),
        })?;

        let mut builder = services::Webdav::default().endpoint(&endpoint);

        if let Some(username) = self.username {
            builder = builder.username(&username);
        }
        if let Some(password) = self.password {
            builder = builder.password(&password);
        }
        if let Some(root) = self.root {
            builder = builder.root(&root);
        }

        let operator = Operator::new(builder)
            .map_err(|e| StorageError::ConfigError {
                message: e.to_string(),
            })?
            .finish();

        Ok(OpenDalStorage::from_operator(operator, "webdav", &endpoint))
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Parse bucket and path from a URL path component
fn parse_bucket_and_path(rest: &str) -> StorageResult<(String, String)> {
    let rest = rest.trim_start_matches('/');

    if rest.is_empty() {
        return Err(StorageError::InvalidPath {
            path: rest.to_string(),
            reason: "Bucket name is required".to_string(),
        });
    }

    match rest.split_once('/') {
        Some((bucket, path)) => Ok((bucket.to_string(), path.to_string())),
        None => Ok((rest.to_string(), String::new())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bucket_and_path() {
        let (bucket, path) = parse_bucket_and_path("mybucket/some/path").unwrap();
        assert_eq!(bucket, "mybucket");
        assert_eq!(path, "some/path");

        let (bucket, path) = parse_bucket_and_path("mybucket").unwrap();
        assert_eq!(bucket, "mybucket");
        assert_eq!(path, "");

        let (bucket, path) = parse_bucket_and_path("/mybucket/path").unwrap();
        assert_eq!(bucket, "mybucket");
        assert_eq!(path, "path");
    }

    #[tokio::test]
    async fn test_memory_storage() {
        let storage = OpenDalStorage::memory().unwrap();

        // Test write
        storage.write("test.txt", b"Hello, World!").await.unwrap();

        // Test read
        let data = storage.read("test.txt").await.unwrap();
        assert_eq!(data, b"Hello, World!");

        // Test exists
        assert!(storage.exists("test.txt").await.unwrap());
        assert!(!storage.exists("nonexistent.txt").await.unwrap());

        // Test stat
        let meta = storage.stat("test.txt").await.unwrap();
        assert_eq!(meta.size, 13);
        assert!(!meta.is_dir);

        // Test delete
        storage.delete("test.txt").await.unwrap();
        assert!(!storage.exists("test.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_memory_storage_directories() {
        let storage = OpenDalStorage::memory().unwrap();

        // Write file in nested path (auto-creates dirs)
        storage.write("a/b/c/file.txt", b"nested").await.unwrap();

        assert!(storage.exists("a/b/c/file.txt").await.unwrap());

        // Test list
        let entries = storage
            .list_with_options("", ListOptions::new().recursive())
            .await
            .unwrap();
        assert!(!entries.is_empty());
    }

    #[test]
    fn test_s3_builder() {
        // Just test that builder compiles and validates
        let result = S3Builder::new().build();
        assert!(matches!(result, Err(StorageError::ConfigError { .. })));

        let result = S3Builder::new().bucket("test-bucket").build();
        // This will fail without credentials, but should get past bucket validation
        assert!(result.is_ok() || matches!(result, Err(StorageError::ConfigError { .. })));
    }

    #[test]
    fn test_b2_builder() {
        let result = B2Builder::new().build();
        assert!(matches!(result, Err(StorageError::ConfigError { .. })));
    }

    #[test]
    fn test_gcs_builder() {
        let result = GcsBuilder::new().build();
        assert!(matches!(result, Err(StorageError::ConfigError { .. })));
    }

    #[test]
    fn test_webdav_builder() {
        let result = WebDavBuilder::new().build();
        assert!(matches!(result, Err(StorageError::ConfigError { .. })));
    }
}
