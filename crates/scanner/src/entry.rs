//! File entry type for scanner results

use std::path::PathBuf;

/// Represents a single file discovered during scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Path relative to the scan root
    pub path: PathBuf,

    /// Absolute path to the file
    pub absolute_path: PathBuf,

    /// File size in bytes
    pub size: u64,

    /// Last modification time (Unix timestamp in seconds)
    pub mtime: u64,

    /// Optional blake3 hash (computed lazily, 32 bytes)
    pub hash: Option<[u8; 32]>,

    /// Optional xxh3 hash (computed lazily, 16 bytes)
    pub hash_xxh3: Option<[u8; 16]>,
}

impl FileEntry {
    /// Create a new `FileEntry`
    pub fn new(path: PathBuf, absolute_path: PathBuf, size: u64, mtime: u64) -> Self {
        Self {
            path,
            absolute_path,
            size,
            mtime,
            hash: None,
            hash_xxh3: None,
        }
    }

    /// Set the blake3 hash
    pub fn with_hash(mut self, hash: [u8; 32]) -> Self {
        self.hash = Some(hash);
        self
    }

    /// Set the xxh3 hash
    pub fn with_hash_xxh3(mut self, hash: [u8; 16]) -> Self {
        self.hash_xxh3 = Some(hash);
        self
    }

    /// Check if this entry matches another by size and mtime (fast comparison)
    pub fn matches_metadata(&self, other: &FileEntry) -> bool {
        self.size == other.size && self.mtime == other.mtime
    }

    /// Check if this entry matches another by hash (requires hash to be computed)
    pub fn matches_hash(&self, other: &FileEntry) -> Option<bool> {
        match (&self.hash, &other.hash) {
            (Some(a), Some(b)) => Some(a == b),
            _ => None,
        }
    }
}

impl Ord for FileEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.path.cmp(&other.path)
    }
}

impl PartialOrd for FileEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
#[path = "entry_test.rs"]
mod entry_test;
