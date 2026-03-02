//! Cloud storage command handlers
//!
//! Provides CLI commands for interacting with cloud storage backends:
//! - `cp` - Copy files between local and cloud storage
//! - `ls` - List files in cloud storage
//!
//! Note: `sync` is now unified under the main `zero sync` command,
//! which automatically handles both local and cloud paths.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, bail};

use zero::output::*;
use zero::progress::AtomicProgress;
use zero::storage::{ListOptions, LocalStorage, StorageBackend};

#[cfg(feature = "opendal")]
use zero::storage::OpenDalStorage;

/// Parse a path/URL string into a storage backend and relative path
///
/// Supported formats:
/// - `/path/to/dir` or `./relative` - Local filesystem
/// - `s3://bucket/path` - Amazon S3
/// - `b2://bucket/path` - Backblaze B2
/// - `gs://bucket/path` - Google Cloud Storage
/// - `dropbox://path` - Dropbox
pub fn parse_storage_path(path: &str) -> Result<(Box<dyn StorageBackend + Send + Sync>, String)> {
    // Check if it's a URL with a scheme
    if let Some((scheme, rest)) = path.split_once("://") {
        match scheme {
            "file" => {
                // file:///path/to/dir -> local storage
                // For file:// URLs, the path starts after the third slash
                // file:///tmp/foo -> /tmp/foo
                let local_path = if rest.starts_with('/') {
                    // Absolute path: file:///tmp/foo
                    format!("/{}", rest.trim_start_matches('/'))
                } else {
                    // Relative path: file://./foo or file://foo
                    rest.to_string()
                };

                let path_obj = Path::new(&local_path);
                let (parent, filename) = if path_obj.is_file() {
                    // It's a file - parent is root, filename is the subpath
                    (
                        path_obj.parent().unwrap_or(Path::new("/")).to_path_buf(),
                        path_obj
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default(),
                    )
                } else {
                    // It's a directory - use it as root
                    (path_obj.to_path_buf(), String::new())
                };

                Ok((Box::new(LocalStorage::new(&parent)), filename))
            }
            #[cfg(feature = "opendal")]
            "s3" | "b2" | "gs" | "gcs" | "dropbox" | "webdav" | "dav" => {
                let (storage, subpath) = OpenDalStorage::from_url(path)
                    .map_err(|e| anyhow::anyhow!("Failed to parse cloud URL: {}", e))?;
                Ok((Box::new(storage), subpath))
            }
            #[cfg(not(feature = "opendal"))]
            _ => {
                bail!(
                    "Cloud storage scheme '{}' requires the 'opendal' feature.\n\
                     Recompile with: cargo build --features opendal",
                    scheme
                );
            }
            #[cfg(feature = "opendal")]
            _ => {
                bail!(
                    "Unsupported storage scheme: '{}'\n\
                     Supported: file, s3, b2, gs, gcs, dropbox, webdav",
                    scheme
                );
            }
        }
    } else {
        // No scheme - treat as local path
        let path_obj = Path::new(path);
        let canonical = if path_obj.is_absolute() {
            path_obj.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(path_obj)
        };

        Ok((Box::new(LocalStorage::new(&canonical)), String::new()))
    }
}

