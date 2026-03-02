//! Etch-backed persistence for per-storage file caches
//!
//! WAL ops: collection 0, key = path (String), value = postcard-encoded CachedFileEntry.
//! Snapshot = full StorageState serialized with postcard.
//!
//! Secondary indexes (by_hash_xxh3, by_hash_blake3, by_size) are maintained
//! on insert/remove and rebuilt from the primary map on deserialization.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::UNIX_EPOCH;

use etchdb::{EtchKey, Op, Replayable, Store, Transactable, WalBackend};
use serde::{Deserialize, Serialize};

use crate::util::now_timestamp;

const FILES: u8 = 0;

/// A cached file entry stored in the etch state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFileEntry {
    /// Relative path from storage root
    pub path: String,
    /// File size in bytes
    pub size: i64,
    /// Modification time (unix timestamp)
    pub mtime: i64,
    /// XXH3 hash (16 bytes, 128-bit)
    pub hash_xxh3: Option<Vec<u8>>,
    /// Blake3 hash (32 bytes)
    pub hash_blake3: Option<Vec<u8>>,
    /// When this entry was indexed
    pub indexed_at: i64,
}

impl CachedFileEntry {
    /// Check if this entry is valid for the given size and mtime
    pub fn is_valid_for(&self, size: i64, mtime: i64) -> bool {
        self.size == size && self.mtime == mtime
    }

    /// Check if this entry has any hash
    pub fn has_hash(&self) -> bool {
        self.hash_xxh3.is_some() || self.hash_blake3.is_some()
    }

    /// Get XXH3 hash as hex string
    pub fn hash_xxh3_hex(&self) -> Option<String> {
        self.hash_xxh3.as_ref().map(hex::encode)
    }

    /// Get Blake3 hash as hex string
    pub fn hash_blake3_hex(&self) -> Option<String> {
        self.hash_blake3.as_ref().map(hex::encode)
    }

    /// Check if this cached entry is valid for filesystem metadata
    pub fn is_valid_for_metadata(&self, metadata: &std::fs::Metadata) -> bool {
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        self.is_valid_for(metadata.len() as i64, mtime)
    }
}

/// A file entry to be stored in the cache (input type for upserts)
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Relative path from storage root
    pub path: String,
    /// File size in bytes
    pub size: i64,
    /// Modification time (unix timestamp in seconds)
    pub mtime: i64,
    /// XXH3 hash (16 bytes, 128-bit)
    pub hash_xxh3: Option<Vec<u8>>,
    /// Blake3 hash (32 bytes, optional)
    pub hash_blake3: Option<Vec<u8>>,
}

impl CacheEntry {
    /// Create a new file entry without hashes
    pub fn new(path: impl Into<String>, size: i64, mtime: i64) -> Self {
        Self {
            path: path.into(),
            size,
            mtime,
            hash_xxh3: None,
            hash_blake3: None,
        }
    }

    /// Create a file entry with XXH3 hash
    pub fn with_xxh3(path: impl Into<String>, size: i64, mtime: i64, hash: Vec<u8>) -> Self {
        Self {
            path: path.into(),
            size,
            mtime,
            hash_xxh3: Some(hash),
            hash_blake3: None,
        }
    }

    /// Create a file entry with Blake3 hash
    pub fn with_blake3(path: impl Into<String>, size: i64, mtime: i64, hash: Vec<u8>) -> Self {
        Self {
            path: path.into(),
            size,
            mtime,
            hash_xxh3: None,
            hash_blake3: Some(hash),
        }
    }

    /// Create a file entry from filesystem metadata
    #[allow(dead_code)]
    pub fn from_metadata(path: impl Into<String>, metadata: &std::fs::Metadata) -> Self {
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        Self {
            path: path.into(),
            size: metadata.len() as i64,
            mtime,
            hash_xxh3: None,
            hash_blake3: None,
        }
    }
}

/// Statistics for a storage database
pub struct StorageStats {
    pub file_count: i64,
    pub total_bytes: i64,
    pub hashed_count: i64,
}

/// Per-storage etch state holding file cache entries with secondary indexes
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct StorageState {
    /// Primary map: path -> cached file entry
    files: BTreeMap<String, CachedFileEntry>,

    /// Secondary index: xxh3 hash -> paths
    #[serde(skip)]
    by_hash_xxh3: BTreeMap<Vec<u8>, Vec<String>>,

    /// Secondary index: blake3 hash -> paths
    #[serde(skip)]
    by_hash_blake3: BTreeMap<Vec<u8>, Vec<String>>,

    /// Secondary index: size -> paths
    #[serde(skip)]
    by_size: BTreeMap<i64, Vec<String>>,
}

