//! Index command handler

use rayon::prelude::*;
use std::panic;
use std::path::Path;
use std::time::Instant;
use zero::cache::{CacheEntry, ChecksumBatch, ControlDb, DeviceInfo, MountType};
use zero::disk::DiskInfo;
use zero::hasher::HashAlgorithm;
use zero::output::*;
use zero::scanner::{ScanOptions, scan_collect};

pub struct IndexOptions<'a> {
    pub path: &'a Path,
    pub algorithm: HashAlgorithm,
    pub max_depth: Option<usize>,
    pub stats: bool,
    pub list: bool,
    pub no_store: bool,
    pub prune: bool,
    pub benchmark: Option<usize>,
}

pub fn cmd_index(out: &Outputter, opts: &IndexOptions<'_>) -> anyhow::Result<()> {
    let IndexOptions {
        path,
        algorithm,
        max_depth,
        stats,
        list,
        no_store,
        prune,
        benchmark,
    } = *opts;
    let start_time = Instant::now();

    // --no-store mode: just display hashes without storing (like old `hash` command)
    if no_store {
        out.header(&format!(
            "Hashing files in {} with {} (no-store mode)",
            path.display(),
            algorithm
        ));

        let options = ScanOptions {
            max_depth: max_depth.unwrap_or(usize::MAX),
            ..Default::default()
        };

        let entries = scan_collect(path, options)?;

        out.info(&format!("Found {} files", entries.len()));
        out.newline();

        let hash_start = Instant::now();

        // Parallel hashing using rayon
        let parallel_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            entries
                .par_iter()
                .map(|entry| {
                    thread_local! {
                        static BUFFER: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(vec![0u8; 128 * 1024]);
                    }

                    BUFFER.with(|buf| {
                        let mut buffer = buf.borrow_mut();
                        match zero::hasher::hash_file_with_buffer(&entry.absolute_path, algorithm, &mut buffer) {
                            Ok(result) => Ok((entry.path.clone(), result.to_hex(), result.bytes_hashed)),
                            Err(e) => Err((entry.path.clone(), e)),
                        }
                    })
                })
                .collect::<Vec<_>>()
        }));

        let results: Vec<_> = match parallel_result {
            Ok(results) => results,
            Err(panic_payload) => {
                eprintln!("\n⚠ Fatal error during parallel hashing");
                panic::resume_unwind(panic_payload);
            }
        };

        let mut total_bytes: u64 = 0;
        let mut hashed_count = 0;
        let mut error_count = 0;

        for result in results {
            match result {
                Ok((file_path, hash, bytes)) => {
                    out.indented(&format!("{}  {}", hash, file_path.display()));
                    total_bytes += bytes;
                    hashed_count += 1;
                }
                Err((file_path, e)) => {
                    error_count += 1;
                    if !out.is_json() {
                        eprintln!("Error hashing {}: {}", file_path.display(), e);
                    }
                }
            }
        }

        let hash_elapsed = hash_start.elapsed();

        out.newline();
        out.success(&format!(
            "Hashed {} files ({})",
            hashed_count,
            format_bytes(total_bytes)
        ));
        if error_count > 0 {
            out.kv("Errors", error_count);
        }
        out.kv("Throughput", format_throughput(total_bytes, hash_elapsed));
        out.kv("Duration", format_duration(start_time.elapsed()));

        return Ok(());
    }

    // All other modes need the database
    out.header(&format!("Index {}", path.display()));

    let control_db =
        ControlDb::open().map_err(|e| anyhow::anyhow!("Failed to open control database: {}", e))?;
    out.info(&format!("Database: {}", control_db.db_path().display()));

    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    let device_info = match DiskInfo::for_path(&canonical_path) {
        Ok(info) => {
            let mount_type = if info.usb.is_some() {
                Some(MountType::Usb)
            } else if info.volume.mount_point == "/" {
                Some(MountType::Internal)
            } else {
                Some(MountType::Folder)
            };
            Some(DeviceInfo {
                mount_type,
                serial: info.usb.as_ref().map(|u| u.serial_number.clone()),
                display_name: Some(info.volume.name.clone()),
                ..Default::default()
            })
        }
        Err(_) => None,
    };

    let storage = control_db
        .get_or_create_storage(&canonical_path, device_info)
        .map_err(|e| anyhow::anyhow!("Failed to get storage: {}", e))?;

    let storage_db = control_db
        .open_storage_db(&storage)
        .map_err(|e| anyhow::anyhow!("Failed to open storage database: {}", e))?;

    out.info(&format!("Storage ID: {}", storage.id));
    out.newline();

    // Handle --stats
    if stats {
        let cache_stats = storage_db
            .get_stats()
            .map_err(|e| anyhow::anyhow!("Failed to get stats: {}", e))?;

        out.info("Index Statistics:");
        out.kv(
            "Files indexed",
            format_count(cache_stats.file_count as usize),
        );
        out.kv("Total bytes", format_bytes(cache_stats.total_bytes as u64));
        out.kv(
            "Files with hash",
            format_count(cache_stats.hashed_count as usize),
        );
        let coverage = if cache_stats.file_count == 0 {
            0.0
        } else {
            (cache_stats.hashed_count as f64 / cache_stats.file_count as f64) * 100.0
        };
        out.kv("Hash coverage", format!("{:.1}%", coverage));

        if let Some(last_seen) = storage.last_seen_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let age_secs = now - last_seen;
            let age_str = if age_secs < 60 {
                format!("{} seconds ago", age_secs)
            } else if age_secs < 3600 {
                format!("{} minutes ago", age_secs / 60)
            } else if age_secs < 86400 {
                format!("{} hours ago", age_secs / 3600)
            } else {
                format!("{} days ago", age_secs / 86400)
            };
            out.kv("Last seen", age_str);
        }

        out.newline();
        out.kv("Duration", format_duration(start_time.elapsed()));
        return Ok(());
    }

    // Handle --list
    if list {
        out.info("Indexed files:");
        out.newline();

        let mut offset = 0i64;
        let limit = 100i64;
        let mut total_listed = 0;

        loop {
            let files = storage_db
                .list_files(limit, offset)
                .map_err(|e| anyhow::anyhow!("Failed to list files: {}", e))?;

            if files.is_empty() {
                break;
            }

            let count = files.len();
            for file in &files {
                let hash_hex = file.hash_xxh3_hex().unwrap_or_default();
                let hash_short = if hash_hex.len() > 16 {
                    format!("{}...", &hash_hex[..16])
                } else {
                    hash_hex
                };
                out.indented(&format!(
                    "{} ({}) [{}]",
                    file.path,
                    format_bytes(file.size as u64),
                    hash_short
                ));
                total_listed += 1;
            }
            if count < limit as usize {
                break;
            }
            offset += limit;
        }

        out.newline();
        out.kv("Total files listed", format_count(total_listed));
        out.kv("Duration", format_duration(start_time.elapsed()));
        return Ok(());
    }

    // Handle --prune
    if prune {
        out.info("Pruning stale index entries...");
        let removed = storage_db
            .prune_missing(&canonical_path)
            .map_err(|e| anyhow::anyhow!("Failed to prune: {}", e))?;

        if removed.is_empty() {
            out.success("No stale entries found");
        } else {
            out.success(&format!("Removed {} stale entries", removed.len()));
            for file_path in removed.iter().take(10) {
                out.indented(&format!("- {}", file_path));
            }
            if removed.len() > 10 {
                out.indented(&format!("... and {} more", removed.len() - 10));
            }
        }

        out.newline();
        out.kv("Duration", format_duration(start_time.elapsed()));
        return Ok(());
    }

    // Handle --benchmark
    if let Some(n) = benchmark {
        out.info(&format!("Benchmarking index with {} files...", n));
        out.newline();

        let scan_options = ScanOptions {
            max_depth: max_depth.unwrap_or(usize::MAX),
            ..Default::default()
        };
        let files = scan_collect(path, scan_options)?;

        if files.is_empty() {
            out.warn("No files found to benchmark");
            return Ok(());
        }

        let files_to_hash: Vec<_> = files.into_iter().take(n).collect();
        let total_bytes: u64 = files_to_hash.iter().map(|f| f.size).sum();

        out.info(&format!(
            "Hashing {} files ({}) with {}...",
            files_to_hash.len(),
            format_bytes(total_bytes),
            algorithm
        ));

        let hash_start = Instant::now();
        let mut batch = ChecksumBatch::new();
        let mut hashed_count = 0u64;
        let mut hashed_bytes = 0u64;
        let mut hash_buffer = vec![0u8; 128 * 1024];

        for file in &files_to_hash {
            let full_path = path.join(&file.path);

            match zero::hasher::hash_file_with_buffer(&full_path, algorithm, &mut hash_buffer) {
                Ok(result) => {
                    let rel_path = file.path.to_string_lossy().to_string();
                    let mtime = file.mtime as i64;

                    let entry = CacheEntry::with_xxh3(
                        rel_path,
                        file.size as i64,
                        mtime,
                        result.hash.clone(),
                    );
                    batch.add(entry);
                    hashed_count += 1;
                    hashed_bytes += file.size;

                    if batch.should_flush() {
                        storage_db
                            .flush_batch(&mut batch)
                            .map_err(|e| anyhow::anyhow!("Failed to flush batch: {}", e))?;
                    }
                }
                Err(e) => {
                    if !out.is_json() {
                        out.warn(&format!("Failed to hash {}: {}", file.path.display(), e));
                    }
                }
            }
        }

        storage_db
            .flush_batch(&mut batch)
            .map_err(|e| anyhow::anyhow!("Failed to flush batch: {}", e))?;

        let hash_duration = hash_start.elapsed();

        let file_count = storage_db.count_files().unwrap_or(0);
        let total_bytes_cached = storage_db.total_bytes().unwrap_or(0);
        control_db
            .update_storage_stats(storage.id, file_count, total_bytes_cached)
            .ok();

        out.newline();
        out.success("Benchmark complete!");
        out.kv("Files hashed", format_count(hashed_count as usize));
        out.kv("Bytes hashed", format_bytes(hashed_bytes));
        out.kv("Hash time", format_duration(hash_duration));
        out.kv("Throughput", format_throughput(hashed_bytes, hash_duration));
        out.kv(
            "Files/sec",
            format!("{:.0}", hashed_count as f64 / hash_duration.as_secs_f64()),
        );

        out.newline();
        out.info("Testing index lookup performance...");

        let lookup_start = Instant::now();
        let mut cache_hits = 0u64;
        let mut cache_misses = 0u64;

        for file in &files_to_hash {
            let rel_path = file.path.to_string_lossy().to_string();
            let mtime = file.mtime as i64;

            match storage_db.lookup_valid_file(&rel_path, file.size as i64, mtime) {
                Ok(Some(_)) => cache_hits += 1,
                Ok(None) => cache_misses += 1,
                Err(_) => cache_misses += 1,
            }
        }

        let lookup_duration = lookup_start.elapsed();

        out.kv("Index hits", format_count(cache_hits as usize));
        out.kv("Index misses", format_count(cache_misses as usize));
        out.kv("Lookup time", format_duration(lookup_duration));
        out.kv(
            "Lookups/sec",
            format!(
                "{:.0}",
                (cache_hits + cache_misses) as f64 / lookup_duration.as_secs_f64()
            ),
        );

        out.newline();
        out.kv("Total duration", format_duration(start_time.elapsed()));
        return Ok(());
    }

    // Default: build/update index for the directory
    out.info(&format!("Indexing files with {}...", algorithm));

    let scan_options = ScanOptions {
        max_depth: max_depth.unwrap_or(usize::MAX),
        ..Default::default()
    };
    let files = scan_collect(path, scan_options)?;

    if files.is_empty() {
        out.warn("No files found");
        return Ok(());
    }

    out.info(&format!("Found {} files", files.len()));

    let hash_start = Instant::now();
    let mut batch = ChecksumBatch::new();
    let mut hashed_count = 0u64;
    let mut hashed_bytes = 0u64;
    let mut errors = 0u64;
    let mut hash_buffer = vec![0u8; 128 * 1024];

    let total_files = files.len();

    for (i, file) in files.iter().enumerate() {
        let full_path = path.join(&file.path);

        if !out.is_json() && i > 0 && i % 100 == 0 {
            let pct = (i as f64 / total_files as f64) * 100.0;
            print!(
                "\r[{}/{}] {:.1}% - {}    ",
                i,
                total_files,
                pct,
                file.path.display()
            );
            use std::io::Write;
            std::io::stdout().flush().ok();
        }

        match zero::hasher::hash_file_with_buffer(&full_path, algorithm, &mut hash_buffer) {
            Ok(result) => {
                let rel_path = file.path.to_string_lossy().to_string();
                let mtime = file.mtime as i64;

                let entry =
                    CacheEntry::with_xxh3(rel_path, file.size as i64, mtime, result.hash.clone());
                batch.add(entry);
                hashed_count += 1;
                hashed_bytes += file.size;

                if batch.should_flush() {
                    storage_db
                        .flush_batch(&mut batch)
                        .map_err(|e| anyhow::anyhow!("Failed to flush batch: {}", e))?;
                }
            }
            Err(_) => {
                errors += 1;
            }
        }
    }

    if !out.is_json() {
        print!("\r{}\r", " ".repeat(80));
        use std::io::Write;
        std::io::stdout().flush().ok();
    }

    storage_db
        .flush_batch(&mut batch)
        .map_err(|e| anyhow::anyhow!("Failed to flush batch: {}", e))?;

    let hash_duration = hash_start.elapsed();

    let file_count = storage_db.count_files().unwrap_or(0);
    let total_bytes_cached = storage_db.total_bytes().unwrap_or(0);
    control_db
        .update_storage_stats(storage.id, file_count, total_bytes_cached)
        .ok();

    out.newline();
    out.success("Index built!");
    out.kv("Files indexed", format_count(hashed_count as usize));
    out.kv("Bytes hashed", format_bytes(hashed_bytes));
    if errors > 0 {
        out.kv("Errors", format_count(errors as usize));
    }
    out.kv("Throughput", format_throughput(hashed_bytes, hash_duration));
    out.newline();
    out.kv("Duration", format_duration(start_time.elapsed()));

    Ok(())
}
