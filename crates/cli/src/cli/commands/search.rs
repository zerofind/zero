//! Search command handler

use std::io::IsTerminal;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use foundation::output::Outputter;
use scanner::CrawlProgress;
use search::{
    FileTypeCategory, IndexManager, IndexWatcher, SearchIndex, SearchQuery, SortBy, WatcherConfig,
    hash_path, persistence,
};

/// Build or rebuild the search index
pub fn cmd_search_index(
    out: &Outputter,
    path: &Path,
    cache_path: Option<&Path>,
) -> anyhow::Result<()> {
    let start = Instant::now();

    out.header(&format!("Building search index for {}", path.display()));

    // Set up progress tracking
    let progress = Arc::new(CrawlProgress::new());
    let progress_clone = Arc::clone(&progress);

    // Spawn progress display thread
    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_flag_clone = Arc::clone(&stop_flag);

    let progress_handle = thread::spawn(move || {
        let mut last_count = 0;
        while !stop_flag_clone.load(std::sync::atomic::Ordering::Relaxed) {
            let files = progress_clone.files();
            let bytes = progress_clone.bytes();
            if files != last_count {
                eprint!(
                    "\r  Scanning... {} files ({:.1} MB)    ",
                    files,
                    bytes as f64 / 1_000_000.0
                );
                last_count = files;
            }
            thread::sleep(Duration::from_millis(100));
        }
        eprintln!();
    });

    if let Some(custom_path) = cache_path {
        // Custom path: build a standalone index and save directly
        let mut index = SearchIndex::new();
        index.build_from_path_with_progress(path, Some(progress))?;

        stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = progress_handle.join();

        let duration = start.elapsed();
        out.info(&format!(
            "Indexed {} files ({:.1} MB) in {:.2}s",
            index.file_count(),
            index.total_bytes() as f64 / 1_000_000.0,
            duration.as_secs_f64()
        ));
        out.indented(&format!("{} unique filenames", index.unique_names()));

        out.info(&format!("Saving index to {}", custom_path.display()));
        persistence::save_index(&index, custom_path)?;
    } else {
        // Default: use IndexManager (saves to ~/.zero/indexes/)
        let mut manager = IndexManager::new()?;
        let root = path.to_string_lossy().to_string();
        manager.add_root_with_progress(&root, Some(progress))?;

        stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = progress_handle.join();

        let duration = start.elapsed();
        let stats = manager.root_stats(&root);
        let file_count = stats.as_ref().map_or(0, |s| s.file_count);
        let total_bytes = stats.as_ref().map_or(0, |s| s.total_bytes);

        out.info(&format!(
            "Indexed {} files ({:.1} MB) in {:.2}s",
            file_count,
            total_bytes as f64 / 1_000_000.0,
            duration.as_secs_f64()
        ));
    }

    out.success("Search index built successfully");

    Ok(())
}

pub struct SearchOptions<'a> {
    pub query: Option<&'a str>,
    pub path_filter: Option<&'a Path>,
    pub cache_path: Option<&'a Path>,
    pub limit: usize,
    pub count_only: bool,
    pub files_only: bool,
    pub dirs_only: bool,
    pub extensions: &'a [String],
    pub type_filter: Option<FileTypeCategory>,
    pub recent: Option<usize>,
    pub sort: Option<&'a str>,
    pub min_size: Option<&'a str>,
    pub max_size: Option<&'a str>,
    pub exclude_hidden: bool,
    pub open: bool,
    pub reveal: bool,
}

/// Parse a human-readable size string like "1KB", "10MB", "1GB" to bytes
fn parse_size(s: &str) -> anyhow::Result<u64> {
    let s = s.trim();
    let (num_str, unit) = if let Some(pos) = s.find(|c: char| c.is_ascii_alphabetic()) {
        (&s[..pos], s[pos..].to_uppercase())
    } else {
        (s, String::new())
    };

    let num: f64 = num_str
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid size: {s}"))?;

    let multiplier: u64 = match unit.as_str() {
        "" | "B" => 1,
        "KB" | "K" => 1024,
        "MB" | "M" => 1024 * 1024,
        "GB" | "G" => 1024 * 1024 * 1024,
        "TB" | "T" => 1024 * 1024 * 1024 * 1024,
        _ => anyhow::bail!("Unknown size unit: {unit}. Use B, KB, MB, GB, TB"),
    };

    Ok((num * multiplier as f64) as u64)
}

