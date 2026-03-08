//! Duplicate files command handler
//!
//! Supports multiple input modes:
//! - Path scan: `zero dupes ~/Photos`
//! - With query filter: `zero dupes ~/Photos "vacation"`
//! - With type filter: `zero dupes ~/Photos --type images`
//! - From stdin pipe: `zero search "old" | zero dupes`
//! - From file: `zero dupes --from-file paths.txt`

use std::io::{self, BufRead, IsTerminal};
use std::path::{Path, PathBuf};
use std::time::Instant;

use zero::dedup::{DedupOptions, DedupResult, delete_duplicates, find_duplicates};
use zero::index::{FileTypeCategory, IndexManager, SearchIndex, SearchQuery};
use zero::output::*;
use zero::{cmd_error, cmd_success};

/// Arguments for the dupes command
pub struct DupesArgs {
    /// Path to scan (optional if using stdin or from-file)
    pub path: Option<PathBuf>,
    /// Search query to filter files
    pub query: Option<String>,
    /// Type filter (images, videos, etc.)
    pub type_filter: Option<FileTypeCategory>,
    /// Read paths from file
    pub from_file: Option<PathBuf>,
    /// Delete duplicates
    pub delete: bool,
    /// Verify checksums before delete
    pub verify: bool,
    /// Maximum depth
    pub max_depth: Option<usize>,
    /// Minimum file size
    pub min_size: u64,
}

pub fn cmd_dupes(out: &Outputter, args: &DupesArgs) -> anyhow::Result<()> {
    let start_time = Instant::now();

    // Determine input mode
    let stdin_is_pipe = !io::stdin().is_terminal();

    // Priority: from-file > path arg > stdin pipe
    // (path arg takes precedence over stdin because user explicitly specified it)
    if let Some(ref file_path) = args.from_file {
        // Read paths from file
        cmd_dupes_from_file(out, args, file_path, start_time)
    } else if let Some(ref path) = args.path {
        // Scan directory (with optional query/type filter)
        if args.query.is_some() || args.type_filter.is_some() {
            cmd_dupes_filtered(out, args, path, start_time)
        } else {
            cmd_dupes_scan(out, args, path, start_time)
        }
    } else if stdin_is_pipe {
        // Read paths from stdin (only if no path arg provided)
        cmd_dupes_from_stdin(out, args, start_time)
    } else {
        // No input provided
        cmd_error!(
            out,
            "dupes",
            0u64,
            "NO_INPUT",
            "No input provided. Specify a path, use --from-file, or pipe paths via stdin."
                .to_string(),
            {
                out.error("No input provided");
                out.newline();
                out.info("Usage:");
                out.indented("zero dupes <path>                    # Scan directory");
                out.indented("zero dupes <path> \"query\"            # Filter by search query");
                out.indented("zero dupes <path> --type images      # Filter by file type");
                out.indented("zero search ... | zero dupes       # Read from pipe");
                out.indented("zero dupes --from-file paths.txt     # Read from file");
            }
        );
        Ok(())
    }
}

/// Standard directory scan for duplicates
fn cmd_dupes_scan(
    out: &Outputter,
    args: &DupesArgs,
    path: &Path,
    start_time: Instant,
) -> anyhow::Result<()> {
    out.header(&format!("Finding duplicates in {}", path.display()));
    print_mode_info(out, args);
    out.newline();
    out.info("Scanning and hashing files...");

    let options = DedupOptions {
        max_depth: args.max_depth,
        min_size: args.min_size,
        verify: args.verify,
        ..Default::default()
    };

    let result = find_duplicates(path, options)?;
    output_results(out, args, &result, Some(path), start_time)
}