impl StorageState {
    /// Rebuild secondary indexes from the primary map.
    /// Called after deserialization (serde skip rebuilds).
    fn rebuild_indexes(&mut self) {
        self.by_hash_xxh3.clear();
        self.by_hash_blake3.clear();
        self.by_size.clear();

        for (path, entry) in &self.files {
            if let Some(ref hash) = entry.hash_xxh3 {
                self.by_hash_xxh3
                    .entry(hash.clone())
                    .or_default()
                    .push(path.clone());
            }
            if let Some(ref hash) = entry.hash_blake3 {
                self.by_hash_blake3
                    .entry(hash.clone())
                    .or_default()
                    .push(path.clone());
            }
            self.by_size
                .entry(entry.size)
                .or_default()
                .push(path.clone());
        }
    }

    /// Insert or update a file entry, maintaining secondary indexes.
    pub fn upsert(&mut self, entry: CachedFileEntry) {
        let path = entry.path.clone();

        // Remove old indexes if updating
        if let Some(old) = self.files.get(&path).cloned() {
            self.remove_from_indexes(&path, &old);
        }

        // Add to secondary indexes
        if let Some(ref hash) = entry.hash_xxh3 {
            self.by_hash_xxh3
                .entry(hash.clone())
                .or_default()
                .push(path.clone());
        }
        if let Some(ref hash) = entry.hash_blake3 {
            self.by_hash_blake3
                .entry(hash.clone())
                .or_default()
                .push(path.clone());
        }
        self.by_size
            .entry(entry.size)
            .or_default()
            .push(path.clone());

        // Insert into primary map
        self.files.insert(path, entry);
    }

    /// Remove a file entry by path, maintaining secondary indexes.
    pub fn remove(&mut self, path: &str) -> Option<CachedFileEntry> {
        if let Some(entry) = self.files.remove(path) {
            self.remove_from_indexes(path, &entry);
            Some(entry)
        } else {
            None
        }
    }

    /// Remove old entry from secondary indexes
    fn remove_from_indexes(&mut self, path: &str, entry: &CachedFileEntry) {
        if let Some(ref hash) = entry.hash_xxh3 {
            if let Some(paths) = self.by_hash_xxh3.get_mut(hash) {
                paths.retain(|p| p != path);
                if paths.is_empty() {
                    self.by_hash_xxh3.remove(hash);
                }
            }
        }
        if let Some(ref hash) = entry.hash_blake3 {
            if let Some(paths) = self.by_hash_blake3.get_mut(hash) {
                paths.retain(|p| p != path);
                if paths.is_empty() {
                    self.by_hash_blake3.remove(hash);
                }
            }
        }
        if let Some(paths) = self.by_size.get_mut(&entry.size) {
            paths.retain(|p| p != path);
            if paths.is_empty() {
                self.by_size.remove(&entry.size);
            }
        }
    }

    /// Lookup a file by path
    pub fn get(&self, path: &str) -> Option<&CachedFileEntry> {
        self.files.get(path)
    }

    /// Lookup a file only if valid (size+mtime match)
    pub fn get_valid(&self, path: &str, size: i64, mtime: i64) -> Option<&CachedFileEntry> {
        self.files
            .get(path)
            .filter(|e| e.is_valid_for(size, mtime))
    }