/// Parse sort string to `SortBy`
fn parse_sort(s: &str) -> SortBy {
    match s.to_lowercase().as_str() {
        "recent" | "date" | "mtime" => SortBy::RecentFirst,
        "size" | "size-desc" | "largest" => SortBy::SizeDesc,
        "size-asc" | "smallest" => SortBy::SizeAsc,
        "name" | "alpha" => SortBy::NameAsc,
        "relevance" | "score" => SortBy::Relevance,
        _ => SortBy::Relevance,
    }
}

/// Search for files matching a query
pub fn cmd_search(out: &Outputter, opts: &SearchOptions<'_>) -> anyhow::Result<()> {
    let SearchOptions {
        query,
        path_filter,
        cache_path,
        limit,
        count_only,
        files_only,
        dirs_only,
        extensions,
        type_filter,
        recent,
        sort,
        min_size,
        max_size,
        exclude_hidden,
        open,
        reveal,
    } = *opts;
    let total_start = Instant::now();

    // Parse size filters
    let min_size_bytes = match min_size {
        Some(s) => Some(parse_size(s)?),
        None => None,
    };
    let max_size_bytes = match max_size {
        Some(s) => Some(parse_size(s)?),
        None => None,
    };

    // Load index
    let load_start = Instant::now();

    enum IndexSource {
        Standalone(Box<SearchIndex>),
        Manager(Box<IndexManager>),
    }

    let source = if let Some(p) = cache_path {
        if !p.is_file() {
            out.error("Search index not found at specified path.");
            return Ok(());
        }
        IndexSource::Standalone(Box::new(persistence::load_index(p)?))
    } else {
        let mgr = IndexManager::load()?;
        if mgr.total_file_count() == 0 {
            out.error("Search index not found. Run 'zero search --index <path>' first.");
            return Ok(());
        }
        IndexSource::Manager(Box::new(mgr))
    };

    let index: &SearchIndex = match &source {
        IndexSource::Standalone(idx) => idx,
        IndexSource::Manager(mgr) => mgr.indexes().next().unwrap(),
    };
    let load_duration = load_start.elapsed();

    let search_start = Instant::now();
    let query_str = query.unwrap_or("");

    // Resolve path filter to absolute path for prefix matching
    let path_prefix: Option<String> = path_filter.map(|p| {
        let path_str = p.to_string_lossy();
        let expanded = if let Some(rest) = path_str.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(rest)
            } else {
                p.to_path_buf()
            }
        } else if path_str == "~" {
            dirs::home_dir().unwrap_or_else(|| p.to_path_buf())
        } else if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(p)
        };
        let canonical = expanded.canonicalize().unwrap_or(expanded);
        let mut prefix = canonical.to_string_lossy().to_string();
        if !prefix.ends_with('/') {
            prefix.push('/');
        }
        prefix
    });

    let is_piped = !std::io::stdout().is_terminal();

    let effective_limit = if limit == 0 || count_only || is_piped {
        usize::MAX
    } else {
        limit
    };

    let search_limit = if path_prefix.is_some() && effective_limit != usize::MAX {
        effective_limit.saturating_mul(100).max(10000)
    } else {
        effective_limit
    };

    // Build unified search query
    let results = if query_str.is_empty()
        && type_filter.is_none()
        && extensions.is_empty()
        && recent.is_none()
        && path_filter.is_none()
    {
        out.info(
            "No query provided. Use --type, --ext, or a path to filter, or provide a search term.",
        );
        return Ok(());
    } else {
        let q = if let Some(recent_limit) = recent {
            SearchQuery::recent(recent_limit).with_type_opt(type_filter)
        } else if path_filter.is_some()
            && query_str.is_empty()
            && type_filter.is_none()
            && extensions.is_empty()
        {
            SearchQuery::all(search_limit)
        } else {
            let mut q = SearchQuery::text(query_str, search_limit).with_type_opt(type_filter);
            // Handle extensions: single or multi
            if extensions.len() == 1 {
                q = q.with_extension(&extensions[0]);
            } else if extensions.len() > 1 {
                q = q.with_extensions(extensions.to_vec());
            }
            if files_only {
                q = q.files_only();
            }
            if dirs_only {
                q = q.dirs_only();
            }
            if exclude_hidden {
                q = q.exclude_hidden();
            }
            if let Some(min) = min_size_bytes {
                q = q.with_min_size(min);
            }
            if let Some(max) = max_size_bytes {
                q = q.with_max_size(max);
            }
            if let Some(s) = sort {
                q = q.sort(parse_sort(s));
            }
            q
        };
        index.query(q)
    };

    // Apply path filter if specified, then apply the actual limit
    let results: Vec<_> = if let Some(ref prefix) = path_prefix {
        results
            .into_iter()
            .filter(|r| {
                r.node.path.starts_with(prefix) || r.node.path == prefix.trim_end_matches('/')
            })
            .take(effective_limit)
            .collect()
    } else {
        results
    };

    // Handle --open / --reveal for the first result
    if (open || reveal) && !results.is_empty() {
        let first_path = &results[0].node.path;
        if reveal {
            let _ = std::process::Command::new("open")
                .arg("-R")
                .arg(first_path)
                .spawn();
        } else {
            let _ = std::process::Command::new("open").arg(first_path).spawn();
        }
        if !count_only && !is_piped {
            out.info(&format!(
                "{} {}",
                if reveal { "Revealing" } else { "Opening" },
                first_path
            ));
        }
    }

    if count_only {
        let total_duration = total_start.elapsed();
        let search_duration = search_start.elapsed();

        let display_query = build_display_query(query_str, type_filter, extensions, &path_prefix);

        out.header(&format!(
            "Count: {} files matching '{}' (search: {:.2}ms, load: {:.0}ms)",
            results.len(),
            display_query,
            search_duration.as_secs_f64() * 1000.0,
            total_duration.as_secs_f64() * 1000.0
        ));
        return Ok(());
    }

    let search_duration = search_start.elapsed();
    let total_duration = total_start.elapsed();

    let display_query = build_display_query(query_str, type_filter, extensions, &path_prefix);

    if results.is_empty() {
        if !is_piped {
            out.info(&format!("No results for '{display_query}'"));
        }
    } else if is_piped {
        for result in &results {
            if result.node.is_file() {
                println!("{}\t{}", result.node.path, result.node.size);
            }
        }
    } else {
        out.header(&format!(
            "Found {} results for '{}' (search: {:.2}ms, load: {:.0}ms, total: {:.1}ms)",
            results.len(),
            display_query,
            search_duration.as_secs_f64() * 1000.0,
            load_duration.as_secs_f64() * 1000.0,
            total_duration.as_secs_f64() * 1000.0
        ));

        for result in &results {
            let type_indicator = if result.node.is_directory() {
                "📁"
            } else {
                "📄"
            };

            let size_str = if result.node.is_file() {
                format_size(result.node.size)
            } else {
                String::new()
            };

            out.indented(&format!(
                "{} {} {}",
                type_indicator, result.node.path, size_str
            ));
        }
    }

    if !is_piped {
        out.newline();
        out.info(&format!(
            "Index: {} files, {} unique names",
            index.file_count(),
            index.unique_names()
        ));
    }

    telemetry::record_search(index.file_count() as u64);

    Ok(())
}