/// Filtered dedup using search index
fn cmd_dupes_filtered(
    out: &Outputter,
    args: &DupesArgs,
    path: &Path,
    start_time: Instant,
) -> anyhow::Result<()> {
    // Build header message
    let mut header = format!("Finding duplicates in {}", path.display());
    if let Some(ref query) = args.query {
        header.push_str(&format!(" matching \"{}\"", query));
    }
    if let Some(type_filter) = args.type_filter {
        header.push_str(&format!(" (type: {:?})", type_filter).to_lowercase());
    }
    out.header(&header);
    print_mode_info(out, args);
    out.newline();

    // Load search index via IndexManager
    out.info("Loading search index...");
    let manager = match IndexManager::load() {
        Ok(m) if m.total_file_count() > 0 => m,
        _ => {
            cmd_error!(
                out,
                "dupes",
                start_time.elapsed().as_millis() as u64,
                "INDEX_NOT_FOUND",
                "Search index not found. Run 'zero search --index <path>' first.".to_string(),
                {
                    out.error("Search index not found");
                    out.info("Run 'zero search --index <path>' to build the index first");
                }
            );
            return Ok(());
        }
    };

    let index = match manager.indexes().next() {
        Some(idx) => idx,
        None => return Ok(()),
    };

    // Search for files matching criteria
    out.info("Searching for matching files...");
    let paths = search_files(index, args.query.as_deref(), args.type_filter, path);

    if paths.is_empty() {
        let data = DupesData {
            path: Some(path.to_path_buf()),
            files_scanned: 0,
            bytes_scanned: 0,
            duplicate_groups: vec![],
            total_duplicate_count: 0,
            total_wasted_bytes: 0,
            deleted: false,
            files_deleted: None,
            bytes_reclaimed: None,
        };
        cmd_success!(
            out,
            "dupes",
            start_time.elapsed().as_millis() as u64,
            data,
            {
                out.newline();
                out.success("No matching files found");
            }
        );
        return Ok(());
    }

    out.info(&format!(
        "Found {} matching files, checking for duplicates...",
        paths.len()
    ));

    // Run dedup on the filtered paths
    let result = find_duplicates_in_paths(&paths, args)?;
    output_results(out, args, &result, Some(path), start_time)
}

/// Read paths from stdin pipe
///
/// Supports two formats:
/// - `path\tsize` (from `zero search` pipe) - skips filesystem metadata calls
/// - `path` only (fallback) - requires filesystem stat for each file
fn cmd_dupes_from_stdin(
    out: &Outputter,
    args: &DupesArgs,
    start_time: Instant,
) -> anyhow::Result<()> {
    out.header("Finding duplicates from piped paths");
    print_mode_info(out, args);
    out.newline();
    out.info("Reading paths from stdin...");

    // Parse stdin - try to extract path and size from tab-separated format
    let mut entries: Vec<(PathBuf, u64)> = Vec::new();
    let mut paths_only: Vec<PathBuf> = Vec::new();
    let mut has_sizes = false;

    for line in io::stdin().lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Try to parse as "path\tsize" format (from zero search pipe)
        if let Some((path_str, size_str)) = line.split_once('\t')
            && let Ok(size) = size_str.parse::<u64>()
        {
            let path = PathBuf::from(path_str);
            if size >= args.min_size {
                entries.push((path, size));
                has_sizes = true;
            }
            continue;
        }

        // Fallback: path only (will need to stat later)
        paths_only.push(PathBuf::from(line));
    }

    // If we got sizes from stdin, use the fast path (no filesystem calls for grouping)
    if has_sizes && !entries.is_empty() {
        out.info(&format!(
            "Read {} files with sizes, checking for duplicates (fast mode)...",
            entries.len()
        ));

        let result = find_duplicates_in_entries(entries, args)?;
        return output_results(out, args, &result, None, start_time);
    }

    // Fallback: paths only, need to stat each file
    let paths: Vec<PathBuf> = paths_only.into_iter().filter(|p| p.exists()).collect();

    if paths.is_empty() {
        cmd_error!(
            out,
            "dupes",
            start_time.elapsed().as_millis() as u64,
            "NO_VALID_PATHS",
            "No valid paths found in stdin"
        );
        return Ok(());
    }

    out.info(&format!(
        "Read {} valid paths, checking for duplicates...",
        paths.len()
    ));

    let result = find_duplicates_in_paths(&paths, args)?;
    output_results(out, args, &result, None, start_time)
}

