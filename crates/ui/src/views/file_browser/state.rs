use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use crate::models::{SortDirection, SortField};
pub use crate::ui::{format_date, format_size};
use natord::compare_ignore_case;

/// Symlink target information.
#[derive(Debug, Clone)]
pub enum SymlinkInfo {
    /// Target exists.
    Valid(String),
    /// Target is broken (dangling).
    Broken(String),
}

/// Owner (user:group) information.
#[derive(Debug, Clone)]
pub struct OwnerInfo {
    pub user: String,
    pub group: String,
}

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
    /// Unix mode bits from stat (e.g. 0o755). Populated on Unix.
    pub mode: Option<u32>,
    /// Whether the file has extended attributes.
    pub has_xattrs: bool,
    /// Whether this entry is a symbolic link.
    pub is_symlink: bool,
    /// Symlink target path and validity.
    pub symlink_target: Option<SymlinkInfo>,
    /// BSD file flags string (macOS). Empty or "-" means none.
    pub flags: Option<String>,
    /// Owner user:group.
    pub owner: Option<OwnerInfo>,
}

impl BrowserEntry {
    pub fn from_fs(path: &Path, depth: usize) -> Option<Self> {
        // Use symlink_metadata to detect symlinks without following them
        let symlink_meta = std::fs::symlink_metadata(path).ok()?;
        let is_symlink = symlink_meta.is_symlink();

        // For actual file info (size, is_dir), follow the link
        let metadata = if is_symlink {
            std::fs::metadata(path).ok()
        } else {
            Some(symlink_meta.clone())
        };

        // If the symlink is broken, metadata will be None
        let (is_dir, size, mtime) = if let Some(m) = &metadata {
            let mt = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            (m.is_dir(), if m.is_dir() { 0 } else { m.len() }, mt)
        } else {
            // Broken symlink: use symlink_metadata for mtime
            let mt = symlink_meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            (false, 0, mt)
        };

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let extension = if is_dir {
            None
        } else {
            path.extension().map(|e| e.to_string_lossy().to_string())
        };

        // Extract Unix metadata from symlink_metadata (don't follow links for mode)
        let (mode, owner, flags) = extract_unix_metadata(&symlink_meta);

        let has_xattrs = has_extended_attrs(path);

        let symlink_target = if is_symlink {
            Some(resolve_symlink(path))
        } else {
            None
        };

        Some(Self {
            name,
            path: path.to_path_buf(),
            size,
            mtime,
            is_dir,
            extension,
            depth,
            expanded: false,
            mode,
            has_xattrs,
            is_symlink,
            symlink_target,
            flags,
            owner,
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
            Some("jpg" | "jpeg") => "JPEG Image",
            Some("png") => "PNG Image",
            Some("gif") => "GIF Image",
            Some("svg") => "SVG Image",
            Some("pdf") => "PDF Document",
            Some("mp4") => "MP4 Video",
            Some("mp3") => "MP3 Audio",
            Some("zip") => "ZIP Archive",
            Some("json") => "JSON",
            Some("toml") => "TOML",
            Some("yaml" | "yml") => "YAML",
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

    // Directories first, then natural sort
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| compare_ignore_case(&a.name, &b.name))
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
            SortField::Name => compare_ignore_case(&a.name, &b.name),
            SortField::Location => {
                let ap = a
                    .path
                    .parent()
                    .map(std::path::Path::as_os_str)
                    .unwrap_or_default();
                let bp = b
                    .path
                    .parent()
                    .map(std::path::Path::as_os_str)
                    .unwrap_or_default();
                ap.cmp(bp)
            }
            SortField::DateModified => a.mtime.cmp(&b.mtime),
            SortField::Size => a.size.cmp(&b.size),
            SortField::Kind => {
                let ak = a.extension.as_deref().unwrap_or("");
                let bk = b.extension.as_deref().unwrap_or("");
                ak.cmp(bk)
            }
            SortField::Permissions => a.mode.unwrap_or(0).cmp(&b.mode.unwrap_or(0)),
            SortField::Owner => {
                let au = a.owner.as_ref().map_or("", |o| o.user.as_str());
                let bu = b.owner.as_ref().map_or("", |o| o.user.as_str());
                au.cmp(bu)
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
#[allow(clippy::indexing_slicing)]
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

// -- Unix metadata extraction -------------------------------------------------

/// Extract mode, owner, and flags from metadata. Returns (mode, owner, flags).
#[cfg(unix)]
fn extract_unix_metadata(
    meta: &std::fs::Metadata,
) -> (Option<u32>, Option<OwnerInfo>, Option<String>) {
    use std::os::unix::fs::MetadataExt;

    let mode = Some(meta.mode());

    let owner = Some(resolve_owner(meta.uid(), meta.gid()));

    let flags = extract_bsd_flags(meta);

    (mode, owner, flags)
}

#[cfg(not(unix))]
fn extract_unix_metadata(
    _meta: &std::fs::Metadata,
) -> (Option<u32>, Option<OwnerInfo>, Option<String>) {
    (None, None, None)
}

/// Resolve uid/gid to user:group names via libc.
#[cfg(unix)]
#[allow(unsafe_code)]
fn resolve_owner(uid: u32, gid: u32) -> OwnerInfo {
    let user = {
        // SAFETY: getpwuid returns a pointer to a static struct or null.
        let pw = unsafe { libc::getpwuid(uid) };
        if pw.is_null() {
            uid.to_string()
        } else {
            // SAFETY: pw is non-null, checked above.
            let pw_name = unsafe { (*pw).pw_name };
            if pw_name.is_null() {
                uid.to_string()
            } else {
                // SAFETY: pw_name is non-null, checked above; points to a valid C string.
                unsafe { std::ffi::CStr::from_ptr(pw_name) }
                    .to_string_lossy()
                    .into_owned()
            }
        }
    };
    let group = {
        // SAFETY: getgrgid returns a pointer to a static struct or null.
        let gr = unsafe { libc::getgrgid(gid) };
        if gr.is_null() {
            gid.to_string()
        } else {
            // SAFETY: gr is non-null, checked above.
            let gr_name = unsafe { (*gr).gr_name };
            if gr_name.is_null() {
                gid.to_string()
            } else {
                // SAFETY: gr_name is non-null, checked above; points to a valid C string.
                unsafe { std::ffi::CStr::from_ptr(gr_name) }
                    .to_string_lossy()
                    .into_owned()
            }
        }
    };
    OwnerInfo { user, group }
}

/// Extract BSD file flags via fflagstostr (macOS/BSD).
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn extract_bsd_flags(meta: &std::fs::Metadata) -> Option<String> {
    use std::os::macos::fs::MetadataExt;

    unsafe extern "C" {
        fn fflagstostr(flags: libc::c_ulong) -> *mut libc::c_char;
    }

    /// RAII guard to free a C string allocated by malloc.
    struct MallocStr(*mut libc::c_char);
    impl Drop for MallocStr {
        fn drop(&mut self) {
            // SAFETY: pointer was returned by fflagstostr (malloc'd), only freed once here.
            unsafe { libc::free(self.0.cast()) }
        }
    }

    let st_flags = libc::c_ulong::from(meta.st_flags());
    if st_flags == 0 {
        return None;
    }

    // SAFETY: fflagstostr returns a malloc'd C string or null.
    let ptr = unsafe { fflagstostr(st_flags) };
    if ptr.is_null() {
        return None;
    }

    // Wrap in guard so free() runs even if CStr::from_ptr or to_string_lossy panics.
    let guard = MallocStr(ptr);

    // SAFETY: fflagstostr guarantees a null-terminated string.
    let result = unsafe { std::ffi::CStr::from_ptr(guard.0) }
        .to_string_lossy()
        .into_owned();

    // guard drops here, calling free()

    if result.is_empty() || result == "-" {
        None
    } else {
        Some(result)
    }
}

#[cfg(not(target_os = "macos"))]
fn extract_bsd_flags(_meta: &std::fs::Metadata) -> Option<String> {
    None
}

/// Check if a path has extended attributes (lightweight — no value retrieval).
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn has_extended_attrs(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: c_path is a valid null-terminated C string. listxattr with
    // size=0 returns the total size of xattr names, or -1 on error.
    let result = unsafe {
        libc::listxattr(
            c_path.as_ptr(),
            std::ptr::null_mut(),
            0,
            libc::XATTR_NOFOLLOW,
        )
    };
    // result is ssize_t: -1 = error, 0 = no xattrs, >0 = has xattrs
    result > 0
}

#[cfg(all(unix, not(target_os = "macos")))]
#[allow(unsafe_code)]
fn has_extended_attrs(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: c_path is a valid null-terminated C string.
    // listxattr returns ssize_t: -1 on error, 0 if none, >0 if present.
    let result = unsafe { libc::listxattr(c_path.as_ptr(), std::ptr::null_mut(), 0) };
    result > 0
}

#[cfg(not(unix))]
fn has_extended_attrs(_path: &Path) -> bool {
    false
}

/// Resolve a symlink target and check if it's valid.
fn resolve_symlink(path: &Path) -> SymlinkInfo {
    match std::fs::read_link(path) {
        Ok(target) => {
            // Check if target exists (resolve relative to parent)
            let absolute_target = if target.is_relative() {
                path.parent()
                    .map_or_else(|| target.clone(), |p| p.join(&target))
            } else {
                target.clone()
            };
            let display = target.to_string_lossy().into_owned();
            if absolute_target.exists() {
                SymlinkInfo::Valid(display)
            } else {
                SymlinkInfo::Broken(display)
            }
        }
        Err(_) => SymlinkInfo::Broken("?".to_string()),
    }
}

/// Format a Unix mode as `drwxr-xr-x` (10 characters).
#[allow(clippy::unreadable_literal)]
pub fn format_mode(mode: u32, is_dir: bool, is_symlink: bool) -> String {
    let mut s = String::with_capacity(10);

    // File type character
    if is_symlink {
        s.push('l');
    } else if is_dir {
        s.push('d');
    } else if mode & 0o170000 == 0o140000 {
        s.push('s'); // socket
    } else if mode & 0o170000 == 0o010000 {
        s.push('p'); // FIFO
    } else if mode & 0o170000 == 0o060000 {
        s.push('b'); // block device
    } else if mode & 0o170000 == 0o020000 {
        s.push('c'); // char device
    } else {
        s.push('-');
    }

    // User
    s.push(if mode & 0o400 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o200 != 0 { 'w' } else { '-' });
    s.push(match (mode & 0o100 != 0, mode & 0o4000 != 0) {
        (true, true) => 's',
        (false, true) => 'S',
        (true, false) => 'x',
        (false, false) => '-',
    });

    // Group
    s.push(if mode & 0o040 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o020 != 0 { 'w' } else { '-' });
    s.push(match (mode & 0o010 != 0, mode & 0o2000 != 0) {
        (true, true) => 's',
        (false, true) => 'S',
        (true, false) => 'x',
        (false, false) => '-',
    });

    // Other
    s.push(if mode & 0o004 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o002 != 0 { 'w' } else { '-' });
    s.push(match (mode & 0o001 != 0, mode & 0o1000 != 0) {
        (true, true) => 't',
        (false, true) => 'T',
        (true, false) => 'x',
        (false, false) => '-',
    });

    s
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
