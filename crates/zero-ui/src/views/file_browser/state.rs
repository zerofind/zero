use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use crate::models::{SortDirection, SortField};
pub use crate::ui::{format_date, format_size};

/// A single entry in the file browser — file or folder.
#[derive(Debug, Clone)]
pub struct BrowserEntry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub mtime: u64,
    pub is_dir: bool,
    pub extension: Option<String>,
    pub depth: usize,
    pub expanded: bool,
}

impl BrowserEntry {
    pub fn from_fs(path: &Path, depth: usize) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let is_dir = metadata.is_dir();
        let extension = if is_dir {
            None
        } else {
            path.extension().map(|e| e.to_string_lossy().to_string())
        };
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Some(Self {
            name,
            path: path.to_path_buf(),
            size: if is_dir { 0 } else { metadata.len() },
            mtime,
            is_dir,
            extension,
            depth,
            expanded: false,
        })
    }

    pub fn kind_label(&self) -> &str {
        if self.is_dir {
            return "Folder";
        }
        match self.extension.as_deref() {
            Some("rs") => "Rust Source",
            Some("py") => "Python Script",
            Some("js") => "JavaScript",
            Some("ts") => "TypeScript",
            Some("swift") => "Swift Source",
            Some("go") => "Go Source",
            Some("jpg") | Some("jpeg") => "JPEG Image",
            Some("png") => "PNG Image",
            Some("gif") => "GIF Image",
            Some("svg") => "SVG Image",
            Some("pdf") => "PDF Document",
            Some("mp4") => "MP4 Video",
            Some("mp3") => "MP3 Audio",
            Some("zip") => "ZIP Archive",
            Some("json") => "JSON",
            Some("toml") => "TOML",
            Some("yaml") | Some("yml") => "YAML",
            Some("md") => "Markdown",
            Some("txt") => "Plain Text",
            Some(ext) => ext, // fallback to raw extension
            None => "Document",
        }
    }
}

/// Load immediate children of a directory.
pub fn load_directory(dir: &Path) -> Vec<BrowserEntry> {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut entries: Vec<BrowserEntry> = read_dir
        .flatten()
        .filter(|e| {
            // Skip hidden files
            !e.file_name().to_string_lossy().starts_with('.')
        })
        .filter_map(|e| BrowserEntry::from_fs(&e.path(), 0))
        .collect();

    // Directories first, then alphabetical
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    entries
}

/// Sort entries by the given field and direction.
pub fn sort_entries(entries: &mut [BrowserEntry], field: SortField, direction: SortDirection) {
    entries.sort_by(|a, b| {
        // Directories always come first
        let dir_order = b.is_dir.cmp(&a.is_dir);
        if dir_order != Ordering::Equal {
            return dir_order;
        }

        let cmp = match field {
            SortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortField::DateModified => a.mtime.cmp(&b.mtime),
            SortField::Size => a.size.cmp(&b.size),
            SortField::Kind => {
                let ak = a.extension.as_deref().unwrap_or("");
                let bk = b.extension.as_deref().unwrap_or("");
                ak.cmp(bk)
            }
        };

        match direction {
            SortDirection::Ascending => cmp,
            SortDirection::Descending => cmp.reverse(),
        }
    });
}

/// Toggle expansion of a directory in the flat entries list.
/// Returns the number of children inserted.
pub fn toggle_expand(entries: &mut Vec<BrowserEntry>, idx: usize) -> usize {
    if !entries[idx].is_dir {
        return 0;
    }

    if entries[idx].expanded {
        // Collapse: remove all children with depth > current
        let depth = entries[idx].depth;
        entries[idx].expanded = false;
        let mut remove_count = 0;
        let start = idx + 1;
        while start < entries.len() && entries[start].depth > depth {
            entries.remove(start);
            remove_count += 1;
        }
        return remove_count;
    }

    // Expand: load children and insert after current
    entries[idx].expanded = true;
    let dir_path = entries[idx].path.clone();
    let child_depth = entries[idx].depth + 1;

    let mut children = load_directory(&dir_path);
    for child in &mut children {
        child.depth = child_depth;
    }

    let count = children.len();
    let insert_pos = idx + 1;
    for (i, child) in children.into_iter().enumerate() {
        entries.insert(insert_pos + i, child);
    }

    count
}
