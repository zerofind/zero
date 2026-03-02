//! Sync command handler
//!
//! Supports both local-to-local sync (fast path) and cloud sync via StorageBackend.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};

use zero::output::*;
use zero::progress::AtomicProgress;
use zero::storage::{ListOptions, StorageBackend, StorageEntry};
use zero::sync::{SyncJob, SyncOptions, SyncPhase};
use super::cloud::parse_storage_path;

pub struct CmdSyncOptions<'a> {
    pub source: &'a str,
    pub dest: &'a str,
    pub use_checksum: bool,
    pub delete_orphans: bool,
    pub hash_on_copy: bool,
    pub preserve_permissions: bool,
    pub exclude_patterns: &'a [String],
    pub dry_run: bool,
    pub chunked: bool,
    pub chunk_threshold: u64,
}

/// Check if a path string looks like a cloud URL
fn is_cloud_url(path: &str) -> bool {
    // Cloud URLs have a scheme like s3://, b2://, gs://, etc.
    // file:// is treated as local
    if let Some((scheme, _)) = path.split_once("://") {
        !matches!(scheme, "file")
    } else {
        false
    }
}

/// Unified sync command - handles both local and cloud paths
pub fn cmd_sync(out: &Outputter, opts: &CmdSyncOptions<'_>) -> Result<()> {
    let source_is_cloud = is_cloud_url(opts.source);
    let dest_is_cloud = is_cloud_url(opts.dest);

    if source_is_cloud || dest_is_cloud {
        // Cloud sync path - use StorageBackend
        cmd_sync_storage(
            out,
            opts.source,
            opts.dest,
            opts.delete_orphans,
            opts.dry_run,
            opts.exclude_patterns,
            opts.use_checksum,
        )
    } else {
        // Local sync path - use fast SyncJob
        let source_path = PathBuf::from(opts.source);
        let dest_path = PathBuf::from(opts.dest);
        cmd_sync_local(out, &source_path, &dest_path, opts)
    }
}

