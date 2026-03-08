//! Dev garbage scanner - finds build artifacts using sibling-aware matching
//!
//! Walks the filesystem directly (no index dependency) and identifies
//! developer build directories like `target/`, `node_modules/`, `.next/`
//! by checking for sibling project files (e.g. `Cargo.toml` next to `target/`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use jwalk::WalkDir;

use crate::profiles::{self, AggregateMode, CleanupGroup, MergedCleanupProfile};

/// A matched dev garbage directory
#[derive(Debug, Clone, serde::Serialize)]
pub struct DevGarbageItem {
    /// Full path to the garbage directory
    pub path: PathBuf,
    /// Directory name (e.g. "target")
    pub dir_name: String,
    /// Recursive size in bytes
    pub total_bytes: u64,
    /// Number of files inside
    pub file_count: usize,
    /// Category ID from profile (e.g. "rust_target")
    pub category_id: String,
    /// Display name (e.g. "Rust Build Cache")
    pub category_name: String,
    /// Project root containing the sibling file
    pub project_root: PathBuf,
    /// Warning text from profile
    pub warning: Option<String>,
}

/// Full scan result
#[derive(Debug, Clone, serde::Serialize)]
pub struct DevScanResult {
    /// Matched garbage directories
    pub items: Vec<DevGarbageItem>,
    /// Total bytes across all items
    pub total_bytes: u64,
    /// Number of directories scanned
    pub dirs_scanned: usize,
    /// How long the scan took
    pub scan_duration_ms: u64,
}

/// Lock-free progress counters for scan
pub struct DevScanProgress {
    pub dirs_scanned: AtomicUsize,
    pub garbage_found: AtomicUsize,
    pub bytes_found: AtomicU64,
    pub cancelled: AtomicBool,
}

impl Default for DevScanProgress {
    fn default() -> Self {
        Self {
            dirs_scanned: AtomicUsize::new(0),
            garbage_found: AtomicUsize::new(0),
            bytes_found: AtomicU64::new(0),
            cancelled: AtomicBool::new(false),
        }
    }
}

