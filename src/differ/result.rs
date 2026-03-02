//! Diff result types

use std::path::PathBuf;

/// Status of a file in the diff
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffStatus {
    /// File exists only in source (needs to be copied)
    Added,
    /// File exists only in destination (orphan, may need deletion)
    Removed,
    /// File exists in both but content differs (needs update)
    Modified,
    /// File exists in both and is identical (no action needed)
    Identical,
}

impl DiffStatus {
    /// Whether this status requires action during sync
    pub fn needs_action(&self) -> bool {
        matches!(self, Self::Added | Self::Modified | Self::Removed)
    }

    /// Whether this status requires a file transfer (copy)
    pub fn needs_transfer(&self) -> bool {
        matches!(self, Self::Added | Self::Modified)
    }

    /// Human-readable symbol for display
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Added => "+",
            Self::Removed => "-",
            Self::Modified => "~",
            Self::Identical => "=",
        }
    }

    /// Human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Modified => "modified",
            Self::Identical => "identical",
        }
    }
}

/// A single entry in the diff result
#[derive(Debug, Clone)]
pub struct DiffEntry {
    /// Relative path of the file
    pub path: PathBuf,
    /// Status of this file
    pub status: DiffStatus,
    /// Size in source (if present)
    pub source_size: Option<u64>,
    /// Size in destination (if present)
    pub dest_size: Option<u64>,
    /// Modification time in source (if present)
    pub source_mtime: Option<u64>,
    /// Modification time in destination (if present)
    pub dest_mtime: Option<u64>,
}

impl DiffEntry {
    /// Create a new DiffEntry for a file only in source
    pub fn added(path: PathBuf, size: u64, mtime: u64) -> Self {
        Self {
            path,
            status: DiffStatus::Added,
            source_size: Some(size),
            dest_size: None,
            source_mtime: Some(mtime),
            dest_mtime: None,
        }
    }

    /// Create a new DiffEntry for a file only in destination
    pub fn removed(path: PathBuf, size: u64, mtime: u64) -> Self {
        Self {
            path,
            status: DiffStatus::Removed,
            source_size: None,
            dest_size: Some(size),
            source_mtime: None,
            dest_mtime: Some(mtime),
        }
    }

    /// Create a new DiffEntry for a modified file
    pub fn modified(
        path: PathBuf,
        source_size: u64,
        source_mtime: u64,
        dest_size: u64,
        dest_mtime: u64,
    ) -> Self {
        Self {
            path,
            status: DiffStatus::Modified,
            source_size: Some(source_size),
            dest_size: Some(dest_size),
            source_mtime: Some(source_mtime),
            dest_mtime: Some(dest_mtime),
        }
    }

    /// Create a new DiffEntry for an identical file
    pub fn identical(path: PathBuf, size: u64, mtime: u64) -> Self {
        Self {
            path,
            status: DiffStatus::Identical,
            source_size: Some(size),
            dest_size: Some(size),
            source_mtime: Some(mtime),
            dest_mtime: Some(mtime),
        }
    }
}

/// Result of comparing two directory trees
#[must_use]
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// All diff entries
    pub entries: Vec<DiffEntry>,
    /// Number of files only in source
    pub added_count: usize,
    /// Number of files only in destination
    pub removed_count: usize,
    /// Number of files that differ
    pub modified_count: usize,
    /// Number of identical files
    pub identical_count: usize,
    /// Total bytes to transfer (added + modified source sizes)
    pub bytes_to_transfer: u64,
}

impl DiffResult {
    /// Create a new empty DiffResult
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            added_count: 0,
            removed_count: 0,
            modified_count: 0,
            identical_count: 0,
            bytes_to_transfer: 0,
        }
    }

    /// Create a DiffResult with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            added_count: 0,
            removed_count: 0,
            modified_count: 0,
            identical_count: 0,
            bytes_to_transfer: 0,
        }
    }

    /// Add an entry and update counts
    pub fn push(&mut self, entry: DiffEntry) {
        match entry.status {
            DiffStatus::Added => {
                self.added_count += 1;
                if let Some(size) = entry.source_size {
                    self.bytes_to_transfer += size;
                }
            }
            DiffStatus::Removed => {
                self.removed_count += 1;
            }
            DiffStatus::Modified => {
                self.modified_count += 1;
                if let Some(size) = entry.source_size {
                    self.bytes_to_transfer += size;
                }
            }
            DiffStatus::Identical => {
                self.identical_count += 1;
            }
        }
        self.entries.push(entry);
    }

    /// Total number of entries
    pub fn total_count(&self) -> usize {
        self.entries.len()
    }

    /// Number of entries that need action (not identical)
    pub fn changes_count(&self) -> usize {
        self.added_count + self.removed_count + self.modified_count
    }

    /// Whether the directories are identical
    pub fn is_identical(&self) -> bool {
        self.changes_count() == 0
    }

    /// Get entries filtered by status
    pub fn entries_by_status(&self, status: DiffStatus) -> impl Iterator<Item = &DiffEntry> {
        self.entries.iter().filter(move |e| e.status == status)
    }

    /// Get all entries that need action
    pub fn changes(&self) -> impl Iterator<Item = &DiffEntry> {
        self.entries.iter().filter(|e| e.status.needs_action())
    }
}

impl Default for DiffResult {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "result_test.rs"]
mod result_test;