/// Local-to-local sync using the optimized SyncJob
fn cmd_sync_local(
    out: &Outputter,
    source: &Path,
    dest: &Path,
    opts: &CmdSyncOptions<'_>,
) -> Result<()> {
    let CmdSyncOptions {
        use_checksum,
        delete_orphans,
        hash_on_copy,
        preserve_permissions,
        exclude_patterns,
        dry_run,
        chunked,
        chunk_threshold,
        ..
    } = *opts;
    if dry_run {
        out.header(&format!(
            "[DRY RUN] Sync {} → {}",
            source.display(),
            dest.display()
        ));
        out.warn("Dry run mode: no files will be copied or deleted");
    } else {
        out.header(&format!("Sync {} → {}", source.display(), dest.display()));
    }
    if use_checksum {
        out.info("Using checksums for comparison (thorough)");
    } else {
        out.info("Using mtime+size for comparison (fast)");
    }
    if delete_orphans {
        out.info("Mirror mode: will delete files in destination not in source");
    }
    if hash_on_copy {
        out.info("Hash-on-copy: hashing during transfer + caching checksums");
    }
    if preserve_permissions {
        out.info("Preserve directory permissions: enabled");
    }
    if !exclude_patterns.is_empty() {
        out.info(&format!(
            "Excluding {} pattern(s): {}",
            exclude_patterns.len(),
            exclude_patterns.join(", ")
        ));
    }
    if !chunked {
        out.info("Chunked transfer: disabled (use default for resume capability)");
    }
    out.newline();

    let options = SyncOptions {
        use_checksum,
        delete_orphans,
        verify: hash_on_copy,
        preserve_permissions,
        exclude_patterns: exclude_patterns.to_vec(),
        dry_run,
        chunked,
        chunk_threshold,
        ..Default::default()
    };

    let job = SyncJob::new(source, dest, options)?;

    // Show cache database paths if caching is enabled
    if let Some((src_db, dest_db)) = job.get_cache_info() {
        out.info(&format!("Source cache: {}", src_db));
        out.info(&format!("Dest cache: {}", dest_db));
    }

    let mut last_phase = SyncPhase::Scanning;
    let is_json = out.is_json();

    let result = job.run(|progress| {
        if is_json {
            return; // Skip progress logging in JSON mode
        }
        // Log phase changes
        if progress.phase != last_phase {
            match progress.phase {
                SyncPhase::Scanning => println!("Scanning directories (this may take a moment)..."),
                SyncPhase::Diffing => println!("Comparing files..."),
                SyncPhase::Transferring => {
                    println!();
                    println!(
                        "Copying {} files ({})...",
                        progress.total_files,
                        format_bytes(progress.total_bytes)
                    );
                }
                SyncPhase::Deleting => {
                    println!();
                    println!("Deleting {} orphan files...", progress.files_to_delete);
                }
                SyncPhase::Complete => {}
            }
            last_phase = progress.phase;
        }

        // Log transfer progress
        if progress.phase == SyncPhase::Transferring
            && let Some(ref current) = progress.current_file
                && (progress.files_done % 10 == 0 || progress.files_done == progress.total_files) {
                    println!(
                        "[{}/{}] {:.1}% - {}",
                        progress.files_done,
                        progress.total_files,
                        progress.percent(),
                        current.display()
                    );
                }
    })?;

    let duration_ms = result.duration.as_millis() as u64;
    let throughput = if result.duration.as_secs_f64() > 0.0 && result.bytes_transferred > 0 {
        Some((result.bytes_transferred as f64 / 1_000_000.0) / result.duration.as_secs_f64())
    } else {
        None
    };

    let data = SyncData {
        source: source.to_path_buf(),
        dest: dest.to_path_buf(),
        files_copied: result.files_transferred,
        files_deleted: result.files_deleted,
        files_unchanged: result.diff_summary.identical,
        bytes_copied: result.bytes_transferred,
        bytes_deleted: 0, // TODO: track this
        throughput_mbps: throughput,
        dirs_permissions_synced: if preserve_permissions {
            Some(result.dirs_permissions_synced)
        } else {
            None
        },
    };

    let has_errors = result.errors > 0;
    let response = Response::success("sync", duration_ms, data);

    if out.is_json() {
        out.result(&response);
        if has_errors {
            std::process::exit(1);
        }
    } else {
        let total_files = result.diff_summary.added
            + result.diff_summary.modified
            + result.diff_summary.removed
            + result.diff_summary.identical;

        out.newline();
        if has_errors {
            out.error(&format!("Sync completed with {} errors!", result.errors));
        } else {
            out.success("Sync complete!");
        }
        out.newline();
        out.kv(
            "Source",
            format!(
                "{} files, {}",
                result.source_stats.files,
                format_bytes(result.source_stats.bytes)
            ),
        );
        out.kv(
            "Destination",
            format!(
                "{} files, {}",
                result.dest_stats.files,
                format_bytes(result.dest_stats.bytes)
            ),
        );
        out.newline();
        out.kv(
            "Diff",
            format!(
                "{} added, {} modified, {} removed, {} unchanged (total: {})",
                result.diff_summary.added,
                result.diff_summary.modified,
                result.diff_summary.removed,
                result.diff_summary.identical,
                total_files
            ),
        );
        out.kv("Files transferred", result.files_transferred);
        out.kv("Bytes transferred", format_bytes(result.bytes_transferred));
        if result.files_deleted > 0 {
            out.kv("Files deleted", result.files_deleted);
        }
        if result.dirs_permissions_synced > 0 {
            out.kv("Dir permissions synced", result.dirs_permissions_synced);
        }
        out.kv("Duration", format_duration(result.duration));
        if result.errors > 0 {
            out.kv("Errors", result.errors);
            out.newline();
            out.warn("Some files failed to sync. Run again to retry failed files.");
        }
        if let Some(tp) = throughput {
            out.kv("Throughput", format!("{:.1} MB/s", tp));
        }
    }

    if has_errors {
        anyhow::bail!("Sync completed with {} errors", result.errors);
    }

    Ok(())
}

