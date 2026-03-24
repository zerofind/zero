//! Delete command handler
//!
//! Provides file and directory deletion with:
//! - Single or batch deletion
//! - Move to trash option (macOS)
//! - Dry-run preview
//! - Recursive directory deletion

use std::path::PathBuf;
use std::time::Instant;

use delete::{DeleteOptions, delete_paths, preview_delete};
use foundation::output::{Outputter, format_bytes, format_duration};
use foundation::{cmd_error, cmd_success};

/// Arguments for the delete command
pub struct DeleteArgs {
    /// Paths to delete
    pub paths: Vec<PathBuf>,
    /// Move to system trash instead of permanent delete
    pub trash: bool,
    /// Delete directories recursively
    pub recursive: bool,
    /// Skip confirmation prompt
    pub force: bool,
    /// Show what would be deleted without actually deleting
    pub dry_run: bool,
}

/// Response data for delete command
#[derive(Debug, serde::Serialize)]
pub struct DeleteData {
    /// Paths that were requested for deletion
    pub requested: Vec<PathBuf>,
    /// Number of files deleted
    pub files_deleted: usize,
    /// Number of directories deleted
    pub dirs_deleted: usize,
    /// Total bytes deleted
    pub bytes_deleted: u64,
    /// Whether items were moved to trash (vs permanent delete)
    pub trashed: bool,
    /// Items that failed to delete
    pub failures: Vec<DeleteFailureData>,
    /// Whether this was a dry run
    pub dry_run: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct DeleteFailureData {
    pub path: PathBuf,
    pub message: String,
}

/// Response data for delete dry-run
#[derive(Debug, serde::Serialize)]
pub struct DeletePreviewData {
    /// Items that would be deleted
    pub items: Vec<DeletePreviewItem>,
    /// Total bytes that would be deleted
    pub total_bytes: u64,
    /// Total items that would be deleted
    pub total_items: usize,
    /// Items that cannot be deleted
    pub cannot_delete: Vec<DeletePreviewItem>,
}

#[derive(Debug, serde::Serialize)]
pub struct DeletePreviewItem {
    pub path: PathBuf,
    pub exists: bool,
    pub is_dir: bool,
    pub size: u64,
    pub item_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub fn cmd_delete(out: &Outputter, args: &DeleteArgs) -> anyhow::Result<()> {
    let start = Instant::now();

    if args.paths.is_empty() {
        let msg = "No paths specified for deletion".to_string();
        cmd_error!(out, "delete", 0u64, "NO_PATHS", msg, {
            out.error(&msg);
            out.info("Usage: zero delete <path>... [--trash] [--recursive] [--force]");
        });
        return Ok(());
    }

    // Convert to slice of references for the API
    let path_refs: Vec<&std::path::Path> =
        args.paths.iter().map(std::path::PathBuf::as_path).collect();

    // Dry run mode - just preview
    if args.dry_run {
        return cmd_delete_preview(out, &path_refs, args.recursive, start);
    }

    // Human mode header
    if !out.is_json() {
        if args.trash {
            out.header("Moving to Trash");
        } else {
            out.header("Deleting files");
        }
    }

    // Preview what will be deleted
    let preview = preview_delete(&path_refs, args.recursive);

    // Check if there's anything to delete
    let deletable: Vec<_> = preview.iter().filter(|p| p.can_delete).collect();
    let undeletable: Vec<_> = preview.iter().filter(|p| !p.can_delete).collect();

    if deletable.is_empty() {
        let msg = "No valid paths to delete".to_string();
        cmd_error!(
            out,
            "delete",
            start.elapsed().as_millis() as u64,
            "NOTHING_TO_DELETE",
            msg,
            {
                out.error("Nothing to delete");
                for item in &undeletable {
                    if let Some(reason) = &item.reason {
                        out.indented(&format!("{}: {}", item.path.display(), reason));
                    }
                }
            }
        );
        return Ok(());
    }

    // Show what will be deleted (human mode, unless forced)
    if !out.is_json() && !args.force {
        let total_bytes: u64 = deletable.iter().map(|p| p.size).sum();
        let total_items: usize = deletable.iter().map(|p| p.item_count).sum();

        out.info(&format!(
            "Will delete {} path(s) containing {} item(s) ({})",
            deletable.len(),
            total_items,
            format_bytes(total_bytes)
        ));
        out.newline();

        for item in &deletable {
            let type_str = if item.is_dir { "dir " } else { "file" };
            out.indented(&format!(
                "[{}] {} ({})",
                type_str,
                item.path.display(),
                format_bytes(item.size)
            ));
        }

        if !undeletable.is_empty() {
            out.newline();
            out.warn(&format!("{} path(s) cannot be deleted:", undeletable.len()));
            for item in &undeletable {
                if let Some(reason) = &item.reason {
                    out.indented(&format!("{}: {}", item.path.display(), reason));
                }
            }
        }

        out.newline();

        // Confirmation prompt
        if !args.trash {
            out.warn("⚠️  This will PERMANENTLY delete these files!");
        }

        out.println("Continue? [y/N] ");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input != "y" && input != "yes" {
            out.info("Cancelled");
            return Ok(());
        }
    }

    // Build options
    let options = DeleteOptions::new()
        .use_trash(args.trash)
        .recursive(args.recursive)
        .continue_on_error(true);

    // Perform deletion
    if !out.is_json() {
        if args.trash {
            out.info("Moving to Trash...");
        } else {
            out.info("Deleting...");
        }
    }

    let result = delete_paths(&path_refs, &options);
    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(delete_result) => {
            let data = DeleteData {
                requested: args.paths.clone(),
                files_deleted: delete_result.files_deleted,
                dirs_deleted: delete_result.dirs_deleted,
                bytes_deleted: delete_result.bytes_deleted,
                trashed: args.trash,
                failures: delete_result
                    .failures
                    .iter()
                    .map(|f| DeleteFailureData {
                        path: f.path.clone(),
                        message: f.message.clone(),
                    })
                    .collect(),
                dry_run: false,
            };

            cmd_success!(out, "delete", duration_ms, data, {
                out.newline();
                if delete_result.success() {
                    out.success(&format!(
                        "Deleted {} file(s), {} directory(s) ({})",
                        delete_result.files_deleted,
                        delete_result.dirs_deleted,
                        format_bytes(delete_result.bytes_deleted)
                    ));
                } else {
                    out.success(&format!(
                        "Deleted {} file(s), {} directory(s)",
                        delete_result.files_deleted, delete_result.dirs_deleted
                    ));
                    out.newline();
                    out.warn(&format!("{} item(s) failed:", delete_result.failures.len()));
                    for failure in &delete_result.failures {
                        out.indented(&format!("{}: {}", failure.path.display(), failure.message));
                    }
                }
                out.newline();
                out.kv("Duration", format_duration(start.elapsed()));
            });
        }
        Err(e) => {
            cmd_error!(
                out,
                "delete",
                duration_ms,
                "DELETE_FAILED",
                format!("Delete failed: {}", e)
            );
        }
    }