/// Read paths from file
fn cmd_dupes_from_file(
    out: &Outputter,
    args: &DupesArgs,
    file_path: &Path,
    start_time: Instant,
) -> anyhow::Result<()> {
    out.header(&format!("Finding duplicates from {}", file_path.display()));
    print_mode_info(out, args);
    out.newline();
    out.info("Reading paths from file...");

    let content = std::fs::read_to_string(file_path)?;
    let paths: Vec<PathBuf> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();

    if paths.is_empty() {
        cmd_error!(
            out,
            "dupes",
            start_time.elapsed().as_millis() as u64,
            "NO_VALID_PATHS",
            "No valid paths found in file"
        );
        return Ok(());
    }

    out.info(&format!(
        "Read {} valid paths, checking for duplicates...",
        paths.len()
    ));

    let result = find_duplicates_in_paths(&paths, args)?;
    output_results(out, args, &result, None, start_time)
}

/// Search files using the index with optional query and type filter
fn search_files(
    index: &SearchIndex,
    query: Option<&str>,
    type_filter: Option<FileTypeCategory>,
    base_path: &Path,
) -> Vec<PathBuf> {
    // Get the index root to reconstruct full paths
    let index_root = match index.root() {
        Some(r) => PathBuf::from(r),
        None => return Vec::new(), // No root means empty/invalid index
    };

    // Canonicalize both paths to handle symlinks (e.g., /tmp -> /private/tmp on macOS)
    let canonical_base = base_path
        .canonicalize()
        .unwrap_or_else(|_| base_path.to_path_buf());
    let canonical_root = index_root
        .canonicalize()
        .unwrap_or_else(|_| index_root.clone());

    let base_path_str = canonical_base.to_string_lossy();

    // Build search query
    let limit = 100_000; // High limit for dedup purposes
    let query_str = query.unwrap_or("");

    let mut all_results = Vec::new();

    // Only search if we have a type filter or a query
    if type_filter.is_some() || !query_str.is_empty() {
        let q = SearchQuery::text(query_str, limit)
            .with_type_opt(type_filter)
            .files_only();
        let results = index.query(q);

        for result in results {
            let full_path = canonical_root.join(&result.node.path);
            if full_path
                .to_string_lossy()
                .starts_with(base_path_str.as_ref())
            {
                all_results.push(full_path);
            }
        }
    }

    // Deduplicate paths (in case of overlapping extension searches)
    all_results.sort();
    all_results.dedup();

    all_results
}

