//! Verify command handlers

use anyhow::Result;
use std::panic;
use std::path::Path;
use std::time::{Duration, Instant};
use zero::cache::{CacheEntry, ControlDb, StorageDb};
use zero::differ::{DiffOptions, DiffStatus, diff_with_options, diff_with_progress};
use zero::output::*;
use zero::scanner::{ScanOptions, scan_collect};
use zero::transfer::compare_permissions;

pub fn cmd_verify_with_cache(
    out: &Outputter,
    source: &Path,
    dest: &Path,
    max_depth: Option<usize>,
    check_permissions: bool,
) -> Result<anyhow::Result<()>, ()> {
    let start_time = Instant::now();

    // Try to open cache database
    let control_db = match ControlDb::open() {
        Ok(db) => db,
        Err(_) => return Err(()),
    };

    // Canonicalize paths
    let canonical_source = source
        .canonicalize()
        .unwrap_or_else(|_| source.to_path_buf());
    let canonical_dest = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());

    // Get storages and their databases
    let source_storage = match control_db.get_storage_by_path(&canonical_source) {
        Ok(Some(s)) => s,
        _ => return Err(()),
    };
    let dest_storage = match control_db.get_storage_by_path(&canonical_dest) {
        Ok(Some(s)) => s,
        _ => return Err(()),
    };

    let (source_db, dest_db) = match (|| {
        let source_db = control_db.open_storage_db(&source_storage)?;
        let dest_db = control_db.open_storage_db(&dest_storage)?;
        Ok::<_, zero::cache::CacheError>((source_db, dest_db))
    })() {
        Ok(result) => result,
        Err(_) => return Err(()), // No cached storages, fall back to regular verify
    };

    out.info(&format!(
        "Using cache (source ID: {}, dest ID: {})",
        source_storage.id, dest_storage.id
    ));

    // Scan both directories
    out.info("Scanning directories...");

    let scan_options = ScanOptions {
        max_depth: max_depth.unwrap_or(usize::MAX),
        ..Default::default()
    };

    let scan_opts_source = scan_options.clone();
    let scan_opts_dest = scan_options;

    // Wrap in catch_unwind for panic safety (early exit strategy)
    let scan_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        rayon::join(
            || scan_collect(source, scan_opts_source),
            || scan_collect(dest, scan_opts_dest),
        )
    }));

    // If a panic occurred during parallel scanning, propagate it
    // The global panic handler will ensure clean process exit
    let (source_result, dest_result) = match scan_result {
        Ok(results) => results,
        Err(panic_payload) => {
            eprintln!("\n⚠ Fatal error during parallel directory scan");
            panic::resume_unwind(panic_payload);
        }
    };

    let source_files = match source_result {
        Ok(f) => f,
        Err(e) => return Ok(Err(anyhow::anyhow!("Failed to scan source: {}", e))),
    };
    let dest_files = match dest_result {
        Ok(f) => f,
        Err(e) => return Ok(Err(anyhow::anyhow!("Failed to scan dest: {}", e))),
    };

    out.indented(&format!("Source: {} files", source_files.len()));
    out.indented(&format!("Dest:   {} files", dest_files.len()));
    out.newline();

    // Build lookup maps
    let source_map: std::collections::HashMap<_, _> = source_files
        .iter()
        .map(|f| (f.path.to_string_lossy().to_string(), f))
        .collect();
    let dest_map: std::collections::HashMap<_, _> = dest_files
        .iter()
        .map(|f| (f.path.to_string_lossy().to_string(), f))
        .collect();

    out.info("Comparing files with cache...");
    out.newline();

    let mut cache_hits = 0u64;
    let mut cache_misses = 0u64;
    let mut bytes_hashed = 0u64;
    let mut identical = 0usize;
    let mut different = 0usize;
    let mut missing_in_dest: Vec<String> = Vec::new();
    let mut extra_in_dest: Vec<String> = Vec::new();
    let mut content_differs: Vec<String> = Vec::new();

    let mut hash_buffer = vec![0u8; 128 * 1024];
    let hash_start = Instant::now();

    // Check each source file
    for (rel_path, source_file) in &source_map {
        if let Some(dest_file) = dest_map.get(rel_path) {
            // File exists in both - compare hashes
            let source_hash = get_file_hash_cached(
                &source_db,
                rel_path,
                source_file,
                &canonical_source,
                &mut hash_buffer,
            );

            let dest_hash = get_file_hash_cached(
                &dest_db,
                rel_path,
                dest_file,
                &canonical_dest,
                &mut hash_buffer,
            );

            match (source_hash, dest_hash) {
                (Ok((sh, s_cached)), Ok((dh, d_cached))) => {
                    if s_cached {
                        cache_hits += 1;
                    } else {
                        cache_misses += 1;
                        bytes_hashed += source_file.size;
                    }
                    if d_cached {
                        cache_hits += 1;
                    } else {
                        cache_misses += 1;
                        bytes_hashed += dest_file.size;
                    }

                    if sh == dh {
                        identical += 1;
                    } else {
                        different += 1;
                        content_differs.push(rel_path.clone());
                    }
                }
                _ => {
                    // Hash failed, count as different
                    different += 1;
                    content_differs.push(rel_path.clone());
                }
            }
        } else {
            missing_in_dest.push(rel_path.clone());
        }

        // Progress update
        let total_checked = identical + different + missing_in_dest.len();
        if !out.is_json() && total_checked.is_multiple_of(100) {
            let elapsed = hash_start.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                bytes_hashed as f64 / elapsed / 1_000_000.0
            } else {
                0.0
            };
            print!(
                "\r[{}/{}] {} cache hits, {} hashed ({:.1} MB/s)    ",
                total_checked,
                source_files.len(),
                cache_hits,
                cache_misses,
                speed
            );
            use std::io::Write;
            std::io::stdout().flush().ok();
        }
    }

    // Find extra files in dest
    for rel_path in dest_map.keys() {
        if !source_map.contains_key(rel_path) {
            extra_in_dest.push(rel_path.clone());
        }
    }

    // Clear progress line
    if !out.is_json() {
        print!("\r{}\r", " ".repeat(80));
        use std::io::Write;
        std::io::stdout().flush().ok();
    }

    // Check permissions if requested
    let permission_mismatches = if check_permissions {
        match compare_permissions(source, dest) {
            Ok(result) => result.mismatches,
            Err(e) => {
                out.warn(&format!("Failed to compare permissions: {}", e));
                vec![]
            }
        }
    } else {
        vec![]
    };

    let duration = start_time.elapsed();
    let is_fully_verified = missing_in_dest.is_empty()
        && extra_in_dest.is_empty()
        && content_differs.is_empty()
        && permission_mismatches.is_empty();

    out.newline();
    out.info("Cache Statistics:");
    out.kv("Cache hits", format_count(cache_hits as usize));
    out.kv("Cache misses (hashed)", format_count(cache_misses as usize));
    out.kv(
        "Cache hit rate",
        format!(
            "{:.1}%",
            if cache_hits + cache_misses > 0 {
                cache_hits as f64 / (cache_hits + cache_misses) as f64 * 100.0
            } else {
                0.0
            }
        ),
    );
    if cache_misses > 0 {
        out.kv("Bytes hashed", format_bytes(bytes_hashed));
    }
    out.newline();

    if is_fully_verified {
        out.success(&format!("Verified: {} files match", identical));
        if check_permissions {
            out.success("All permissions match");
        }
        out.newline();
        out.kv("Duration", format_duration(duration));
        return Ok(Ok(()));
    }

    // Show differences
    out.error("Verification failed - directories differ:");
    out.newline();

    if !permission_mismatches.is_empty() {
        out.info(&format!(
            "Permission mismatches ({}):",
            permission_mismatches.len()
        ));
        for pm in permission_mismatches.iter().take(10) {
            let kind = if pm.is_dir { "dir" } else { "file" };
            out.indented(&format!(
                "{} [{}]: {:o} → {:o}",
                pm.path, kind, pm.source_mode, pm.dest_mode
            ));
        }
        if permission_mismatches.len() > 10 {
            out.indented(&format!(
                "... and {} more",
                permission_mismatches.len() - 10
            ));
        }
        out.newline();
    }

    if !missing_in_dest.is_empty() {
        out.info(&format!(
            "Missing in destination ({}):",
            missing_in_dest.len()
        ));
        for path in missing_in_dest.iter().take(10) {
            out.indented(&format!("+ {}", path));
        }
        if missing_in_dest.len() > 10 {
            out.indented(&format!("... and {} more", missing_in_dest.len() - 10));
        }
        out.newline();
    }

    if !extra_in_dest.is_empty() {
        out.info(&format!("Extra in destination ({}):", extra_in_dest.len()));
        for path in extra_in_dest.iter().take(10) {
            out.indented(&format!("- {}", path));
        }
        if extra_in_dest.len() > 10 {
            out.indented(&format!("... and {} more", extra_in_dest.len() - 10));
        }
        out.newline();
    }

    if !content_differs.is_empty() {
        out.info(&format!("Content differs ({}):", content_differs.len()));
        for path in content_differs.iter().take(10) {
            out.indented(&format!("~ {}", path));
        }
        if content_differs.len() > 10 {
            out.indented(&format!("... and {} more", content_differs.len() - 10));
        }
        out.newline();
    }

    out.info("Summary:");
    out.kv("Identical", identical);
    out.kv("Missing", missing_in_dest.len());
    out.kv("Extra", extra_in_dest.len());
    out.kv("Different", content_differs.len());
    out.newline();
    out.kv("Duration", format_duration(duration));

    Ok(Err(anyhow::anyhow!(
        "Verification failed: {} differences found",
        missing_in_dest.len() + extra_in_dest.len() + content_differs.len()
    )))
}

