//! Duplicate file finder and remover (batch API).
//!
//! For streaming results, see `stream::find_duplicates_streaming`.
//!
//! ## Panic Safety (Early Exit Strategy)
//!
//! Parallel operations are wrapped in catch_unwind to ensure
//! clean process exit on fatal errors.

use std::fs;
use std::panic;
use std::path::Path;
use std::sync::Arc;

use rayon::prelude::*;
use tracing::instrument;

use crate::index::FileTypeCategory;

use super::stream::{DedupStreamEvent, find_duplicates_streaming};
use super::types::{DedupError, DedupOptions, DedupProgress, DedupResult, DeleteResult};

/// Find duplicate files in a directory (convenience wrapper).
pub fn find_duplicates(path: &Path, options: DedupOptions) -> Result<DedupResult, DedupError> {
    find_duplicates_with_progress(path, options, None)
}

/// Find duplicate files with optional progress tracking.
///
/// This is a batch wrapper around the streaming API — it collects all
/// `GroupVerified` events into a single `DedupResult`.
#[instrument(skip(options, progress), fields(path = %path.display()))]
pub fn find_duplicates_with_progress(
    path: &Path,
    options: DedupOptions,
    progress: Option<Arc<DedupProgress>>,
) -> Result<DedupResult, DedupError> {
    let progress = progress.unwrap_or_else(|| Arc::new(DedupProgress::new()));
    let (tx, rx) = std::sync::mpsc::channel();

    find_duplicates_streaming(path, options, progress, tx, None)?;

    let mut groups = Vec::new();
    let mut files_scanned = 0;
    let mut bytes_scanned = 0;

    for event in rx {
        match event {
            DedupStreamEvent::GroupVerified(g) => groups.push(g),
            DedupStreamEvent::Complete {
                files_scanned: f,
                bytes_scanned: b,
            } => {
                files_scanned = f;
                bytes_scanned = b;
            }
            _ => {}
        }
    }

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

/// Delete duplicate files (keeps one copy per group — shortest path).
#[instrument(skip(result), fields(groups = result.groups.len()))]
pub fn delete_duplicates(result: &DedupResult) -> DeleteResult {
    let files_to_delete: Vec<(&std::path::PathBuf, u64)> = result
        .groups
        .iter()
        .flat_map(|group| {
            group
                .duplicates_to_delete()
                .into_iter()
                .map(|p| (p, group.size))
        })
        .collect();

    let delete_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        files_to_delete
            .par_iter()
            .map(|(path, size)| match fs::remove_file(path) {
                Ok(()) => Ok(*size),
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to delete duplicate");
                    Err(())
                }
            })
            .collect::<Vec<Result<u64, ()>>>()
    }));

    let delete_results: Vec<Result<u64, ()>> = match delete_result {
        Ok(results) => results,
        Err(panic_payload) => {
            eprintln!("\n⚠ Fatal error during parallel duplicate deletion");
            panic::resume_unwind(panic_payload);
        }
    };

    let mut files_deleted = 0;
    let mut bytes_reclaimed = 0;
    let mut errors = 0;

    for result in delete_results {
        match result {
            Ok(size) => {
                files_deleted += 1;
                bytes_reclaimed += size;
            }
            Err(()) => errors += 1,
        }
    }

    DeleteResult {
        files_deleted,
        bytes_reclaimed,
        errors,
    }
}