/// Copy files between storage backends
///
/// Supports:
/// - Local to local: `zero cloud cp /src/file.txt /dest/file.txt`
/// - Local to cloud: `zero cloud cp /local/file.txt s3://bucket/path/file.txt`
/// - Cloud to local: `zero cloud cp s3://bucket/file.txt /local/file.txt`
/// - Cloud to cloud: `zero cloud cp s3://bucket/file.txt b2://bucket/file.txt`
pub fn cmd_cloud_cp(out: &Outputter, source: &str, dest: &str, recursive: bool) -> Result<()> {
    let start = Instant::now();

    out.header(&format!("Copy {} → {}", source, dest));

    // Parse source and destination
    let (src_storage, src_path) =
        parse_storage_path(source).context("Failed to parse source path")?;
    let (dest_storage, dest_path) =
        parse_storage_path(dest).context("Failed to parse destination path")?;

    out.info(&format!(
        "Source: {} ({})",
        src_storage.scheme(),
        src_storage.root()
    ));
    out.info(&format!(
        "Destination: {} ({})",
        dest_storage.scheme(),
        dest_storage.root()
    ));
    out.newline();

    // Create a runtime for async operations
    let rt = tokio::runtime::Runtime::new().context("Failed to create async runtime")?;

    let result = rt.block_on(async {
        // Check if source exists
        if !src_storage
            .exists(&src_path)
            .await
            .map_err(anyhow::Error::msg)?
        {
            bail!("Source not found: {}", source);
        }

        // Get source metadata
        let src_meta = src_storage
            .stat(&src_path)
            .await
            .map_err(anyhow::Error::msg)?;

        if src_meta.is_dir {
            if !recursive {
                bail!("Source is a directory. Use --recursive (-r) to copy directories.");
            }
            copy_directory(out, &*src_storage, &src_path, &*dest_storage, &dest_path).await
        } else {
            copy_single_file(out, &*src_storage, &src_path, &*dest_storage, &dest_path).await
        }
    });

    let duration = start.elapsed();

    match result {
        Ok((files, bytes)) => {
            out.newline();
            out.success("Copy complete!");
            out.kv("Files copied", files);
            out.kv("Bytes transferred", format_bytes(bytes));
            out.kv("Duration", format_duration(duration));
            Ok(())
        }
        Err(e) => {
            out.error(&format!("Copy failed: {}", e));
            Err(e)
        }
    }
}

/// Copy a single file between storage backends
async fn copy_single_file(
    out: &Outputter,
    src: &dyn StorageBackend,
    src_path: &str,
    dest: &dyn StorageBackend,
    dest_path: &str,
) -> Result<(usize, u64)> {
    let src_meta = src.stat(src_path).await.map_err(anyhow::Error::msg)?;
    let size = src_meta.size;

    out.info(&format!("Copying {} ({})", src_path, format_bytes(size)));

    // Set up progress tracking for large files
    let progress = Arc::new(AtomicProgress::new(1, size));

    // Start progress display for files > 1MB
    let _progress_handle = if size > 1_000_000 && !out.is_json() {
        let progress_clone = Arc::clone(&progress);
        Some(std::thread::spawn(move || {
            let start = Instant::now();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let bytes = progress_clone.bytes_done();
                let total = progress_clone.bytes_total();
                if bytes >= total {
                    break;
                }
                let elapsed = start.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    bytes as f64 / elapsed
                } else {
                    0.0
                };
                eprint!(
                    "\r  Progress: {}/{} ({}/s)    ",
                    format_bytes(bytes),
                    format_bytes(total),
                    format_bytes(speed as u64)
                );
                let _ = std::io::stderr().flush();
            }
            eprintln!();
        }))
    } else {
        None
    };

    // Read source
    let data = src.read(src_path).await.map_err(anyhow::Error::msg)?;

    // Update progress after read
    progress.add_bytes(size);

    // Write to destination
    dest.write(dest_path, &data)
        .await
        .map_err(anyhow::Error::msg)?;

    progress.file_done();

    Ok((1, size))
}

