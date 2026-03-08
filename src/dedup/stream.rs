//! Streaming duplicate finder — emits results as groups are verified.
//!
//! Instead of returning all results at once, `find_duplicates_streaming` sends
//! events through a channel as each phase completes. This lets the UI show
//! results incrementally (largest groups first) while hashing continues.
//!
//! ## Index-accelerated mode
//!
//! When an `IndexManager` is provided, Phase 1 (discovery) is replaced by
//! an in-memory iteration over the pre-built search index — turning a
//! 30-90 second filesystem walk into a ~100ms pass.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;

use rayon::prelude::*;
use tracing::instrument;

use crate::hasher::{HashAlgorithm, hash_file_prefix, hash_file_with_buffer};
use crate::index::IndexManager;
use crate::scanner::{CrawlProgress, ScanOptions, scan_with_progress};

use super::finder::matches_type_filter;
use super::types::{DedupError, DedupOptions, DedupProgress, DuplicateGroup};

/// Prefix size for early-rejection hashing (4 KB).
const PREFIX_HASH_BYTES: u64 = 4096;

/// Events emitted during a streaming dedup scan.
#[derive(Debug)]
pub enum DedupStreamEvent {
    /// Scan-phase progress update.
    ScanProgress {
        files_found: usize,
        bytes_found: u64,
    },
    /// Scan complete; size groups formed. Sent once before hashing begins.
    SizeGroupsReady {
        group_count: usize,
        total_candidates: usize,
        bytes_to_hash: u64,
    },
    /// A hash-verified duplicate group (emitted as each group is confirmed).
    GroupVerified(DuplicateGroup),
    /// Hash-phase progress update.
    HashProgress { bytes_hashed: u64, bytes_total: u64 },
    /// Scan finished (success or cancelled).
    Complete {
        files_scanned: usize,
        bytes_scanned: u64,
    },
    /// Non-fatal error encountered during scan.
    Error(String),
}

/// Lightweight entry used during size-grouping (avoids full FileEntry weight).
struct CandidateFile {
    path: PathBuf,
    size: u64,
}