impl DevScanProgress {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Errors from the dev scanner
#[derive(Debug, thiserror::Error)]
pub enum DevScanError {
    #[error("root path does not exist: {0}")]
    RootNotFound(PathBuf),
    #[error("profile error: {0}")]
    Profile(#[from] profiles::ProfileError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Category info extracted from profile for scanner use
struct CategoryMatch {
    id: String,
    name: String,
    sibling: Option<String>,
    warning: Option<String>,
}

/// Scan a directory tree for developer build artifacts.
///
/// Uses sibling-aware matching: a `target/` directory only matches if
/// `Cargo.toml` exists in the same parent directory.
pub fn scan_dev_garbage(
    root: &Path,
    category_filter: Option<&str>,
    max_depth: Option<usize>,
    progress: Option<Arc<DevScanProgress>>,
) -> Result<DevScanResult, DevScanError> {
    let root = root
        .canonicalize()
        .map_err(|_| DevScanError::RootNotFound(root.to_path_buf()))?;
    let start = Instant::now();

    // Load profile and build lookup table
    let profile = profiles::load_cleanup()?;
    let lookup = build_lookup_table(profile, category_filter);

    if lookup.is_empty() {
        return Ok(DevScanResult {
            items: Vec::new(),
            total_bytes: 0,
            dirs_scanned: 0,
            scan_duration_ms: 0,
        });
    }

    let mut items = Vec::new();
    let mut dirs_scanned: usize = 0;

    // Build a second copy for the process_read_dir callback (needs 'static)
    let callback_lookup = build_lookup_table(profile, category_filter);

    // Walk the tree, checking each directory entry
    let depth = max_depth.unwrap_or(usize::MAX);
    let walker = WalkDir::new(&root)
        .skip_hidden(false)
        .max_depth(depth)
        .sort(false)
        .process_read_dir(move |_depth, _path, _state, children| {
            // Mark garbage dirs so we skip descending into them
            for entry in children.iter_mut().flatten() {
                if !entry.file_type().is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(categories) = callback_lookup.get(name.as_str()) {
                    let entry_path = entry.path();
                    let parent = match entry_path.parent() {
                        Some(p) => p,
                        None => continue,
                    };

                    for cat in categories {
                        let matched = match &cat.sibling {
                            Some(sibling) => parent.join(sibling).exists(),
                            None => true,
                        };
                        if matched {
                            entry.read_children_path = None;
                            break;
                        }
                    }
                }
            }
        });

    for entry in walker {
        if let Some(ref p) = progress
            && p.cancelled.load(Ordering::Relaxed)
        {
            break;
        }

        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_dir() {
            continue;
        }

        dirs_scanned += 1;
        if let Some(ref p) = progress {
            p.dirs_scanned.store(dirs_scanned, Ordering::Relaxed);
        }

        let name = entry.file_name().to_string_lossy().to_string();
        let categories = match lookup.get(name.as_str()) {
            Some(c) => c,
            None => continue,
        };

        let entry_path = entry.path();
        let parent = match entry_path.parent() {
            Some(p) => p,
            None => continue,
        };

        for cat in categories {
            let matched = match &cat.sibling {
                Some(sibling) => parent.join(sibling).exists(),
                None => true,
            };

            if !matched {
                continue;
            }

            let (total_bytes, file_count) = dir_size(&entry_path);

            let item = DevGarbageItem {
                path: entry_path.clone(),
                dir_name: name.clone(),
                total_bytes,
                file_count,
                category_id: cat.id.clone(),
                category_name: cat.name.clone(),
                project_root: parent.to_path_buf(),
                warning: cat.warning.clone(),
            };

            if let Some(ref p) = progress {
                p.garbage_found.fetch_add(1, Ordering::Relaxed);
                p.bytes_found.fetch_add(total_bytes, Ordering::Relaxed);
            }

            items.push(item);
            break;
        }
    }

    // Sort by size descending
    items.sort_by_key(|i| std::cmp::Reverse(i.total_bytes));
    let total_bytes = items.iter().map(|i| i.total_bytes).sum();
    let scan_duration = start.elapsed();

    Ok(DevScanResult {
        items,
        total_bytes,
        dirs_scanned,
        scan_duration_ms: scan_duration.as_millis() as u64,
    })
}

/// Build a lookup table mapping directory names to matching categories
fn build_lookup_table(
    profile: &MergedCleanupProfile,
    category_filter: Option<&str>,
) -> HashMap<String, Vec<CategoryMatch>> {
    let mut lookup: HashMap<String, Vec<CategoryMatch>> = HashMap::new();

    for cat in profile.all_categories() {
        if cat.aggregate != AggregateMode::Directory || cat.group != CleanupGroup::Developer {
            continue;
        }

        if let Some(filter) = category_filter
            && cat.id != filter
        {
            continue;
        }

        for pattern in &cat.patterns {
            if let Some(name) = extract_dir_name(pattern) {
                let cm = CategoryMatch {
                    id: cat.id.clone(),
                    name: cat.name.clone(),
                    sibling: cat.sibling.clone(),
                    warning: cat.warning.clone(),
                };
                lookup.entry(name.to_string()).or_default().push(cm);
            }
        }
    }

    lookup
}

/// Extract directory name from a glob pattern like "**/target" -> "target"
fn extract_dir_name(pattern: &str) -> Option<&str> {
    if let Some(name) = pattern.strip_prefix("**/") {
        // Handle nested patterns like "**/vendor/bundle" - use first component
        if let Some(slash) = name.find('/') {
            Some(&name[..slash])
        } else {
            Some(name)
        }
    } else {
        None
    }
}

/// Calculate total size and file count for a directory
fn dir_size(path: &Path) -> (u64, usize) {
    let mut total: u64 = 0;
    let mut count: usize = 0;

    for entry in WalkDir::new(path)
        .skip_hidden(false)
        .sort(false)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() {
            total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            count += 1;
        }
    }

    (total, count)
}