/// Cloud sync using StorageBackend abstraction
///
/// This handles any combination of local and cloud paths:
/// - local → cloud (backup/upload)
/// - cloud → local (restore/download)
/// - cloud → cloud (migration)
fn cmd_sync_storage(
    out: &Outputter,
    source: &str,
    dest: &str,
    delete_orphans: bool,
    dry_run: bool,
    exclude_patterns: &[String],
    _use_checksum: bool, // TODO: implement checksum comparison for cloud
) -> Result<()> {
    let start = Instant::now();

    if dry_run {
        out.header(&format!("[DRY RUN] Sync {} → {}", source, dest));
        out.warn("Dry run mode: no files will be copied or deleted");
    } else {
        out.header(&format!("Sync {} → {}", source, dest));
    }

    let (src_storage, src_path) =
        parse_storage_path(source).context("Failed to parse source path")?;
    let (dest_storage, dest_path) =
        parse_storage_path(dest).context("Failed to parse destination path")?;

    out.info(&format!(
        "Source: {}://{} path='{}'",
        src_storage.scheme(),
        src_storage.root(),
        src_path
    ));
    out.info(&format!(
        "Destination: {}://{} path='{}'",
        dest_storage.scheme(),
        dest_storage.root(),
        dest_path
    ));
    if delete_orphans {
        out.info("Mirror mode: will delete files in destination not in source");
    }
    if !exclude_patterns.is_empty() {
        out.info(&format!(
            "Excluding {} pattern(s): {}",
            exclude_patterns.len(),
            exclude_patterns.join(", ")
        ));
    }
    out.newline();

    let rt = tokio::runtime::Runtime::new().context("Failed to create async runtime")?;

    let result = rt.block_on(async {
        storage_sync_impl(
            out,
            &*src_storage,
            &*dest_storage,
            &StorageSyncOptions {
                src_path: &src_path,
                dest_path: &dest_path,
                delete_orphans,
                dry_run,
                exclude_patterns,
            },
        )
        .await
    });

    let duration = start.elapsed();

    match result {
        Ok(stats) => {
            out.newline();
            if dry_run {
                out.success("[DRY RUN] Sync analysis complete");
                out.kv("Would copy", format!("{} files", stats.files_to_copy));
                out.kv("Would transfer", format_bytes(stats.bytes_to_copy));
                if delete_orphans {
                    out.kv("Would delete", format!("{} files", stats.files_to_delete));
                }
            } else {
                out.success("Sync complete!");
                out.kv("Files copied", stats.files_copied);
                out.kv("Bytes transferred", format_bytes(stats.bytes_copied));
                if delete_orphans {
                    out.kv("Files deleted", stats.files_deleted);
                }
                out.kv("Files unchanged", stats.files_unchanged);
            }
            out.kv("Duration", format_duration(duration));
            Ok(())
        }
        Err(e) => {
            out.error(&format!("Sync failed: {}", e));
            Err(e)
        }
    }
}

/// Statistics from a storage sync operation
#[derive(Default)]
struct StorageSyncStats {
    files_to_copy: usize,
    bytes_to_copy: u64,
    files_to_delete: usize,
    files_copied: usize,
    bytes_copied: u64,
    files_deleted: usize,
    files_unchanged: usize,
}

/// Check if a file path matches any exclude pattern
fn matches_exclude_pattern(path: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        // Simple glob matching - support * and ?
        if pattern.contains('*') || pattern.contains('?') {
            // Convert glob to simple matching
            let pattern_parts: Vec<&str> = pattern.split('*').collect();
            if pattern_parts.len() == 2 {
                // Pattern like "*.txt" or "prefix*"
                let (prefix, suffix) = (pattern_parts[0], pattern_parts[1]);
                if path.starts_with(prefix) && path.ends_with(suffix) {
                    return true;
                }
                // Also check just the filename
                if let Some(filename) = path.rsplit('/').next()
                    && filename.starts_with(prefix) && filename.ends_with(suffix) {
                        return true;
                    }
            }
        } else {
            // Exact match or path component match
            if path == pattern
                || path.contains(&format!("/{}/", pattern))
                || path.ends_with(&format!("/{}", pattern))
                || path.starts_with(&format!("{}/", pattern))
            {
                return true;
            }
        }
    }
    false
}

struct StorageSyncOptions<'a> {
    pub src_path: &'a str,
    pub dest_path: &'a str,
    pub delete_orphans: bool,
    pub dry_run: bool,
    pub exclude_patterns: &'a [String],
}