/// Get file hash from cache or compute it
pub fn get_file_hash_cached(
    db: &StorageDb,
    rel_path: &str,
    file: &zero::scanner::FileEntry,
    storage_path: &std::path::Path,
    hash_buffer: &mut [u8],
) -> Result<(String, bool), anyhow::Error> {
    let mtime = file.mtime as i64;

    // Try cache first
    if let Ok(Some(cached)) = db
        .lookup_valid_file(rel_path, file.size as i64, mtime)
        && let Some(hash) = cached.hash_xxh3_hex() {
            return Ok((hash, true)); // Cache hit
        }

    // Cache miss - compute hash
    let full_path = storage_path.join(rel_path);
    let result = zero::hasher::hash_file_with_buffer(
        &full_path,
        zero::hasher::HashAlgorithm::Xxh3,
        hash_buffer,
    )?;
    let hash_hex = result.to_hex();

    // Store in cache
    let entry = CacheEntry::with_xxh3(
        rel_path.to_string(),
        file.size as i64,
        mtime,
        result.hash.clone(),
    );
    let _ = db.upsert_file(&entry);

    Ok((hash_hex, false)) // Cache miss
}

pub fn cmd_verify(
    out: &Outputter,
    source: &Path,
    dest: &Path,
    max_depth: Option<usize>,
    quick: bool,
    full: bool,
    check_permissions: bool,
) -> anyhow::Result<()> {
    let start_time = Instant::now();

    out.header(&format!("Verify {} ↔ {}", source.display(), dest.display()));
    if quick {
        out.info("Quick mode: comparing metadata only (size + mtime)");
    } else if full {
        out.info("Full mode: rehashing all files (ignoring cache)");
    } else {
        out.info("Using checksums for comparison (cache-enabled)");
    }
    if check_permissions {
        out.info("Permission checking: enabled");
    }
    out.newline();

    // Try to use cache for faster verification (unless --full or --quick)
    if !quick && !full {
        if let Ok(result) = cmd_verify_with_cache(out, source, dest, max_depth, check_permissions) {
            return result;
        }
        // Fall through to regular verify if cache fails
        out.info("Cache not available, falling back to full verification...");
        out.newline();
    }

    // Scan both directories
    out.info("Scanning directories...");

    let scan_options = ScanOptions {
        max_depth: max_depth.unwrap_or(usize::MAX),
        ..Default::default()
    };

    // Parallel scan
    let source_path = source;
    let dest_path = dest;
    let scan_opts_source = scan_options.clone();
    let scan_opts_dest = scan_options;

    // Wrap in catch_unwind for panic safety (early exit strategy)
    let scan_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        rayon::join(
            || scan_collect(source_path, scan_opts_source),
            || scan_collect(dest_path, scan_opts_dest),
        )
    }));

    // If a panic occurred during parallel scanning, propagate it
    // The global panic handler will ensure clean process exit
    let (source_result, dest_result) = match scan_result {
        Ok(results) => results,
        Err(panic_payload) => {
            eprintln!("\n⚠ Fatal error during parallel directory scan");
            panic::resume_unwind(panic_payload);
        }
    };

    let source_files = source_result?;
    let dest_files = dest_result?;

    out.indented(&format!("Source: {} files", source_files.len()));
    out.indented(&format!("Dest:   {} files", dest_files.len()));
    out.newline();

    // Diff
    let diff_options = DiffOptions {
        use_checksum: !quick,
        include_identical: false,
        ..Default::default()
    };

    // Calculate total bytes for progress
    let total_bytes: u64 = source_files.iter().map(|f| f.size).sum::<u64>()
        + dest_files.iter().map(|f| f.size).sum::<u64>();

    let diff_result = if quick {
        out.info("Comparing metadata...");
        diff_with_options(&source_files, &dest_files, &diff_options)
    } else {
        out.info("Computing checksums and comparing...");
        out.newline();

        let last_update = std::sync::Mutex::new(Instant::now());
        let hash_start = Instant::now();

        diff_with_progress(&source_files, &dest_files, &diff_options, |progress| {
            // Only update every 100ms to avoid flickering
            let mut last = last_update.lock().unwrap();
            if last.elapsed() < Duration::from_millis(100) {
                return;
            }
            *last = Instant::now();

            if !out.is_json() {
                let pct = if progress.bytes_total > 0 {
                    progress.bytes_processed as f64 / progress.bytes_total as f64 * 100.0
                } else {
                    0.0
                };

                let elapsed = hash_start.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    progress.bytes_processed as f64 / elapsed / 1_000_000.0
                } else {
                    0.0
                };

                let eta = if speed > 0.0 && progress.bytes_total > progress.bytes_processed {
                    let remaining = progress.bytes_total - progress.bytes_processed;
                    let secs = remaining as f64 / (speed * 1_000_000.0);
                    if secs > 3600.0 {
                        format!("{:.1}h left", secs / 3600.0)
                    } else if secs > 60.0 {
                        format!("{:.0}m left", secs / 60.0)
                    } else {
                        format!("{:.0}s left", secs)
                    }
                } else {
                    "calculating...".to_string()
                };

                let processed_gb = progress.bytes_processed as f64 / 1_000_000_000.0;
                let total_gb = progress.bytes_total as f64 / 1_000_000_000.0;

                // Truncate filename for display (using char boundaries for UTF-8 safety)
                let filename = if progress.current_file.chars().count() > 40 {
                    let suffix: String = progress
                        .current_file
                        .chars()
                        .rev()
                        .take(37)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    format!("...{}", suffix)
                } else {
                    progress.current_file.clone()
                };

                // Format current file size for large files (>100MB)
                let file_size_str = if progress.current_file_size > 100_000_000 {
                    format!(
                        " ({:.1} GB)",
                        progress.current_file_size as f64 / 1_000_000_000.0
                    )
                } else {
                    String::new()
                };

                print!(
                    "\r[{}/{}] {:.1}% ({:.1}/{:.1} GB) {:.1} MB/s ~{} | {}{}    ",
                    progress.files_processed,
                    progress.files_total,
                    pct,
                    processed_gb,
                    total_gb,
                    speed,
                    eta,
                    filename,
                    file_size_str
                );
                use std::io::Write;
                std::io::stdout().flush().ok();
            }
        })
    };

    // Clear progress line
    if !quick && !out.is_json() {
        print!("\r{}\r", " ".repeat(100));
        use std::io::Write;
        std::io::stdout().flush().ok();
    }

    let duration = start_time.elapsed();
    let duration_ms = duration.as_millis() as u64;

    // Check permissions if requested
    let permission_mismatches = if check_permissions {
        match compare_permissions(source, dest) {
            Ok(result) => result.mismatches,
            Err(e) => {
                out.warn(&format!("Failed to compare permissions: {}", e));
                vec![]
            }
        }
    } else {
        vec![]
    };

    // Build mismatches for JSON
    let mut mismatches: Vec<VerifyMismatch> = diff_result
        .entries_by_status(DiffStatus::Modified)
        .map(|e| VerifyMismatch {
            path: e.path.to_string_lossy().to_string(),
            reason: "content_differs".to_string(),
            source_hash: None,
            dest_hash: None,
        })
        .collect();

    // Add permission mismatches
    for pm in &permission_mismatches {
        mismatches.push(VerifyMismatch {
            path: pm.path.clone(),
            reason: format!(
                "permissions_differ (source: {:o}, dest: {:o})",
                pm.source_mode, pm.dest_mode
            ),
            source_hash: None,
            dest_hash: None,
        });
    }

    let missing_in_dest: Vec<String> = diff_result
        .entries_by_status(DiffStatus::Added)
        .map(|e| e.path.to_string_lossy().to_string())
        .collect();

    let extra_in_dest: Vec<String> = diff_result
        .entries_by_status(DiffStatus::Removed)
        .map(|e| e.path.to_string_lossy().to_string())
        .collect();

    let has_permission_mismatches = !permission_mismatches.is_empty();
    let is_fully_verified = diff_result.is_identical() && !has_permission_mismatches;

    let data = VerifyData {
        source: source.to_path_buf(),
        dest: dest.to_path_buf(),
        verified: is_fully_verified,
        files_checked: diff_result.identical_count + diff_result.changes_count(),
        bytes_hashed: if quick { 0 } else { total_bytes },
        mismatches,
        missing_in_dest,
        extra_in_dest,
    };

    let response = Response::success("verify", duration_ms, data);

    if out.is_json() {
        out.result(&response);
        if !is_fully_verified {
            // Return error for scripts but after outputting JSON
            std::process::exit(1);
        }
        return Ok(());
    }

    // Human-readable output
    out.newline();

    if is_fully_verified {
        out.success(&format!(
            "Verified: {} files match",
            diff_result.identical_count
        ));
        if check_permissions {
            out.success("All permissions match");
        }
        out.newline();
        out.kv("Duration", format_duration(duration));
        if !quick {
            out.kv(
                "Hashed",
                format!(
                    "{} ({})",
                    format_bytes(total_bytes),
                    format_throughput(total_bytes, duration)
                ),
            );
        }
        return Ok(());
    }

    // Show differences
    out.error("Verification failed - directories differ:");
    out.newline();

    // Show permission mismatches
    if has_permission_mismatches {
        out.info(&format!(
            "Permission mismatches ({}):",
            permission_mismatches.len()
        ));
        for pm in permission_mismatches.iter().take(10) {
            let kind = if pm.is_dir { "dir" } else { "file" };
            out.indented(&format!(
                "{} [{}]: {:o} → {:o}",
                pm.path, kind, pm.source_mode, pm.dest_mode
            ));
        }
        if permission_mismatches.len() > 10 {
            out.indented(&format!(
                "... and {} more",
                permission_mismatches.len() - 10
            ));
        }
        out.newline();
    }

    if diff_result.added_count > 0 {
        out.info(&format!(
            "Missing in destination ({}):",
            diff_result.added_count
        ));
        for entry in diff_result.entries_by_status(DiffStatus::Added).take(10) {
            out.indented(&format!("+ {}", entry.path.display()));
        }
        if diff_result.added_count > 10 {
            out.indented(&format!("... and {} more", diff_result.added_count - 10));
        }
        out.newline();
    }

    if diff_result.removed_count > 0 {
        out.info(&format!(
            "Extra in destination ({}):",
            diff_result.removed_count
        ));
        for entry in diff_result.entries_by_status(DiffStatus::Removed).take(10) {
            out.indented(&format!("- {}", entry.path.display()));
        }
        if diff_result.removed_count > 10 {
            out.indented(&format!("... and {} more", diff_result.removed_count - 10));
        }
        out.newline();
    }

    if diff_result.modified_count > 0 {
        out.info(&format!(
            "Content differs ({}):",
            diff_result.modified_count
        ));
        for entry in diff_result.entries_by_status(DiffStatus::Modified).take(10) {
            out.indented(&format!("~ {}", entry.path.display()));
        }
        if diff_result.modified_count > 10 {
            out.indented(&format!("... and {} more", diff_result.modified_count - 10));
        }
        out.newline();
    }

    out.info("Summary:");
    out.kv("Identical", diff_result.identical_count);
    out.kv("Missing", diff_result.added_count);
    out.kv("Extra", diff_result.removed_count);
    out.kv("Different", diff_result.modified_count);
    out.newline();
    out.kv("Duration", format_duration(duration));
    if !quick {
        out.kv(
            "Hashed",
            format!(
                "{} ({})",
                format_bytes(total_bytes),
                format_throughput(total_bytes, duration)
            ),
        );
    }

    // Return error code for scripts
    anyhow::bail!(
        "Verification failed: {} differences found",
        diff_result.changes_count()
    );
}