/// Check if a file path matches a type filter.
pub(super) fn matches_type_filter(path: &std::path::Path, filter: FileTypeCategory) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    let ext = match ext {
        Some(e) => e,
        None => return false,
    };

    match filter {
        FileTypeCategory::Images => matches!(
            ext.as_str(),
            "jpg"
                | "jpeg"
                | "png"
                | "gif"
                | "bmp"
                | "svg"
                | "webp"
                | "ico"
                | "tiff"
                | "tif"
                | "heic"
                | "heif"
                | "raw"
                | "cr2"
                | "nef"
                | "arw"
                | "dng"
                | "psd"
                | "ai"
                | "eps"
        ),
        FileTypeCategory::Videos => matches!(
            ext.as_str(),
            "mp4"
                | "mov"
                | "avi"
                | "mkv"
                | "wmv"
                | "flv"
                | "webm"
                | "m4v"
                | "mpeg"
                | "mpg"
                | "3gp"
                | "ogv"
                | "mts"
                | "m2ts"
        ),
        FileTypeCategory::Audio => matches!(
            ext.as_str(),
            "mp3"
                | "wav"
                | "flac"
                | "aac"
                | "ogg"
                | "wma"
                | "m4a"
                | "aiff"
                | "opus"
                | "mid"
                | "midi"
                | "ape"
                | "alac"
        ),
        FileTypeCategory::Documents => matches!(
            ext.as_str(),
            "pdf"
                | "doc"
                | "docx"
                | "xls"
                | "xlsx"
                | "ppt"
                | "pptx"
                | "odt"
                | "ods"
                | "odp"
                | "rtf"
                | "txt"
                | "md"
                | "markdown"
                | "csv"
                | "pages"
                | "numbers"
                | "key"
                | "epub"
                | "mobi"
        ),
        FileTypeCategory::Code => matches!(
            ext.as_str(),
            "rs" | "py"
                | "js"
                | "ts"
                | "jsx"
                | "tsx"
                | "java"
                | "c"
                | "cpp"
                | "h"
                | "hpp"
                | "go"
                | "rb"
                | "php"
                | "swift"
                | "kt"
                | "scala"
                | "cs"
                | "vb"
                | "lua"
                | "pl"
                | "sh"
                | "bash"
                | "zsh"
                | "fish"
                | "ps1"
                | "bat"
                | "cmd"
                | "html"
                | "htm"
                | "css"
                | "scss"
                | "sass"
                | "less"
                | "vue"
                | "svelte"
                | "sql"
                | "r"
                | "m"
                | "mm"
                | "zig"
                | "nim"
                | "d"
                | "ex"
                | "exs"
                | "erl"
                | "clj"
                | "hs"
                | "ml"
        ),
        FileTypeCategory::Archives => matches!(
            ext.as_str(),
            "zip"
                | "tar"
                | "gz"
                | "bz2"
                | "xz"
                | "7z"
                | "rar"
                | "tgz"
                | "tbz2"
                | "txz"
                | "lz"
                | "lzma"
                | "cab"
                | "iso"
                | "dmg"
                | "pkg"
                | "deb"
                | "rpm"
        ),
        FileTypeCategory::Config => matches!(
            ext.as_str(),
            "json"
                | "yaml"
                | "yml"
                | "toml"
                | "xml"
                | "ini"
                | "conf"
                | "cfg"
                | "env"
                | "properties"
                | "plist"
        ),
        FileTypeCategory::Executables => matches!(
            ext.as_str(),
            "exe"
                | "dll"
                | "so"
                | "dylib"
                | "app"
                | "bin"
                | "msi"
                | "apk"
                | "ipa"
                | "deb"
                | "rpm"
                | "appimage"
                | "snap"
                | "flatpak"
                | "wasm"
        ),
        FileTypeCategory::Fonts => matches!(
            ext.as_str(),
            "ttf" | "otf" | "woff" | "woff2" | "eot" | "fon" | "fnt"
        ),
        FileTypeCategory::Files => true,
        FileTypeCategory::Directories => false,
        FileTypeCategory::Rust => ext == "rs",
        FileTypeCategory::Go => ext == "go",
        FileTypeCategory::Swift => ext == "swift",
        FileTypeCategory::TypeScript => matches!(ext.as_str(), "ts" | "tsx"),
        FileTypeCategory::JavaScript => matches!(ext.as_str(), "js" | "jsx" | "mjs" | "cjs"),
        FileTypeCategory::Python => ext == "py",
    }
}

#[cfg(test)]
#[path = "finder_test.rs"]
mod finder_test;
