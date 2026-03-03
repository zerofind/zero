//! Diff command handler

use std::path::Path;
use std::time::Instant;
use zero::cmd_success;
use zero::differ::{DiffOptions, DiffStatus, diff_with_options};
use zero::hasher::HashAlgorithm;
use zero::output::*;
use zero::scanner::{ScanOptions, scan_collect};
use zero::transfer::compare_permissions;

#[allow(clippy::too_many_arguments)]
pub fn cmd_diff(
    out: &Outputter,
    source: &Path,
    dest: &Path,
    use_checksum: bool,
    full: bool,
    check_permissions: bool,
    show_identical: bool,
    max_depth: Option<usize>,
) -> anyhow::Result<()> {
    // --full implies --checksum
    let use_checksum = use_checksum || full;

    // When checksum mode is active, delegate to the verify path which has
    // cache acceleration, permission checks, and richer output.
    if use_checksum {
        return super::verify::cmd_verify(
            out,
            source,
            dest,
            max_depth,
            false, // quick
            full,
            check_permissions,
        );
    }

    let start = Instant::now();

    out.header(&format!(
        "Comparing {} → {}",
        source.display(),
        dest.display()
    ));
    out.info("Comparing by metadata (size + mtime)");

    let scan_options = ScanOptions {
        max_depth: max_depth.unwrap_or(usize::MAX),
        ..Default::default()
    };

    // Scan both directories
    out.info("Scanning source...");
    let source_entries = scan_collect(source, scan_options.clone())?;
    out.indented(&format!("Found {} files", source_entries.len()));

    out.info("Scanning destination...");
    let dest_entries = scan_collect(dest, scan_options)?;
    out.indented(&format!("Found {} files", dest_entries.len()));
    out.newline();

    // Compare
    out.info("Comparing...");
    let diff_options = DiffOptions {
        use_checksum,
        include_identical: show_identical,
        hash_algorithm: HashAlgorithm::default(),
    };

    let result = diff_with_options(&source_entries, &dest_entries, &diff_options);
    let duration = start.elapsed();
    let duration_ms = duration.as_millis() as u64;

    // Build changes list for JSON
    let changes: Vec<DiffChange> = result
        .entries
        .iter()
        .filter(|e| e.status != DiffStatus::Identical || show_identical)
        .map(|e| DiffChange {
            status: format!("{:?}", e.status).to_lowercase(),
            path: e.path.to_string_lossy().to_string(),
            source_size: e.source_size,
            dest_size: e.dest_size,
            source_modified: e.source_mtime,
            dest_modified: e.dest_mtime,
        })
        .collect();

    let data = DiffData {
        source: source.to_path_buf(),
        dest: dest.to_path_buf(),
        summary: DiffOutputSummary {
            identical: result.identical_count,
            modified: result.modified_count,
            source_only: result.added_count,
            dest_only: result.removed_count,
        },
        changes,
    };

    // Check permissions if requested
    if check_permissions {
        match compare_permissions(source, dest) {
            Ok(perm_result) => {
                if !perm_result.mismatches.is_empty() {
                    out.newline();
                    out.info(&format!(
                        "Permission mismatches ({}):",
                        perm_result.mismatches.len()
                    ));
                    for pm in perm_result.mismatches.iter().take(10) {
                        let kind = if pm.is_dir { "dir" } else { "file" };
                        out.indented(&format!(
                            "{} [{}]: {:o} → {:o}",
                            pm.path, kind, pm.source_mode, pm.dest_mode
                        ));
                    }
                    if perm_result.mismatches.len() > 10 {
                        out.indented(&format!(
                            "... and {} more",
                            perm_result.mismatches.len() - 10
                        ));
                    }
                }
            }
            Err(e) => {
                out.warn(&format!("Failed to compare permissions: {}", e));
            }
        }
    }

    cmd_success!(out, "diff", duration_ms, data, {
        out.newline();
        out.info("Results:");
        out.kv("Source only", format!("{:>8} files", result.added_count));
        out.kv("Dest only", format!("{:>8} files", result.removed_count));
        out.kv("Modified", format!("{:>8} files", result.modified_count));
        out.kv("Identical", format!("{:>8} files", result.identical_count));
        out.newline();
        out.kv("Bytes to transfer", format_bytes(result.bytes_to_transfer));
        out.kv("Duration", format_duration(duration));
        if result.changes_count() > 0 {
            out.newline();
            out.info("Changes:");
            for entry in result.entries_by_status(DiffStatus::Added).take(20) {
                out.indented(&format!(
                    "{} {}",
                    entry.status.symbol(),
                    entry.path.display()
                ));
            }
            if result.added_count > 20 {
                out.indented(&format!("... and {} more added", result.added_count - 20));
            }
            for entry in result.entries_by_status(DiffStatus::Removed).take(20) {
                out.indented(&format!(
                    "{} {}",
                    entry.status.symbol(),
                    entry.path.display()
                ));
            }
            if result.removed_count > 20 {
                out.indented(&format!(
                    "... and {} more removed",
                    result.removed_count - 20
                ));
            }
            for entry in result.entries_by_status(DiffStatus::Modified).take(20) {
                out.indented(&format!(
                    "{} {}",
                    entry.status.symbol(),
                    entry.path.display()
                ));
            }
            if result.modified_count > 20 {
                out.indented(&format!(
                    "... and {} more modified",
                    result.modified_count - 20
                ));
            }
        } else {
            out.newline();
            out.success("Directories are identical!");
        }
    });

    Ok(())
}