    Ok(())
}

#[allow(clippy::unnecessary_wraps)] // Returns Result for consistency with cmd_delete call site
fn cmd_delete_preview(
    out: &Outputter,
    paths: &[&std::path::Path],
    recursive: bool,
    start: Instant,
) -> anyhow::Result<()> {
    let preview = preview_delete(paths, recursive);

    let can_delete: Vec<_> = preview.iter().filter(|p| p.can_delete).collect();
    let cannot_delete: Vec<_> = preview.iter().filter(|p| !p.can_delete).collect();

    let total_bytes: u64 = can_delete.iter().map(|p| p.size).sum();
    let total_items: usize = can_delete.iter().map(|p| p.item_count).sum();

    let duration_ms = start.elapsed().as_millis() as u64;

    let data = DeletePreviewData {
        items: can_delete
            .iter()
            .map(|p| DeletePreviewItem {
                path: p.path.clone(),
                exists: p.exists,
                is_dir: p.is_dir,
                size: p.size,
                item_count: p.item_count,
                reason: None,
            })
            .collect(),
        total_bytes,
        total_items,
        cannot_delete: cannot_delete
            .iter()
            .map(|p| DeletePreviewItem {
                path: p.path.clone(),
                exists: p.exists,
                is_dir: p.is_dir,
                size: p.size,
                item_count: p.item_count,
                reason: p.reason.clone(),
            })
            .collect(),
    };
    cmd_success!(out, "delete", duration_ms, data, {
        out.header("Delete Preview (dry-run)");
        if can_delete.is_empty() {
            out.info("Nothing to delete");
        } else {
            out.info(&format!(
                "Would delete {} path(s) containing {} item(s) ({})",
                can_delete.len(),
                total_items,
                format_bytes(total_bytes)
            ));
            out.newline();
            for item in &can_delete {
                let type_str = if item.is_dir { "dir " } else { "file" };
                out.indented(&format!(
                    "[{}] {} ({}, {} item(s))",
                    type_str,
                    item.path.display(),
                    format_bytes(item.size),
                    item.item_count
                ));
            }
        }
        if !cannot_delete.is_empty() {
            out.newline();
            out.warn(&format!(
                "{} path(s) cannot be deleted:",
                cannot_delete.len()
            ));
            for item in &cannot_delete {
                if let Some(reason) = &item.reason {
                    out.indented(&format!("{}: {}", item.path.display(), reason));
                }
            }
        }
        out.newline();
        out.info("Run without --dry-run to actually delete");
    });

    Ok(())
}