/// Copy a directory recursively between storage backends
async fn copy_directory(
    out: &Outputter,
    src: &dyn StorageBackend,
    src_path: &str,
    dest: &dyn StorageBackend,
    dest_path: &str,
) -> Result<(usize, u64)> {
    // List all files recursively
    let entries = src
        .list_with_options(src_path, ListOptions::new().recursive())
        .await
        .map_err(anyhow::Error::msg)?;

    let files: Vec<_> = entries.iter().filter(|e| !e.is_dir()).collect();
    let total_files = files.len();
    let total_bytes: u64 = files.iter().map(|e| e.size()).sum();

    out.info(&format!(
        "Found {} files ({}) to copy",
        total_files,
        format_bytes(total_bytes)
    ));

    // Set up progress tracking
    let progress = Arc::new(AtomicProgress::new(total_files, total_bytes));

    // Start progress display thread
    let show_progress = !out.is_json() && total_files > 0;
    let progress_handle = if show_progress {
        let progress_clone = Arc::clone(&progress);
        let total_files_copy = total_files;
        let total_bytes_copy = total_bytes;
        Some(std::thread::spawn(move || {
            let start = Instant::now();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(200));
                let files_done = progress_clone.files_done();
                let bytes_done = progress_clone.bytes_done();

                if files_done >= total_files_copy {
                    break;
                }

                let elapsed = start.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    bytes_done as f64 / elapsed
                } else {
                    0.0
                };

                // Calculate ETA
                let eta = if speed > 0.0 {
                    let remaining = total_bytes_copy.saturating_sub(bytes_done);
                    remaining as f64 / speed
                } else {
                    0.0
                };

                eprint!(
                    "\r  {}/{} files | {}/{} | {}/s | ETA: {:.0}s    ",
                    files_done,
                    total_files_copy,
                    format_bytes(bytes_done),
                    format_bytes(total_bytes_copy),
                    format_bytes(speed as u64),
                    eta
                );
                let _ = std::io::stderr().flush();
            }
        }))
    } else {
        None
    };

    let mut copied_files = 0;
    let mut copied_bytes = 0u64;

    for entry in files {
        let rel_path = entry.path.to_string_lossy();

        // Calculate destination path
        let file_dest_path = if dest_path.is_empty() {
            rel_path.to_string()
        } else {
            format!("{}/{}", dest_path.trim_end_matches('/'), rel_path)
        };

        // Read and write
        let data = src.read(&rel_path).await.map_err(anyhow::Error::msg)?;
        dest.write(&file_dest_path, &data)
            .await
            .map_err(anyhow::Error::msg)?;

        copied_files += 1;
        copied_bytes += entry.size();

        // Update progress tracker
        progress.add_bytes(entry.size());
        progress.file_done();
    }

    // Wait for progress thread to finish
    if let Some(handle) = progress_handle {
        let _ = handle.join();
    }

    if show_progress {
        eprintln!(); // Clear progress line
    }

    Ok((copied_files, copied_bytes))
}

/// List files in a storage location
pub fn cmd_cloud_ls(out: &Outputter, path: &str, recursive: bool, long: bool) -> Result<()> {
    let (storage, subpath) = parse_storage_path(path).context("Failed to parse path")?;

    out.header(&format!(
        "Listing {} ({}://{})",
        path,
        storage.scheme(),
        storage.root()
    ));
    out.newline();

    let rt = tokio::runtime::Runtime::new().context("Failed to create async runtime")?;

    let entries = rt.block_on(async {
        let mut options = ListOptions::new();
        if recursive {
            options = options.recursive();
        }

        storage
            .list_with_options(&subpath, options)
            .await
            .map_err(anyhow::Error::msg)
    })?;

    if entries.is_empty() {
        out.info("No files found");
        return Ok(());
    }

    // Sort entries: directories first, then by name
    let mut sorted_entries = entries;
    sorted_entries.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    let mut total_size = 0u64;
    let mut file_count = 0;
    let mut dir_count = 0;

    for entry in &sorted_entries {
        if entry.is_dir() {
            dir_count += 1;
            if long {
                out.println(&format!("d {:>12}  {}/", "-", entry.path.display()));
            } else {
                out.println(&format!("{}/", entry.path.display()));
            }
        } else {
            file_count += 1;
            total_size += entry.size();
            if long {
                out.println(&format!(
                    "- {:>12}  {}",
                    format_bytes(entry.size()),
                    entry.path.display()
                ));
            } else {
                out.println(&format!("{}", entry.path.display()));
            }
        }
    }

    out.newline();
    out.kv("Directories", dir_count);
    out.kv("Files", file_count);
    out.kv("Total size", format_bytes(total_size));

    Ok(())
}