/// Build display query string for output
fn build_display_query(
    query_str: &str,
    type_filter: Option<FileTypeCategory>,
    extensions: &[String],
    path_prefix: &Option<String>,
) -> String {
    let mut parts = Vec::new();

    if !query_str.is_empty() {
        parts.push(query_str.to_string());
    }

    if let Some(t) = type_filter {
        parts.push(format!("type:{t:?}").to_lowercase());
    }

    if !extensions.is_empty() {
        parts.push(format!("ext:{}", extensions.join(",")));
    }

    if let Some(prefix) = path_prefix {
        parts.push(format!("in:{}", prefix.trim_end_matches('/')));
    }

    if parts.is_empty() {
        "(all)".to_string()
    } else {
        parts.join(" ")
    }
}

/// Format file size for display
fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return String::new();
    }
    if bytes < 1024 {
        format!("({bytes}B)")
    } else if bytes < 1024 * 1024 {
        format!("({:.1}KB)", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("({:.1}MB)", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("({:.1}GB)", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// List available type filters
pub fn cmd_search_types(out: &Outputter) -> anyhow::Result<()> {
    out.header("Available file type filters (powered by roaring bitmaps)");
    out.newline();

    let types = [
        (
            "images",
            "PNG, JPG, JPEG, GIF, WEBP, HEIC, BMP, TIFF, SVG, RAW, PSD...",
        ),
        ("videos", "MP4, MOV, AVI, MKV, WEBM, M4V, WMV, FLV, MPEG..."),
        ("audio", "MP3, WAV, FLAC, AAC, OGG, M4A, OPUS, WMA, AIFF..."),
        (
            "documents",
            "PDF, DOC, DOCX, XLS, XLSX, PPT, TXT, MD, RTF, EPUB...",
        ),
        (
            "code",
            "All source code (RS, PY, JS, TS, GO, JAVA, C, CPP...)",
        ),
        ("archives", "ZIP, TAR, GZ, 7Z, RAR, DMG, ISO, XZ, ZST..."),
        ("config", "JSON, YAML, TOML, XML, INI, ENV, PLIST..."),
        ("executables", "EXE, APP, APK, JAR, DEB, RPM, BIN..."),
        ("fonts", "TTF, OTF, WOFF, WOFF2, EOT..."),
        ("files", "All regular files (not directories)"),
        ("directories", "All directories/folders"),
    ];

    for (name, extensions) in types {
        out.info(&format!("  {name:12} → {extensions}"));
    }

    out.newline();
    out.header("Quick language filters:");
    let lang_types = [
        ("rs", "Rust (.rs)"),
        ("go", "Go (.go)"),
        ("swift", "Swift (.swift)"),
        ("ts", "TypeScript (.ts, .tsx, .mts)"),
        ("js", "JavaScript (.js, .jsx, .mjs)"),
        ("py", "Python (.py, .pyi)"),
    ];

    for (name, extensions) in lang_types {
        out.info(&format!("  {name:12} → {extensions}"));
    }

    out.newline();
    out.header("Usage examples:");
    out.indented("zero search --type images              # List all images (<1ms!)");
    out.indented("zero search --type images -n 100       # First 100 images");
    out.indented("zero search vacation --type images     # Images matching 'vacation'");
    out.indented("zero search --type videos              # List all videos");
    out.indented("zero search --type code                # List all source code files");

    Ok(())
}

/// Watch a directory for changes and update the index in real-time
pub fn cmd_search_watch(
    out: &Outputter,
    path: &Path,
    cache_path: Option<&Path>,
) -> anyhow::Result<()> {
    out.header(&format!("Watching {} for changes", path.display()));

    // Load or build index, and determine save path for periodic snapshots
    let (index, save_path) = if let Some(custom_path) = cache_path {
        if custom_path.is_file() {
            out.info(&format!(
                "Loading existing index from {}",
                custom_path.display()
            ));
            (
                persistence::load_index(custom_path)?,
                custom_path.to_path_buf(),
            )
        } else {
            out.info("Building initial index...");
            let mut index = SearchIndex::new();
            index.build_from_path(path)?;
            persistence::save_index(&index, custom_path)?;
            (index, custom_path.to_path_buf())
        }
    } else {
        let root = path.to_string_lossy().to_string();
        let manager = IndexManager::load()?;
        let idx_path = manager
            .indexes_dir()
            .join(format!("{}.zidx", hash_path(&root)));

        let index = if let Some(idx) = manager.get_index(&root) {
            out.info("Loading existing index...");
            idx.clone()
        } else {
            out.info("Building initial index...");
            drop(manager);
            let mut mgr = IndexManager::new()?;
            mgr.add_root(&root)?;
            mgr.get_index(&root).cloned().unwrap_or_default()
        };
        (index, idx_path)
    };

    out.info(&format!(
        "Index: {} files, {} unique names",
        index.file_count(),
        index.unique_names()
    ));

    // Wrap index in Arc<RwLock> for sharing with watcher
    let index = Arc::new(RwLock::new(index));

    // Create watcher
    let config = WatcherConfig::default();
    let mut watcher = IndexWatcher::with_config(Arc::clone(&index), config)?;

    // Start watching
    watcher.watch(path)?;
    out.success(&format!("Watching {} for changes", path.display()));
    out.newline();
    out.info("Press Ctrl+C to stop watching");
    out.newline();

    telemetry::record_search_watch();

    // Track stats for change detection
    let mut last_processed = 0u64;
    let mut last_save_processed = 0u64;

    // Main loop - poll for events and print stats
    loop {
        // Process any pending events (with 500ms timeout)
        let processed = watcher.process_events_timeout(Duration::from_millis(500));

        let stats = watcher.stats();

        // Only print if something changed
        if stats.events_processed != last_processed {
            let idx = index.read().unwrap();
            out.indented(&format!(
                "Events: {} received, {} processed | Index: {} files | +{} -{} ~{}",
                stats.events_received,
                stats.events_processed,
                idx.file_count(),
                stats.files_added,
                stats.files_removed,
                stats.files_updated,
            ));

            last_processed = stats.events_processed;

            // Save index periodically when there are changes (every 10 events)
            if stats.events_processed >= last_save_processed + 10 {
                if let Err(e) = persistence::save_index(&idx, &save_path) {
                    out.warn(&format!("Failed to save index: {e}"));
                } else {
                    out.indented(&format!(
                        "  Index saved ({} events)",
                        stats.events_processed
                    ));
                }
                last_save_processed = stats.events_processed;
            }
        }

        // Also save if we processed events but haven't saved in a while
        if processed > 0 && last_processed > last_save_processed {
            // Will save on next iteration if more events come
        }
    }
}