    /// Find files by XXH3 hash
    pub fn find_by_hash_xxh3(&self, hash: &[u8]) -> Vec<&CachedFileEntry> {
        self.by_hash_xxh3
            .get(hash)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(|p| self.files.get(p))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find files by Blake3 hash
    pub fn find_by_hash_blake3(&self, hash: &[u8]) -> Vec<&CachedFileEntry> {
        self.by_hash_blake3
            .get(hash)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(|p| self.files.get(p))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find files by size
    pub fn find_by_size(&self, size: i64) -> Vec<&CachedFileEntry> {
        self.by_size
            .get(&size)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(|p| self.files.get(p))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Count all files
    pub fn file_count(&self) -> i64 {
        self.files.len() as i64
    }

    /// Get total bytes of all files
    pub fn total_bytes(&self) -> i64 {
        self.files.values().map(|e| e.size).sum()
    }

    /// Count files with at least one hash
    pub fn hashed_count(&self) -> i64 {
        self.files.values().filter(|e| e.has_hash()).count() as i64
    }

    /// List files (paginated, sorted by path)
    pub fn list(&self, limit: usize, offset: usize) -> Vec<&CachedFileEntry> {
        self.files.values().skip(offset).take(limit).collect()
    }

    /// Iterate all files
    pub fn iter(&self) -> impl Iterator<Item = (&String, &CachedFileEntry)> {
        self.files.iter()
    }

    /// Prune entries for files that no longer exist on disk
    pub fn prune_missing(&mut self, storage_path: &Path) -> Vec<String> {
        let missing: Vec<String> = self
            .files
            .keys()
            .filter(|path| !storage_path.join(path).exists())
            .cloned()
            .collect();

        for path in &missing {
            self.remove(path);
        }

        missing
    }
}

// Etch integration

impl Replayable for StorageState {
    fn apply(&mut self, ops: &[Op]) -> etchdb::Result<()> {
        for op in ops {
            match op {
                Op::Put { value, .. } => {
                    let entry: CachedFileEntry =
                        postcard::from_bytes(value).map_err(|e| etchdb::Error::WalCorrupted {
                            offset: 0,
                            reason: format!("bad cached file: {e}"),
                        })?;
                    self.upsert(entry);
                }
                Op::Delete { key, .. } => {
                    let path = String::from_bytes(key)?;
                    self.remove(&path);
                }
            }
        }
        Ok(())
    }

    fn after_load(&mut self) {
        self.rebuild_indexes();
    }
}

/// Transaction overlay for storage writes.
pub struct StorageOverlay {
    upserts: Vec<CachedFileEntry>,
    deletes: Vec<String>,
}

/// Transaction handle for storage operations.
pub struct StorageTx<'a> {
    pub state: &'a StorageState,
    ops: Vec<Op>,
    overlay: StorageOverlay,
}

impl<'a> StorageTx<'a> {
    /// Lookup a file (checks overlay first, then committed state).
    pub fn get(&self, path: &str) -> Option<&CachedFileEntry> {
        if self.overlay.deletes.iter().any(|p| p == path) {
            return None;
        }
        for entry in self.overlay.upserts.iter().rev() {
            if entry.path == path {
                return Some(entry);
            }
        }
        self.state.get(path)
    }

    /// Insert or update a file entry.
    pub fn put(&mut self, entry: &CachedFileEntry) {
        let value = postcard::to_allocvec(entry).expect("CachedFileEntry serialization");
        self.ops.push(Op::Put {
            collection: FILES,
            key: entry.path.to_bytes(),
            value,
        });
        self.overlay.upserts.push(entry.clone());
    }

    /// Delete a file entry by path.
    pub fn delete(&mut self, path: &str) {
        self.ops.push(Op::Delete {
            collection: FILES,
            key: path.to_string().to_bytes(),
        });
        self.overlay.deletes.push(path.to_string());
    }
}

impl Transactable for StorageState {
    type Tx<'a> = StorageTx<'a>;
    type Overlay = StorageOverlay;

    fn begin_tx(&self) -> StorageTx<'_> {
        StorageTx {
            state: self,
            ops: Vec::new(),
            overlay: StorageOverlay {
                upserts: Vec::new(),
                deletes: Vec::new(),
            },
        }
    }

    fn finish_tx(tx: StorageTx<'_>) -> (Vec<Op>, StorageOverlay) {
        (tx.ops, tx.overlay)
    }

    fn apply_overlay(&mut self, overlay: StorageOverlay) {
        for path in &overlay.deletes {
            self.remove(path);
        }
        for entry in overlay.upserts {
            self.upsert(entry);
        }
    }
}

/// Open an etch store for a storage directory.
pub fn open_storage_store(
    dir: &Path,
) -> Result<Store<StorageState, WalBackend<StorageState>>, std::io::Error> {
    Store::<StorageState, WalBackend<StorageState>>::open_wal(dir.to_path_buf())
        .map_err(|e| std::io::Error::other(e.to_string()))
}

/// Convert a CacheEntry to a CachedFileEntry (for etch storage).
pub fn cache_entry_to_file_entry(entry: &CacheEntry) -> CachedFileEntry {
    CachedFileEntry {
        path: entry.path.clone(),
        size: entry.size,
        mtime: entry.mtime,
        hash_xxh3: entry.hash_xxh3.clone(),
        hash_blake3: entry.hash_blake3.clone(),
        indexed_at: now_timestamp(),
    }
}