/// Download files from cloud storage to local directory
///
/// Quick way to download without specifying full destination path:
/// - `zero get s3://bucket/file.txt` → downloads to `./file.txt`
/// - `zero get s3://bucket/backup/` → downloads to `./backup/`
/// - `zero get s3://bucket/data/ -o ./dest/` → downloads to `./dest/data/`
pub fn cmd_get(out: &Outputter, url: &str, output: &str, recursive: bool) -> Result<()> {
    let start = Instant::now();

    // Parse the source URL
    let (src_storage, src_path) = parse_storage_path(url).context("Failed to parse source URL")?;

    // Determine the destination path
    // Extract the last component of the source path for the local filename/dirname
    let src_name = src_path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(&src_path);

    let dest_path = if output == "." {
        // Default: use the source filename in current directory
        if src_name.is_empty() {
            // Root of bucket - use bucket name or "download"
            std::path::PathBuf::from("download")
        } else {
            std::path::PathBuf::from(src_name)
        }
    } else {
        let out_path = std::path::Path::new(output);
        if out_path.is_dir() || output.ends_with('/') {
            // Output is a directory - put file/folder inside it
            out_path.join(src_name)
        } else {
            // Output is a specific path
            out_path.to_path_buf()
        }
    };

    out.header(&format!("Download {} → {}", url, dest_path.display()));
    out.info(&format!(
        "Source: {}://{} path='{}'",
        src_storage.scheme(),
        src_storage.root(),
        src_path
    ));

    let rt = tokio::runtime::Runtime::new().context("Failed to create async runtime")?;

    let result = rt.block_on(async {
        // Check if source is a file or directory
        let is_dir = src_storage.is_dir(&src_path).await.unwrap_or(false);

        if is_dir {
            if !recursive {
                anyhow::bail!("Source is a directory. Use -r/--recursive to download directories.");
            }
            // Download directory
            download_directory(out, &*src_storage, &src_path, &dest_path).await
        } else {
            // Download single file
            download_single_file(out, &*src_storage, &src_path, &dest_path).await
        }
    });

    let duration = start.elapsed();

    match result {
        Ok((files, bytes)) => {
            out.newline();
            out.success("Download complete!");
            out.kv("Files downloaded", files);
            out.kv("Bytes transferred", format_bytes(bytes));
            out.kv("Duration", format_duration(duration));
            Ok(())
        }
        Err(e) => {
            out.error(&format!("Download failed: {}", e));
            Err(e)
        }
    }
}

/// Download a single file from cloud storage
async fn download_single_file(
    out: &Outputter,
    src: &dyn StorageBackend,
    src_path: &str,
    dest_path: &std::path::Path,
) -> Result<(usize, u64)> {
    use std::io::Write;

    // Get file size for progress
    let metadata = src.stat(src_path).await.map_err(anyhow::Error::msg)?;
    let file_size = metadata.size;

    out.info(&format!(
        "Downloading {} ({})",
        src_path,
        format_bytes(file_size)
    ));

    // Set up progress tracking
    let progress = Arc::new(AtomicProgress::new(1, file_size));

    // Start progress display thread for large files
    let show_progress = !out.is_json() && file_size > 1024 * 1024; // Show progress for files > 1MB
    let progress_handle = if show_progress {
        let progress_clone = Arc::clone(&progress);
        Some(std::thread::spawn(move || {
            let start = Instant::now();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let bytes_done = progress_clone.bytes_done();
                let files_done = progress_clone.files_done();

                if files_done >= 1 {
                    break;
                }

                let elapsed = start.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    bytes_done as f64 / elapsed
                } else {
                    0.0
                };

                eprint!(
                    "\r  {}/{} | {}/s    ",
                    format_bytes(bytes_done),
                    format_bytes(file_size),
                    format_bytes(speed as u64),
                );
                let _ = std::io::stderr().flush();
            }
        }))
    } else {
        None
    };

    // Read from cloud
    let data = src.read(src_path).await.map_err(anyhow::Error::msg)?;

    // Create parent directories if needed
    if let Some(parent) = dest_path.parent()
        && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }

    // Write to local file
    std::fs::write(dest_path, &data)?;

    // Update progress
    progress.add_bytes(file_size);
    progress.file_done();

    // Wait for progress thread
    if let Some(handle) = progress_handle {
        let _ = handle.join();
    }

    if show_progress {
        eprintln!(); // Clear progress line
    }

    Ok((1, file_size))
}

