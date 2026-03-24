//! Cleanup command handler
//!
//! Scans for developer build artifacts (`node_modules`, target, .next, etc.)
//! and offers to clean them by moving to Trash.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use cleanup::{DevScanProgress, scan_dev_garbage};
use delete::{DeleteOptions, delete_paths};
use foundation::output::{Outputter, format_bytes};
use foundation::{cmd_error, cmd_success};

use super::super::CleanupCommands;

pub fn cmd_cleanup(out: &Outputter, cleanup_cmd: CleanupCommands) -> anyhow::Result<()> {
    match cleanup_cmd {
        CleanupCommands::Dev {
            path,
            dry_run,
            all,
            category,
            max_depth,
        } => cmd_cleanup_dev(out, &path, dry_run, all, category.as_deref(), max_depth),
    }
}

fn cmd_cleanup_dev(
    out: &Outputter,
    path: &PathBuf,
    dry_run: bool,
    all: bool,
    category: Option<&str>,
    max_depth: Option<usize>,
) -> anyhow::Result<()> {
    let start = Instant::now();
    let resolved = std::fs::canonicalize(path)?;

    if !out.is_json() {
        let display = if resolved == dirs::home_dir().unwrap_or_default() {
            "~".to_string()
        } else {
            resolved.display().to_string()
        };
        out.println(&format!(
            "Scanning {display} for developer build artifacts..."
        ));
        out.newline();
    }

    let progress = Arc::new(DevScanProgress::new());
    let result = scan_dev_garbage(&resolved, category, max_depth, Some(&progress));

    let duration_ms = start.elapsed().as_millis() as u64;

    let scan_result = match result {
        Ok(r) => r,
        Err(e) => {
            cmd_error!(
                out,
                "cleanup",
                duration_ms,
                "SCAN_FAILED",
                format!("Scan failed: {}", e)
            );
            return Ok(());
        }
    };

    if scan_result.items.is_empty() {
        cmd_success!(out, "cleanup", duration_ms, &scan_result, {
            out.println("No developer build artifacts found.");
        });
        return Ok(());
    }

    // Display results
    if out.is_json() {
        let response = foundation::output::Response::success("cleanup", duration_ms, &scan_result);
        out.result(&response);
        if dry_run || !all {
            return Ok(());
        }
    }

    if !out.is_json() {
        out.println(&format!(
            "Developer Build Artifacts ({} total)",
            format_bytes(scan_result.total_bytes)
        ));
        out.newline();

        // Table header
        out.println(&format!(
            "  {:<4} {:<50} {:>10}   {}",
            "#", "Path", "Size", "Type"
        ));

        let home = dirs::home_dir();
        for (i, item) in scan_result.items.iter().enumerate() {
            let display_path = shorten_path(&item.path, home.as_deref());
            out.println(&format!(
                "  {:<4} {:<50} {:>10}   {}",
                i + 1,
                display_path,
                format_bytes(item.total_bytes),
                item.category_name,
            ));
        }
        out.newline();
    }

    if dry_run {
        if !out.is_json() {
            out.println(&format!(
                "Scanned {} directories in {}ms",
                scan_result.dirs_scanned, scan_result.scan_duration_ms,
            ));
        }
        return Ok(());
    }

    // Determine which items to clean
    let indices = if all {
        (0..scan_result.items.len()).collect::<Vec<_>>()
    } else {
        // Interactive selection
        out.println("Select items to clean (e.g. 1,3,5 or 'all' or 'q'): ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input == "q" || input == "quit" || input.is_empty() {
            out.println("Cancelled.");
            return Ok(());
        }

        if input == "all" {
            (0..scan_result.items.len()).collect()
        } else {
            parse_selection(input, scan_result.items.len())
        }
    };

    if indices.is_empty() {
        out.println("No valid items selected.");
        return Ok(());
    }

    // SAFETY(index): indices are validated by parse_selection to be in bounds
    let selected: Vec<_> = indices
        .iter()
        .filter_map(|&i| scan_result.items.get(i))
        .collect();
    let selected_bytes: u64 = selected.iter().map(|i| i.total_bytes).sum();

    if !out.is_json() {
        out.println(&format!(
            "Moving to Trash: {} ({} items)...",
            format_bytes(selected_bytes),
            selected.len(),
        ));
    }

    let paths: Vec<&std::path::Path> = selected.iter().map(|i| i.path.as_path()).collect();
    let options = DeleteOptions::new()
        .use_trash(true)
        .recursive(true)
        .continue_on_error(true);

    let delete_result = delete_paths(&paths, &options);
    let total_duration_ms = start.elapsed().as_millis() as u64;

    match delete_result {
        Ok(dr) => {
            // Record telemetry
            let categories: Vec<String> =
                selected.iter().map(|i| i.category_name.clone()).collect();
            telemetry::record_cleanup(&categories, selected_bytes);

            #[derive(serde::Serialize)]
            struct CleanupData {
                cleaned_bytes: u64,
                cleaned_count: usize,
                failures: usize,
            }
            let data = CleanupData {
                cleaned_bytes: selected_bytes,
                cleaned_count: selected.len(),
                failures: dr.failures.len(),
            };
            cmd_success!(out, "cleanup", total_duration_ms, data, {
                out.newline();
                if dr.failures.is_empty() {
                    out.println(&format!(
                        "Done. Moved {} to Trash.",
                        format_bytes(selected_bytes),
                    ));
                } else {
                    out.println(&format!(
                        "Cleaned {} items, {} failed:",
                        selected.len() - dr.failures.len(),
                        dr.failures.len(),
                    ));
                    for f in &dr.failures {
                        out.println(&format!("  {}: {}", f.path.display(), f.message));
                    }
                }
            });
        }
        Err(e) => {
            cmd_error!(
                out,
                "cleanup",
                total_duration_ms,
                "DELETE_FAILED",
                format!("Cleanup failed: {}", e)
            );
        }
    }

    Ok(())
}

/// Shorten a path by replacing home dir with ~
fn shorten_path(path: &std::path::Path, home: Option<&std::path::Path>) -> String {
    if let Some(home) = home
        && let Ok(relative) = path.strip_prefix(home)
    {
        return format!("~/{}", relative.display());
    }
    path.display().to_string()
}

/// Parse user selection like "1,3,5" or "1-3,5" into indices
fn parse_selection(input: &str, max: usize) -> Vec<usize> {
    let mut indices = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if let Some((start, end)) = part.split_once('-') {
            if let (Ok(s), Ok(e)) = (start.trim().parse::<usize>(), end.trim().parse::<usize>()) {
                for i in s..=e {
                    if i >= 1 && i <= max {
                        indices.push(i - 1);
                    }
                }
            }
        } else if let Ok(n) = part.parse::<usize>()
            && n >= 1
            && n <= max
        {
            indices.push(n - 1);
        }
    }
    indices.sort_unstable();
    indices.dedup();
    indices
}