/// Find duplicates among entries with known sizes (fast path - no stat calls for grouping)
fn find_duplicates_in_entries(
    entries: Vec<(PathBuf, u64)>,
    _args: &DupesArgs,
) -> anyhow::Result<DedupResult> {
    use rayon::prelude::*;
    use std::collections::HashMap;
    use zero::hasher::{HashAlgorithm, hash_file_with_buffer};

    let files_scanned = entries.len();
    let bytes_scanned: u64 = entries.iter().map(|(_, size)| *size).sum();

    // Group by size first (fast filter) - no filesystem calls needed!
    let mut size_groups: HashMap<u64, Vec<(PathBuf, u64)>> = HashMap::new();
    for (path, size) in entries {
        size_groups.entry(size).or_default().push((path, size));
    }

    // Keep only groups with 2+ files
    let potential_duplicates: Vec<Vec<(PathBuf, u64)>> = size_groups
        .into_values()
        .filter(|group| group.len() > 1)
        .collect();

    // Hash files in parallel
    let hash_results: Vec<(String, PathBuf, u64)> = potential_duplicates
        .into_par_iter()
        .flat_map(|group| {
            group
                .into_par_iter()
                .filter_map(|(path, size)| {
                    thread_local! {
                        static BUFFER: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(vec![0u8; 128 * 1024]);
                    }

                    BUFFER.with(|buf| {
                        let mut buffer = buf.borrow_mut();
                        hash_file_with_buffer(&path, HashAlgorithm::default(), &mut buffer)
                            .ok()
                            .map(|hash_result| (hash_result.to_hex(), path, size))
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();

    // Group by hash
    let mut hash_groups: HashMap<String, Vec<(PathBuf, u64)>> = HashMap::new();
    for (hash, path, size) in hash_results {
        hash_groups.entry(hash).or_default().push((path, size));
    }

    // Build result
    let groups: Vec<zero::dedup::DuplicateGroup> = hash_groups
        .into_iter()
        .filter(|(_, entries)| entries.len() > 1)
        .map(|(hash, entries)| {
            let size = entries.first().map(|(_, s)| *s).unwrap_or(0);
            let files: Vec<PathBuf> = entries.into_iter().map(|(p, _)| p).collect();
            zero::dedup::DuplicateGroup { hash, size, files }
        })
        .collect();

    let duplicate_count: usize = groups.iter().map(|g| g.files.len() - 1).sum();
    let wasted_bytes: u64 = groups.iter().map(|g| g.wasted_bytes()).sum();

    Ok(DedupResult {
        groups,
        files_scanned,
        bytes_scanned,
        duplicate_count,
        wasted_bytes,
    })
}

/// Find duplicates among a specific set of paths (requires stat calls)
fn find_duplicates_in_paths(paths: &[PathBuf], args: &DupesArgs) -> anyhow::Result<DedupResult> {
    use rayon::prelude::*;
    use std::collections::HashMap;
    use zero::hasher::{HashAlgorithm, hash_file_with_buffer};

    // Group by size first (fast filter)
    let mut size_groups: HashMap<u64, Vec<&PathBuf>> = HashMap::new();

    for path in paths {
        if let Ok(metadata) = path.metadata()
            && metadata.is_file()
            && metadata.len() >= args.min_size
        {
            size_groups.entry(metadata.len()).or_default().push(path);
        }
    }

    // Keep only groups with 2+ files
    let potential_duplicates: Vec<Vec<&PathBuf>> = size_groups
        .into_values()
        .filter(|group| group.len() > 1)
        .collect();

    // Hash files in parallel
    let hash_results: Vec<(String, &PathBuf, u64)> = potential_duplicates
        .into_par_iter()
        .flat_map(|group| {
            group
                .into_par_iter()
                .filter_map(|path| {
                    thread_local! {
                        static BUFFER: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(vec![0u8; 128 * 1024]);
                    }

                    BUFFER.with(|buf| {
                        let mut buffer = buf.borrow_mut();
                        let size = path.metadata().ok()?.len();

                        hash_file_with_buffer(path, HashAlgorithm::default(), &mut buffer)
                            .ok()
                            .map(|hash_result| (hash_result.to_hex(), path, size))
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();

    // Group by hash
    let mut hash_groups: HashMap<String, Vec<(&PathBuf, u64)>> = HashMap::new();
    for (hash, path, size) in hash_results {
        hash_groups.entry(hash).or_default().push((path, size));
    }

    // Build result
    let groups: Vec<zero::dedup::DuplicateGroup> = hash_groups
        .into_iter()
        .filter(|(_, entries)| entries.len() > 1)
        .map(|(hash, entries)| {
            let size = entries.first().map(|(_, s)| *s).unwrap_or(0);
            let files: Vec<PathBuf> = entries.into_iter().map(|(p, _)| p.clone()).collect();
            zero::dedup::DuplicateGroup { hash, size, files }
        })
        .collect();

    let files_scanned = paths.len();
    let bytes_scanned: u64 = paths
        .iter()
        .filter_map(|p| p.metadata().ok())
        .map(|m| m.len())
        .sum();

    let duplicate_count: usize = groups.iter().map(|g| g.files.len() - 1).sum();
    let wasted_bytes: u64 = groups.iter().map(|g| g.wasted_bytes()).sum();

    Ok(DedupResult {
        groups,
        files_scanned,
        bytes_scanned,
        duplicate_count,
        wasted_bytes,
    })
}

/// Print mode info (verify/delete)
fn print_mode_info(out: &Outputter, args: &DupesArgs) {
    if args.verify {
        out.info("Verify mode: checksums will be double-checked");
    }
    if args.delete {
        out.info("Delete mode: duplicates will be removed (keeping shortest path)");
    }
}

/// Output the dedup results
fn output_results(
    out: &Outputter,
    args: &DupesArgs,
    result: &DedupResult,
    path: Option<&Path>,
    start_time: Instant,
) -> anyhow::Result<()> {
    let duration = start_time.elapsed();
    let duration_ms = duration.as_millis() as u64;

    // Build response data
    let duplicate_groups: Vec<DuplicateGroupData> = result
        .groups
        .iter()
        .map(|group| {
            let keeper = group.keeper();
            DuplicateGroupData {
                hash: group.hash.clone(),
                size: group.size,
                count: group.files.len(),
                wasted_bytes: group.wasted_bytes(),
                files: group
                    .files
                    .iter()
                    .map(|f| DuplicateFileData {
                        path: f.clone(),
                        keep: Some(f) == keeper,
                    })
                    .collect(),
            }
        })
        .collect();

    // Handle delete if requested
    let (files_deleted, bytes_reclaimed) = if args.delete && result.has_duplicates() {
        out.newline();
        out.info("Deleting duplicates...");
        let delete_result = delete_duplicates(result);
        (
            Some(delete_result.files_deleted),
            Some(delete_result.bytes_reclaimed),
        )
    } else {
        (None, None)
    };

    let data = DupesData {
        path: path.map(|p| p.to_path_buf()),
        files_scanned: result.files_scanned,
        bytes_scanned: result.bytes_scanned,
        duplicate_groups,
        total_duplicate_count: result.duplicate_count,
        total_wasted_bytes: result.wasted_bytes,
        deleted: args.delete && result.has_duplicates(),
        files_deleted,
        bytes_reclaimed,
    };

    cmd_success!(out, "dupes", duration_ms, data, {
        out.newline();

        if !result.has_duplicates() {
            out.success("No duplicates found");
            out.newline();
            out.kv("Files scanned", result.files_scanned);
            out.kv("Bytes scanned", format_mb(result.bytes_scanned));
            out.kv("Duration", format_duration(duration));
            return Ok(());
        }

        out.info(&format!(
            "Found {} duplicate groups ({} duplicate files, {} reclaimable)",
            result.groups.len(),
            result.duplicate_count,
            format_bytes(result.wasted_bytes)
        ));
        out.newline();

        for (i, group) in result.groups.iter().enumerate() {
            let keeper = group.keeper();
            out.info(&format!(
                "Group {} ({} files, {} each)",
                i + 1,
                group.files.len(),
                format_bytes(group.size)
            ));
            out.indented(&format!(
                "Hash: {}...",
                &group.hash[..12.min(group.hash.len())]
            ));
            for file in &group.files {
                let marker = if Some(file) == keeper { " [keep]" } else { "" };
                out.indented(&format!("{}{}", file.display(), marker));
            }
            out.newline();
        }

        if let (Some(deleted), Some(reclaimed)) = (files_deleted, bytes_reclaimed) {
            out.newline();
            out.success("Delete complete!");
            out.kv("Files deleted", deleted);
            out.kv("Space reclaimed", format_bytes(reclaimed));
        } else if !args.delete {
            out.info("Run with --delete to remove duplicates");
        }

        out.newline();
        out.kv("Duration", format_duration(duration));
    });

    Ok(())
}