/// Download a directory recursively from cloud storage
async fn download_directory(
    out: &Outputter,
    src: &dyn StorageBackend,
    src_path: &str,
    dest_path: &std::path::Path,
) -> Result<(usize, u64)> {
    use std::io::Write;

    // List all files in the source directory
    out.info("Scanning source directory...");
    let entries = src
        .list_with_options(src_path, ListOptions::new().recursive().files_only())
        .await
        .map_err(anyhow::Error::msg)?;

    if entries.is_empty() {
        out.warn("No files found in source directory");
        return Ok((0, 0));
    }

    let total_files = entries.len();
    let total_bytes: u64 = entries.iter().map(|e| e.size()).sum();

    out.info(&format!(
        "Found {} files ({})",
        total_files,
        format_bytes(total_bytes)
    ));

    // Create destination directory
    std::fs::create_dir_all(dest_path)?;

    // Set up progress tracking
    let progress = Arc::new(AtomicProgress::new(total_files, total_bytes));

    // Start progress display thread
    let show_progress = !out.is_json();
    let progress_handle = if show_progress {
        let progress_clone = Arc::clone(&progress);
        Some(std::thread::spawn(move || {
            let start = Instant::now();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(200));
                let files_done = progress_clone.files_done();
                let bytes_done = progress_clone.bytes_done();

                if files_done >= total_files {
                    break;
                }

                let elapsed = start.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    bytes_done as f64 / elapsed
                } else {
                    0.0
                };

                let eta = if speed > 0.0 {
                    let remaining = total_bytes.saturating_sub(bytes_done);
                    remaining as f64 / speed
                } else {
                    0.0
                };

                eprint!(
                    "\r  {}/{} files | {}/{} | {}/s | ETA: {:.0}s    ",
                    files_done,
                    total_files,
                    format_bytes(bytes_done),
                    format_bytes(total_bytes),
                    format_bytes(speed as u64),
                    eta
                );
                let _ = std::io::stderr().flush();
            }
        }))
    } else {
        None
    };

    let mut files_downloaded = 0;
    let mut bytes_downloaded = 0u64;

    // Download each file
    for entry in &entries {
        let rel_path = entry.path.to_string_lossy();

        // Build source path
        let file_src_path = if src_path.is_empty() {
            rel_path.to_string()
        } else {
            format!("{}/{}", src_path.trim_end_matches('/'), rel_path)
        };

        // Build destination path
        let file_dest_path = dest_path.join(rel_path.as_ref());

        // Create parent directories
        if let Some(parent) = file_dest_path.parent()
            && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }

        // Download file
        let data = src.read(&file_src_path).await.map_err(anyhow::Error::msg)?;
        std::fs::write(&file_dest_path, &data)?;

        let file_size = entry.size();
        files_downloaded += 1;
        bytes_downloaded += file_size;

        progress.add_bytes(file_size);
        progress.file_done();
    }

    // Wait for progress thread
    if let Some(handle) = progress_handle {
        let _ = handle.join();
    }

    if show_progress {
        eprintln!(); // Clear progress line
    }

    Ok((files_downloaded, bytes_downloaded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_local_path() {
        let (storage, path) = parse_storage_path("/tmp/test").unwrap();
        assert_eq!(storage.scheme(), "file");
        assert!(path.is_empty() || path == "/tmp/test");
    }

    #[test]
    fn test_parse_file_url() {
        let (storage, _path) = parse_storage_path("file:///tmp/test").unwrap();
        assert_eq!(storage.scheme(), "file");
    }

    #[cfg(not(feature = "opendal"))]
    #[test]
    fn test_cloud_url_without_feature() {
        let result = parse_storage_path("s3://bucket/path");
        match result {
            Ok(_) => panic!("Expected error for cloud URL without opendal feature"),
            Err(e) => assert!(e.to_string().contains("opendal")),
        }
    }

    #[cfg(feature = "opendal")]
    #[test]
    fn test_parse_s3_url() {
        // This will fail without credentials, but should parse the URL correctly
        let result = parse_storage_path("s3://mybucket/path");
        // Either succeeds or fails with config error (not parse error)
        match result {
            Ok((storage, path)) => {
                assert_eq!(storage.scheme(), "s3");
                assert_eq!(path, "path");
            }
            Err(e) => {
                // Config error is expected without AWS credentials
                assert!(
                    e.to_string().contains("config")
                        || e.to_string().contains("credential")
                        || e.to_string().contains("Config")
                );
            }
        }
    }
}