/// Internal implementation of storage sync
async fn storage_sync_impl(
    out: &Outputter,
    src: &dyn StorageBackend,
    dest: &dyn StorageBackend,
    opts: &StorageSyncOptions<'_>,
) -> Result<StorageSyncStats> {
    let src_path = opts.src_path;
    let dest_path = opts.dest_path;
    let delete_orphans = opts.delete_orphans;
    let dry_run = opts.dry_run;
    let exclude_patterns = opts.exclude_patterns;
    use std::io::Write;

    let mut stats = StorageSyncStats::default();

    // List source files
    out.info("Scanning source...");
    let src_entries = src
        .list_with_options(src_path, ListOptions::new().recursive().files_only())
        .await
        .map_err(anyhow::Error::msg)?;

    // Filter by exclude patterns
    let src_entries: Vec<StorageEntry> = if exclude_patterns.is_empty() {
        src_entries
    } else {
        src_entries
            .into_iter()
            .filter(|e| !matches_exclude_pattern(&e.path.to_string_lossy(), exclude_patterns))
            .collect()
    };

    out.info(&format!("Found {} files in source", src_entries.len()));

    // List destination files
    out.info("Scanning destination...");
    let dest_entries = dest
        .list_with_options(dest_path, ListOptions::new().recursive().files_only())
        .await
        .unwrap_or_default(); // Destination might not exist yet

    out.info(&format!(
        "Found {} files in destination",
        dest_entries.len()
    ));

    // Build a map of destination files for quick lookup
    let dest_map: std::collections::HashMap<String, &StorageEntry> = dest_entries
        .iter()
        .map(|e| (e.path.to_string_lossy().to_string(), e))
        .collect();

    // Build a set of source paths for orphan detection
    let src_paths: std::collections::HashSet<String> = src_entries
        .iter()
        .map(|e| e.path.to_string_lossy().to_string())
        .collect();

    // Determine which files need to be copied
    let mut files_to_copy = Vec::new();

    for src_entry in &src_entries {
        let rel_path = src_entry.path.to_string_lossy().to_string();

        let needs_copy = if let Some(dest_entry) = dest_map.get(&rel_path) {
            // File exists in destination - check if it needs updating
            // Compare by size (mtime comparison is unreliable across storage backends)
            src_entry.size() != dest_entry.size()
        } else {
            // File doesn't exist in destination
            true
        };

        if needs_copy {
            files_to_copy.push(src_entry);
            stats.files_to_copy += 1;
            stats.bytes_to_copy += src_entry.size();
        } else {
            stats.files_unchanged += 1;
        }
    }

    // Determine orphaned files in destination
    let mut files_to_delete = Vec::new();
    if delete_orphans {
        for dest_entry in &dest_entries {
            let rel_path = dest_entry.path.to_string_lossy().to_string();
            if !src_paths.contains(&rel_path) {
                files_to_delete.push(dest_entry);
                stats.files_to_delete += 1;
            }
        }
    }

    out.newline();
    out.info(&format!(
        "To copy: {} files ({})",
        stats.files_to_copy,
        format_bytes(stats.bytes_to_copy)
    ));
    if delete_orphans {
        out.info(&format!("To delete: {} files", stats.files_to_delete));
    }
    out.info(&format!("Unchanged: {} files", stats.files_unchanged));

    if dry_run {
        // In dry run mode, just show what would be done
        if !files_to_copy.is_empty() {
            out.newline();
            out.info("Files to copy:");
            for entry in files_to_copy.iter().take(20) {
                out.indented(&format!(
                    "{} ({})",
                    entry.path.display(),
                    format_bytes(entry.size())
                ));
            }
            if files_to_copy.len() > 20 {
                out.indented(&format!("... and {} more", files_to_copy.len() - 20));
            }
        }

        if !files_to_delete.is_empty() {
            out.newline();
            out.info("Files to delete:");
            for entry in files_to_delete.iter().take(20) {
                out.indented(&format!("{}", entry.path.display()));
            }
            if files_to_delete.len() > 20 {
                out.indented(&format!("... and {} more", files_to_delete.len() - 20));
            }
        }

        return Ok(stats);
    }

    // Actually perform the sync
    if !files_to_copy.is_empty() {
        out.newline();
        out.info("Copying files...");

        // Set up progress tracking
        let progress = Arc::new(AtomicProgress::new(
            files_to_copy.len(),
            stats.bytes_to_copy,
        ));

        // Start progress display thread
        let show_progress = !out.is_json();
        let progress_handle = if show_progress {
            let progress_clone = Arc::clone(&progress);
            let total_files = files_to_copy.len();
            let total_bytes = stats.bytes_to_copy;
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

        for src_entry in files_to_copy.iter() {
            let rel_path = src_entry.path.to_string_lossy().to_string();

            // Calculate destination path
            let file_dest_path = if dest_path.is_empty() {
                rel_path.clone()
            } else {
                format!("{}/{}", dest_path.trim_end_matches('/'), rel_path)
            };

            // Calculate source path
            let file_src_path = if src_path.is_empty() {
                rel_path.clone()
            } else {
                format!("{}/{}", src_path.trim_end_matches('/'), rel_path)
            };

            // Read and write
            let data = src.read(&file_src_path).await.map_err(anyhow::Error::msg)?;
            dest.write(&file_dest_path, &data)
                .await
                .map_err(anyhow::Error::msg)?;

            stats.files_copied += 1;
            stats.bytes_copied += src_entry.size();

            // Update progress tracker
            progress.add_bytes(src_entry.size());
            progress.file_done();
        }

        // Wait for progress thread to finish
        if let Some(handle) = progress_handle {
            let _ = handle.join();
        }

        if show_progress {
            eprintln!(); // Clear progress line
        }
    }

    // Delete orphaned files
    if delete_orphans && !files_to_delete.is_empty() {
        out.newline();
        out.info("Deleting orphaned files...");

        for dest_entry in files_to_delete {
            let rel_path = dest_entry.path.to_string_lossy().to_string();

            let file_dest_path = if dest_path.is_empty() {
                rel_path.clone()
            } else {
                format!("{}/{}", dest_path.trim_end_matches('/'), rel_path)
            };

            dest.delete(&file_dest_path)
                .await
                .map_err(anyhow::Error::msg)?;
            stats.files_deleted += 1;
        }
    }

    Ok(stats)
}