/// Find duplicates, streaming results via `tx` as groups are verified.
///
/// When `manager` is `Some`, the search index is used for discovery (Phase 1)
/// instead of walking the filesystem — dramatically faster on indexed roots.
/// Falls back to directory walking when the index is not available.
#[instrument(skip(options, progress, tx, manager), fields(path = %path.display()))]
pub fn find_duplicates_streaming(
    path: &Path,
    options: DedupOptions,
    progress: Arc<DedupProgress>,
    tx: Sender<DedupStreamEvent>,
    manager: Option<&IndexManager>,
) -> Result<(), DedupError> {
    if progress.is_cancelled() {
        let _ = tx.send(DedupStreamEvent::Complete {
            files_scanned: 0,
            bytes_scanned: 0,
        });
        return Ok(());
    }

    // --- Phase 0: Discover files (index or walk) ---
    let (candidates, files_scanned, bytes_scanned) = if let Some(mgr) = manager {
        discover_from_index(mgr, path, &options, &progress, &tx)
    } else {
        discover_from_walk(path, &options, &progress, &tx)?
    };

    if progress.is_cancelled() {
        let _ = tx.send(DedupStreamEvent::Complete {
            files_scanned,
            bytes_scanned,
        });
        return Ok(());
    }

    // --- Phase 1: Size-group ---
    let mut size_groups: HashMap<u64, Vec<CandidateFile>> = HashMap::new();
    for entry in candidates {
        size_groups.entry(entry.size).or_default().push(entry);
    }

    let mut potential_duplicates: Vec<Vec<CandidateFile>> = size_groups
        .into_values()
        .filter(|group| group.len() > 1)
        .collect();

    // Largest savings surface first
    potential_duplicates
        .sort_by_key(|group| std::cmp::Reverse(group.iter().map(|e| e.size).sum::<u64>()));

    let total_potential: usize = potential_duplicates.iter().map(|g| g.len()).sum();
    let total_bytes_to_hash: u64 = potential_duplicates
        .iter()
        .flat_map(|g| g.iter())
        .map(|e| e.size)
        .sum();

    progress
        .potential_duplicates
        .store(total_potential, Ordering::Release);
    progress
        .bytes_to_hash
        .store(total_bytes_to_hash, Ordering::Release);
    progress.phase.store(1, Ordering::Release);

    let _ = tx.send(DedupStreamEvent::SizeGroupsReady {
        group_count: potential_duplicates.len(),
        total_candidates: total_potential,
        bytes_to_hash: total_bytes_to_hash,
    });

    // --- Phase 2: Hash each size-group and emit verified groups ---
    if options.skip_hash {
        emit_size_only_groups(potential_duplicates, &tx);
    } else {
        hash_groups_streaming(potential_duplicates, &options, &progress, &tx);
    }

    // --- Phase 3: Done ---
    progress.phase.store(2, Ordering::Release);
    let _ = tx.send(DedupStreamEvent::Complete {
        files_scanned,
        bytes_scanned,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Phase 0 strategies
// ---------------------------------------------------------------------------

/// Discover candidates from the search index (sub-millisecond for millions of files).
fn discover_from_index(
    manager: &IndexManager,
    scope: &Path,
    options: &DedupOptions,
    progress: &Arc<DedupProgress>,
    tx: &Sender<DedupStreamEvent>,
) -> (Vec<CandidateFile>, usize, u64) {
    let scope_str = scope.to_string_lossy();
    let type_filter = options.type_filter;
    let min_size = options.min_size;

    let mut candidates = Vec::new();
    let mut files_scanned = 0usize;
    let mut bytes_scanned = 0u64;
    let mut last_progress_at = 0usize;

    for node in manager.iter_all() {
        if !node.is_file() {
            continue;
        }
        // Scope filter: only files under the requested path
        if !node.path.starts_with(scope_str.as_ref()) {
            continue;
        }
        files_scanned += 1;
        bytes_scanned += node.size;
        progress.files_found.fetch_add(1, Ordering::Relaxed);
        progress.bytes_found.fetch_add(node.size, Ordering::Relaxed);

        // Size filter
        if node.size < min_size {
            continue;
        }
        // Type filter (extension check on path string — same as matches_type_filter)
        if let Some(filter) = type_filter {
            let path = Path::new(&node.path);
            if !matches_type_filter(path, filter) {
                continue;
            }
        }

        candidates.push(CandidateFile {
            path: PathBuf::from(&node.path),
            size: node.size,
        });

        if files_scanned - last_progress_at >= 500 {
            last_progress_at = files_scanned;
            let _ = tx.send(DedupStreamEvent::ScanProgress {
                files_found: files_scanned,
                bytes_found: bytes_scanned,
            });
        }
    }

    (candidates, files_scanned, bytes_scanned)
}

/// Discover candidates via filesystem walk (fallback when index is unavailable).
fn discover_from_walk(
    path: &Path,
    options: &DedupOptions,
    progress: &Arc<DedupProgress>,
    tx: &Sender<DedupStreamEvent>,
) -> Result<(Vec<CandidateFile>, usize, u64), DedupError> {
    let scan_progress = Arc::new(CrawlProgress::new());
    let scan_options = ScanOptions {
        max_depth: options.max_depth.unwrap_or(usize::MAX),
        ..Default::default()
    };

    let iter = scan_with_progress(path, scan_options, Some(scan_progress.clone()))?;
    let type_filter = options.type_filter;
    let min_size = options.min_size;
    let mut candidates = Vec::new();
    let mut files_scanned = 0usize;
    let mut bytes_scanned = 0u64;
    let mut last_progress_at = 0usize;

    for result in iter {
        if progress.is_cancelled() {
            scan_progress.cancelled.store(true, Ordering::Release);
            break;
        }

        match result {
            Ok(entry) => {
                files_scanned += 1;
                bytes_scanned += entry.size;
                progress.files_found.fetch_add(1, Ordering::Relaxed);
                progress
                    .bytes_found
                    .fetch_add(entry.size, Ordering::Relaxed);

                if entry.size < min_size {
                    continue;
                }
                if let Some(filter) = type_filter
                    && !matches_type_filter(&entry.absolute_path, filter)
                {
                    continue;
                }

                candidates.push(CandidateFile {
                    path: entry.absolute_path,
                    size: entry.size,
                });

                if files_scanned - last_progress_at >= 500 {
                    last_progress_at = files_scanned;
                    let _ = tx.send(DedupStreamEvent::ScanProgress {
                        files_found: files_scanned,
                        bytes_found: bytes_scanned,
                    });
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Dedup scan: skipping inaccessible entry");
            }
        }
    }

    Ok((candidates, files_scanned, bytes_scanned))
}

// ---------------------------------------------------------------------------
// Phase 2: Hashing
// ---------------------------------------------------------------------------

/// Skip-hash mode: emit size-only groups without hashing.
fn emit_size_only_groups(groups: Vec<Vec<CandidateFile>>, tx: &Sender<DedupStreamEvent>) {
    for entries in groups {
        let size = entries.first().map(|e| e.size).unwrap_or(0);
        let files = entries.into_iter().map(|e| e.path).collect();
        let _ = tx.send(DedupStreamEvent::GroupVerified(DuplicateGroup {
            hash: format!("size:{size}"),
            size,
            files,
        }));
    }
}

/// Hash each size-group with prefix-hash early rejection, emit groups as verified.
fn hash_groups_streaming(
    groups: Vec<Vec<CandidateFile>>,
    options: &DedupOptions,
    progress: &Arc<DedupProgress>,
    tx: &Sender<DedupStreamEvent>,
) {
    let algorithm = options.hash_algorithm;
    let verify = options.verify;

    for group in groups {
        if progress.is_cancelled() {
            break;
        }

        let file_size = group.first().map(|e| e.size).unwrap_or(0);

        // Stage 1: Prefix-hash to cheaply split the group (only for files > PREFIX_HASH_BYTES)
        let sub_groups = if file_size > PREFIX_HASH_BYTES && group.len() > 2 {
            prefix_split_group(group, algorithm, progress)
        } else {
            // Small files or 2-file groups: skip prefix, go straight to full hash
            vec![group]
        };

        // Stage 2: Full-hash each sub-group to confirm true duplicates
        for sub in sub_groups {
            if sub.len() < 2 {
                // Prefix hash already proved these are unique — update progress
                let skipped: u64 = sub.iter().map(|e| e.size).sum();
                progress.bytes_hashed.fetch_add(skipped, Ordering::Release);
                progress
                    .files_hashed
                    .fetch_add(sub.len(), Ordering::Release);
                continue;
            }
            full_hash_and_emit(sub, algorithm, verify, progress, tx);
        }

        // Emit hash progress after each size-group
        let hashed = progress.bytes_hashed.load(Ordering::Relaxed);
        let total = progress.bytes_to_hash.load(Ordering::Relaxed);
        let _ = tx.send(DedupStreamEvent::HashProgress {
            bytes_hashed: hashed,
            bytes_total: total,
        });
    }
}

/// Hash the first PREFIX_HASH_BYTES of each file; split the group by prefix hash.
/// Files with different prefixes cannot be duplicates — avoids full-file reads.
fn prefix_split_group(
    group: Vec<CandidateFile>,
    algorithm: HashAlgorithm,
    progress: &Arc<DedupProgress>,
) -> Vec<Vec<CandidateFile>> {
    let prefix_results: Vec<(Option<String>, CandidateFile)> = group
        .into_par_iter()
        .map(|entry| {
            if progress.is_cancelled() {
                return (None, entry);
            }
            thread_local! {
                static BUF: std::cell::RefCell<Vec<u8>> =
                    std::cell::RefCell::new(vec![0u8; 128 * 1024]);
            }
            let prefix = BUF.with(|buf| {
                let mut buffer = buf.borrow_mut();
                hash_file_prefix(&entry.path, algorithm, PREFIX_HASH_BYTES, &mut buffer)
                    .ok()
                    .flatten()
                    .map(|r| r.to_hex())
            });
            (prefix, entry)
        })
        .collect();

    let mut buckets: HashMap<String, Vec<CandidateFile>> = HashMap::new();
    let mut no_prefix = Vec::new(); // Files smaller than prefix or errors

    for (prefix, entry) in prefix_results {
        match prefix {
            Some(h) => buckets.entry(h).or_default().push(entry),
            None => no_prefix.push(entry),
        }
    }

    let mut result: Vec<Vec<CandidateFile>> = buckets.into_values().collect();
    if !no_prefix.is_empty() {
        result.push(no_prefix);
    }
    result
}

/// Full-hash files in parallel, sub-group by hash, emit verified duplicate groups.
fn full_hash_and_emit(
    group: Vec<CandidateFile>,
    algorithm: HashAlgorithm,
    verify: bool,
    progress: &Arc<DedupProgress>,
    tx: &Sender<DedupStreamEvent>,
) {
    let progress_ref = progress.clone();

    let hash_results: Vec<(String, CandidateFile)> = group
        .into_par_iter()
        .filter_map(|entry| {
            if progress_ref.is_cancelled() {
                return None;
            }
            let size = entry.size;

            thread_local! {
                static BUF: std::cell::RefCell<Vec<u8>> =
                    std::cell::RefCell::new(vec![0u8; 128 * 1024]);
            }

            let hash = BUF.with(|buf| {
                let mut buffer = buf.borrow_mut();
                let result = hash_file_with_buffer(&entry.path, algorithm, &mut buffer);
                match result {
                    Ok(h) => {
                        if verify {
                            let v = hash_file_with_buffer(&entry.path, algorithm, &mut buffer);
                            match v {
                                Ok(v2) if v2.hash == h.hash => Some(h.to_hex()),
                                _ => None,
                            }
                        } else {
                            Some(h.to_hex())
                        }
                    }
                    Err(e) => {
                        tracing::warn!(path = %entry.path.display(), "Failed to hash: {}", e);
                        None
                    }
                }
            });

            hash.map(|h| {
                progress_ref.files_hashed.fetch_add(1, Ordering::Release);
                progress_ref.bytes_hashed.fetch_add(size, Ordering::Release);
                (h, entry)
            })
        })
        .collect();

    // Sub-group by hash
    let mut hash_groups: HashMap<String, Vec<CandidateFile>> = HashMap::new();
    for (hash, entry) in hash_results {
        hash_groups.entry(hash).or_default().push(entry);
    }

    for (hash, entries) in hash_groups {
        if entries.len() > 1 {
            let size = entries.first().map(|e| e.size).unwrap_or(0);
            let files = entries.into_iter().map(|e| e.path).collect();
            let _ = tx.send(DedupStreamEvent::GroupVerified(DuplicateGroup {
                hash,
                size,
                files,
            }));
        }
    }
}

#[cfg(test)]
#[path = "stream_test.rs"]
mod stream_test;
