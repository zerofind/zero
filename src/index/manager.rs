//! Index Manager - manages multiple per-root indexes with parallel search
//!
//! This module provides a clean architecture where:
//! - Each indexed root has its own separate index file
//! - Removing a root is instant (just drop from memory + delete file)
//! - Search runs in parallel across all indexes
//!
//! ## Architecture
//!
//! ```text
//! ~/.zero/indexes/
//!   a1b2c3/                 # etch WAL dir for /Users/foo
//!   d4e5f6/                 # etch WAL dir for /Volumes/External
//! ```
//!
//! Root registry is stored in ControlDb (etch-backed) rather than a manifest file.
//!
//! ## Usage
//!
//! ```no_run
//! use zero::index::IndexManager;
//!
//! let mut manager = IndexManager::load().unwrap();
//!
//! // Add a root (builds index in background)
//! manager.add_root("/Users/me/Documents").unwrap();
//!
//! // Search across all indexes in parallel
//! let results = manager.search("report", 100);
//!
//! // Remove a root (instant!)
//! manager.remove_root("/Users/me/Documents");
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use rayon::prelude::*;
use sha2::{Digest, Sha256};

use crate::cache::{ControlDb, IndexedRoot};
use crate::scanner::CrawlProgress;

use super::etch::{open_index_store, save_index_via_etch};
use super::node::FileNode;
use super::search::{IndexError, SearchIndex, SearchOptions, SearchQuery, SearchResult, SortBy};
use super::type_index::TypeIndexStats;

/// Statistics for a single indexed root
#[derive(Debug, Clone)]
pub struct RootStats {
    /// The root path
    pub path: String,
    /// Number of files
    pub file_count: usize,
    /// Number of directories
    pub dir_count: usize,
    /// Total bytes
    pub total_bytes: u64,
    /// When last indexed (Unix timestamp)
    pub last_indexed: u64,
}

/// Manager for multiple per-root search indexes
///
/// Provides:
/// - Instant root removal (O(1) instead of O(n))
/// - Parallel search across all indexes
/// - Clean separation of data per root
///
/// Root registry is persisted in ControlDb. An in-memory cache avoids
/// repeated reads for hot-path lookups (has_root, stats, etc.).
pub struct IndexManager {
    /// Directory where index files are stored
    indexes_dir: PathBuf,

    /// Loaded indexes by root path
    indexes: HashMap<String, SearchIndex>,

    /// In-memory cache of indexed root metadata (synced with ControlDb)
    roots_cache: HashMap<String, IndexedRoot>,

    /// ControlDb for persistent root registry
    control_db: ControlDb,
}

impl Clone for IndexManager {
    fn clone(&self) -> Self {
        // ControlDb is not Clone — open a fresh one at the same path
        let control_db = ControlDb::open()
            .unwrap_or_else(|_| ControlDb::open_memory().expect("fallback memory db"));
        Self {
            indexes_dir: self.indexes_dir.clone(),
            indexes: self.indexes.clone(),
            roots_cache: self.roots_cache.clone(),
            control_db,
        }
    }
}

impl IndexManager {
    /// Get the indexes directory path
    pub fn indexes_dir(&self) -> &Path {
        &self.indexes_dir
    }

    /// Insert a pre-built index into the manager
    /// Used by async indexing to avoid holding write lock during crawl
    pub fn insert_loaded_index(&mut self, root: &str, index: SearchIndex, file_count: usize) {
        let dir_count = index.dir_count();
        let total_bytes = index.total_bytes();
        let hash = hash_path(root);

        // Save to disk via etch (ignore errors - index is still usable)
        let etch_dir = self.indexes_dir.join(&hash);
        let _ = save_index_via_etch(&index, &etch_dir);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let indexed_root = IndexedRoot {
            hash,
            file_count,
            dir_count,
            total_bytes,
            last_indexed: now,
        };

        // Persist to ControlDb (ignore errors - index is still usable)
        let _ = self.control_db.upsert_indexed_root(root, &indexed_root);

        self.roots_cache.insert(root.to_string(), indexed_root);
        self.indexes.insert(root.to_string(), index);
    }

    /// Insert a pre-built index without disk I/O (caller persisted it already).
    ///
    /// Use this when the index was built and saved on a background thread,
    /// then passed back to the main thread for in-memory registration.
    pub fn insert_index_memory_only(&mut self, root: &str, index: SearchIndex, file_count: usize) {
        let dir_count = index.dir_count();
        let total_bytes = index.total_bytes();
        let hash = hash_path(root);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let indexed_root = IndexedRoot {
            hash,
            file_count,
            dir_count,
            total_bytes,
            last_indexed: now,
        };

        let _ = self.control_db.upsert_indexed_root(root, &indexed_root);
        self.roots_cache.insert(root.to_string(), indexed_root);
        self.indexes.insert(root.to_string(), index);
    }

    /// Create a new IndexManager with the default indexes directory
    pub fn new() -> Result<Self, IndexError> {
        let indexes_dir = default_indexes_dir().ok_or_else(|| {
            IndexError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not determine cache directory",
            ))
        })?;

        let control_db =
            ControlDb::open().map_err(|e| IndexError::Io(std::io::Error::other(e.to_string())))?;

        Self::with_dir_and_db(indexes_dir, control_db)
    }

    /// Create a new IndexManager with a custom directory (uses in-memory ControlDb for testing)
    pub fn with_dir(indexes_dir: PathBuf) -> Result<Self, IndexError> {
        let control_db = ControlDb::open_memory()
            .map_err(|e| IndexError::Io(std::io::Error::other(e.to_string())))?;

        Self::with_dir_and_db(indexes_dir, control_db)
    }

    /// Create a new IndexManager with a custom directory and ControlDb
    ///
    /// Eagerly loads root metadata from ControlDb so that file counts are
    /// available immediately, even before the full indexes are loaded.
    pub fn with_dir_and_db(
        indexes_dir: PathBuf,
        control_db: ControlDb,
    ) -> Result<Self, IndexError> {
        fs::create_dir_all(&indexes_dir)?;

        let roots_cache: HashMap<String, IndexedRoot> = control_db
            .list_indexed_roots()
            .unwrap_or_default()
            .into_iter()
            .collect();

        Ok(Self {
            indexes_dir,
            indexes: HashMap::new(),
            roots_cache,
            control_db,
        })
    }

    /// Load the IndexManager from disk, including all indexed roots
    pub fn load() -> Result<Self, IndexError> {
        let indexes_dir = default_indexes_dir().ok_or_else(|| {
            IndexError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not determine cache directory",
            ))
        })?;

        Self::load_from(indexes_dir)
    }

    /// Load the IndexManager from a specific directory
    pub fn load_from(indexes_dir: PathBuf) -> Result<Self, IndexError> {
        let control_db =
            ControlDb::open().map_err(|e| IndexError::Io(std::io::Error::other(e.to_string())))?;

        Self::load_from_with_db(indexes_dir, control_db)
    }

    /// Load the IndexManager from a specific directory with a given ControlDb
    pub fn load_from_with_db(
        indexes_dir: PathBuf,
        control_db: ControlDb,
    ) -> Result<Self, IndexError> {
        fs::create_dir_all(&indexes_dir)?;

        // Load roots from ControlDb
        let db_roots = control_db
            .list_indexed_roots()
            .map_err(|e| IndexError::Io(std::io::Error::other(e.to_string())))?;

        let roots_cache: HashMap<String, IndexedRoot> = db_roots.into_iter().collect();

        let mut manager = Self {
            indexes_dir,
            indexes: HashMap::new(),
            roots_cache,
            control_db,
        };

        // Load all indexes from disk
        manager.load_all_indexes()?;

        Ok(manager)
    }

    /// Load all indexes listed in the roots cache
    fn load_all_indexes(&mut self) -> Result<(), IndexError> {
        let roots: Vec<_> = self.roots_cache.keys().cloned().collect();

        for root in roots {
            if let Err(e) = self.load_index(&root) {
                tracing::warn!(root = %root, error = %e, "failed to load index");
                // Remove from cache + ControlDb if index file is missing/corrupt
                self.roots_cache.remove(&root);
                let _ = self.control_db.remove_indexed_root(&root);
            }
        }

        Ok(())
    }

    /// Load a single index from disk (etch directory)
    fn load_index(&mut self, root: &str) -> Result<(), IndexError> {
        let entry = self
            .roots_cache
            .get(root)
            .ok_or_else(|| IndexError::Serialize(format!("Root not in registry: {}", root)))?;

        let etch_dir = self.indexes_dir.join(&entry.hash);
        if !etch_dir.is_dir() {
            return Err(IndexError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Index not found: {:?}", etch_dir),
            )));
        }

        let store = open_index_store(&etch_dir)?;
        let index = store.read().clone();
        self.indexes.insert(root.to_string(), index);

        Ok(())
    }

    /// Number of loaded in-memory indexes.
    pub fn indexes_count(&self) -> usize {
        self.indexes.len()
    }

    /// Number of roots registered in the cache (may exceed indexes_count
    /// while indexes are still loading).
    pub fn roots_count(&self) -> usize {
        self.roots_cache.len()
    }

    /// Remove a stale root from the cache and ControlDb without deleting
    /// the etch directory on disk (the index file may just need a rebuild).
    pub fn remove_stale_root(&mut self, root: &str) {
        self.indexes.remove(root);
        self.roots_cache.remove(root);
        let _ = self.control_db.remove_indexed_root(root);
    }

    /// Provide mutable access to a loaded index for in-place operations
    /// like chunked insertion.
    pub fn with_index_mut<R>(
        &mut self,
        root: &str,
        f: impl FnOnce(&mut SearchIndex) -> R,
    ) -> Option<R> {
        self.indexes.get_mut(root).map(f)
    }

    /// Provide read access to a loaded index.
    pub fn with_index<R>(&self, root: &str, f: impl FnOnce(&SearchIndex) -> R) -> Option<R> {
        self.indexes.get(root).map(f)
    }

    /// Save a single root's index to disk via etch store
    #[allow(dead_code)]
    fn save_root_index(&self, root: &str) -> Result<(), IndexError> {
        let index = self
            .indexes
            .get(root)
            .ok_or_else(|| IndexError::Serialize(format!("Root not loaded: {}", root)))?;

        let hash = hash_path(root);
        let etch_dir = self.indexes_dir.join(&hash);

        save_index_via_etch(index, &etch_dir)?;

        Ok(())
    }

    /// Add a new root to the index manager
    ///
    /// This builds the index for the root and saves it to disk.
    /// Returns the number of files indexed.
    pub fn add_root(&mut self, root: &str) -> Result<usize, IndexError> {
        self.add_root_with_progress(root, None)
    }

    /// Add a new root with optional progress tracking
    ///
    /// Same as `add_root` but passes progress through to the crawler
    /// so callers can poll file/byte counts in real time.
    pub fn add_root_with_progress(
        &mut self,
        root: &str,
        progress: Option<Arc<CrawlProgress>>,
    ) -> Result<usize, IndexError> {
        let root_path = Path::new(root);
        if !root_path.exists() {
            return Err(IndexError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Path does not exist: {}", root),
            )));
        }

        // Build the index
        let mut index = SearchIndex::new();
        index.build_from_path_with_progress(root_path, progress)?;

        let file_count = index.file_count();
        let dir_count = index.dir_count();
        let total_bytes = index.total_bytes();

        // Generate hash for filename
        let hash = hash_path(root);

        // Save to disk via etch store
        let etch_dir = self.indexes_dir.join(&hash);
        save_index_via_etch(&index, &etch_dir)?;

        // Update ControlDb
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let indexed_root = IndexedRoot {
            hash,
            file_count,
            dir_count,
            total_bytes,
            last_indexed: now,
        };

        self.control_db
            .upsert_indexed_root(root, &indexed_root)
            .map_err(|e| IndexError::Io(std::io::Error::other(e.to_string())))?;

        self.roots_cache.insert(root.to_string(), indexed_root);
        self.indexes.insert(root.to_string(), index);

        Ok(file_count)
    }

    /// Remove a root from the index manager
    ///
    /// This is instant - just drops from memory and deletes the file.
    /// Returns the number of files that were in the index.
    pub fn remove_root(&mut self, root: &str) -> usize {
        // Get file count before removing
        let file_count = self
            .indexes
            .get(root)
            .map(|idx| idx.file_count())
            .unwrap_or(0);

        // Remove from memory (instant)
        self.indexes.remove(root);

        // Get hash, remove from cache + ControlDb, delete index dir
        if let Some(entry) = self.roots_cache.remove(root) {
            let etch_dir = self.indexes_dir.join(&entry.hash);
            let _ = fs::remove_dir_all(&etch_dir);
        }
        let _ = self.control_db.remove_indexed_root(root);

        file_count
    }

    /// Check if a root is already indexed
    pub fn has_root(&self, root: &str) -> bool {
        self.roots_cache.contains_key(root)
    }

    /// Get all indexed root paths
    pub fn roots(&self) -> Vec<String> {
        self.roots_cache.keys().cloned().collect()
    }

    /// Get statistics for all indexed roots
    pub fn stats(&self) -> Vec<RootStats> {
        self.roots_cache
            .iter()
            .map(|(path, entry)| RootStats {
                path: path.clone(),
                file_count: entry.file_count,
                dir_count: entry.dir_count,
                total_bytes: entry.total_bytes,
                last_indexed: entry.last_indexed,
            })
            .collect()
    }

    /// Get statistics for a specific root
    pub fn root_stats(&self, root: &str) -> Option<RootStats> {
        self.roots_cache.get(root).map(|entry| RootStats {
            path: root.to_string(),
            file_count: entry.file_count,
            dir_count: entry.dir_count,
            total_bytes: entry.total_bytes,
            last_indexed: entry.last_indexed,
        })
    }

    /// Get total file count across all indexes
    ///
    /// Uses in-memory indexes when available, falls back to persisted metadata
    /// in roots_cache for roots that haven't been loaded yet.
    pub fn total_file_count(&self) -> usize {
        self.roots_cache
            .iter()
            .map(|(root, cached)| {
                self.indexes
                    .get(root)
                    .map(|idx| idx.file_count())
                    .unwrap_or(cached.file_count)
            })
            .sum()
    }

    /// Get total directory count across all indexes
    pub fn total_dir_count(&self) -> usize {
        self.roots_cache
            .iter()
            .map(|(root, cached)| {
                self.indexes
                    .get(root)
                    .map(|idx| idx.dir_count())
                    .unwrap_or(cached.dir_count)
            })
            .sum()
    }

    /// Get total bytes across all indexes
    pub fn total_bytes(&self) -> u64 {
        self.roots_cache
            .iter()
            .map(|(root, cached)| {
                self.indexes
                    .get(root)
                    .map(|idx| idx.total_bytes())
                    .unwrap_or(cached.total_bytes)
            })
            .sum()
    }

    /// Iterate over all files in all indexes
    /// Returns an iterator of FileNode references
    pub fn iter_all(&self) -> impl Iterator<Item = &FileNode> {
        self.indexes.values().flat_map(|idx| idx.iter())
    }

    /// Get all files from all indexes as SearchResults
    /// This is useful for cleanup queries that need to scan all files
    pub fn all_files(&self) -> Vec<SearchResult> {
        self.indexes
            .values()
            .flat_map(|idx| {
                idx.iter().map(|node| SearchResult {
                    node: node.clone(),
                    score: 0,
                })
            })
            .collect()
    }

    /// Search across all indexes in parallel
    ///
    /// Results are merged and sorted by score, returning the top `limit` results.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        self.search_with_options(query, SearchOptions::with_limit(limit))
    }

    /// Search with custom options across all indexes in parallel
    /// If options.limit is 0, returns all results (unlimited)
    pub fn search_with_options(&self, query: &str, options: SearchOptions) -> Vec<SearchResult> {
        self.search_filtered(query, options, None)
    }

    /// Search with filtering by roots
    ///
    /// # Arguments
    /// * `query` - Search query
    /// * `options` - Search options (includes include_trash)
    /// * `roots` - Optional list of roots to search. If None, searches all roots.
    ///   If Some, only searches indexes whose root starts with one of the given paths.
    pub fn search_filtered(
        &self,
        query: &str,
        options: SearchOptions,
        roots: Option<&[String]>,
    ) -> Vec<SearchResult> {
        if self.indexes.is_empty() {
            return Vec::new();
        }

        // Use usize::MAX for unlimited (when limit is 0)
        let effective_limit = if options.limit == 0 {
            usize::MAX
        } else {
            options.limit
        };
        let mut effective_options = options.clone();
        effective_options.limit = effective_limit;

        // Filter indexes by roots if specified
        let indexes_to_search: Vec<_> = match roots {
            Some(filter_roots) => self
                .indexes
                .iter()
                .filter(|(root, _)| {
                    filter_roots
                        .iter()
                        .any(|r| root.starts_with(r) || r.starts_with(*root))
                })
                .collect(),
            None => self.indexes.iter().collect(),
        };

        // Parallel search across selected indexes
        let mut all_results: Vec<SearchResult> = indexes_to_search
            .par_iter()
            .flat_map(|(_, index)| index.search_with_options(query, effective_options.clone()))
            .collect();

        // Sort by score (descending)
        all_results.sort_by_key(|a| std::cmp::Reverse(a.score));

        // Truncate to limit (only if limit > 0)
        if options.limit > 0 {
            all_results.truncate(options.limit);
        }

        all_results
    }

    /// Unified search across all indexes in parallel.
    ///
    /// This is the preferred entry point. Dispatches to per-root SearchIndex::query()
    /// and merges results.
    pub fn query(&self, q: SearchQuery) -> Vec<SearchResult> {
        if self.indexes.is_empty() {
            return Vec::new();
        }

        let limit = q.limit;
        let sort = q.sort;

        let mut all_results: Vec<SearchResult> = self
            .indexes
            .par_iter()
            .flat_map(|(_, index)| index.query(q.clone()))
            .collect();

        // Sort based on query's sort order
        match sort {
            SortBy::RecentFirst => {
                all_results.sort_by_key(|a| std::cmp::Reverse(a.node.mtime));
            }
            SortBy::Relevance => {
                all_results.sort_by_key(|a| std::cmp::Reverse(a.score));
            }
            SortBy::None => {
                // Type-only results: sort by mtime for consistent ordering
                all_results.sort_by_key(|a| std::cmp::Reverse(a.node.mtime));
            }
        }

        if limit > 0 {
            all_results.truncate(limit);
        }

        all_results
    }

    /// Search by file type across all indexes in parallel
    /// If limit is 0, returns all results (unlimited)
    pub fn search_by_type(&self, type_filter: &str, limit: usize) -> Vec<SearchResult> {
        self.search_by_type_filtered(type_filter, limit, true, None)
    }

    /// Search by file type with filtering
    ///
    /// # Arguments
    /// * `type_filter` - Type category name (e.g., "images", "videos")
    /// * `limit` - Maximum results (0 = unlimited)
    /// * `include_trash` - Whether to include files in Trash
    /// * `roots` - Optional list of roots to search
    pub fn search_by_type_filtered(
        &self,
        type_filter: &str,
        limit: usize,
        include_trash: bool,
        roots: Option<&[String]>,
    ) -> Vec<SearchResult> {
        if self.indexes.is_empty() {
            return Vec::new();
        }

        // Use usize::MAX for unlimited (when limit is 0)
        let effective_limit = if limit == 0 { usize::MAX } else { limit };

        // Filter indexes by roots if specified
        let indexes_to_search: Vec<_> = match roots {
            Some(filter_roots) => self
                .indexes
                .iter()
                .filter(|(root, _)| {
                    filter_roots
                        .iter()
                        .any(|r| root.starts_with(r) || r.starts_with(*root))
                })
                .collect(),
            None => self.indexes.iter().collect(),
        };

        // Parallel search across selected indexes
        let mut all_results: Vec<SearchResult> = indexes_to_search
            .par_iter()
            .flat_map(|(_, index)| {
                index.search_by_type_filtered(type_filter, effective_limit, include_trash)
            })
            .collect();

        // Sort by mtime (most recent first)
        all_results.sort_by_key(|a| std::cmp::Reverse(a.node.mtime));

        // Truncate to limit (only if limit > 0)
        if limit > 0 {
            all_results.truncate(limit);
        }

        all_results
    }

    /// Combined search: text query + type filter across all indexes in parallel
    ///
    /// This allows searching for items with a specific name AND type,
    /// e.g., searching for all folders named "zero".
    ///
    /// If limit is 0, returns all results (unlimited)
    pub fn search_with_type(
        &self,
        query: &str,
        type_filter: &str,
        limit: usize,
    ) -> Vec<SearchResult> {
        self.search_with_type_filtered(query, type_filter, limit, true, None)
    }

    /// Combined search with filtering
    ///
    /// # Arguments
    /// * `query` - Text query
    /// * `type_filter` - Type category name
    /// * `limit` - Maximum results (0 = unlimited)
    /// * `include_trash` - Whether to include files in Trash
    /// * `roots` - Optional list of roots to search
    pub fn search_with_type_filtered(
        &self,
        query: &str,
        type_filter: &str,
        limit: usize,
        include_trash: bool,
        roots: Option<&[String]>,
    ) -> Vec<SearchResult> {
        if self.indexes.is_empty() {
            return Vec::new();
        }

        // Use usize::MAX for unlimited (when limit is 0)
        let effective_limit = if limit == 0 { usize::MAX } else { limit };

        // Filter indexes by roots if specified
        let indexes_to_search: Vec<_> = match roots {
            Some(filter_roots) => self
                .indexes
                .iter()
                .filter(|(root, _)| {
                    filter_roots
                        .iter()
                        .any(|r| root.starts_with(r) || r.starts_with(*root))
                })
                .collect(),
            None => self.indexes.iter().collect(),
        };

        // Parallel search across selected indexes using combined query + type
        let mut all_results: Vec<SearchResult> = indexes_to_search
            .par_iter()
            .flat_map(|(_, index)| {
                index.search_with_type_filtered(query, type_filter, effective_limit, include_trash)
            })
            .collect();

        // Sort by score (descending) for query-based search
        all_results.sort_by_key(|a| std::cmp::Reverse(a.score));

        // Truncate to limit (only if limit > 0)
        if limit > 0 {
            all_results.truncate(limit);
        }

        all_results
    }

    /// Get count of files matching a type filter
    pub fn type_count(&self, type_filter: &str) -> u64 {
        use super::type_index::FileTypeCategory;

        let Some(category) = FileTypeCategory::parse_str(type_filter) else {
            return 0;
        };

        self.indexes
            .values()
            .map(|idx| idx.type_index().count(category))
            .sum()
    }

    /// Get type index statistics across all indexes
    pub fn type_stats(&self) -> TypeIndexStats {
        let mut combined = TypeIndexStats {
            total_files: 0,
            images: 0,
            videos: 0,
            audio: 0,
            documents: 0,
            code: 0,
            archives: 0,
            config: 0,
            executables: 0,
            fonts: 0,
            files: 0,
            directories: 0,
            trash: 0,
            memory_bytes: 0,
        };

        for index in self.indexes.values() {
            let stats = index.type_index().stats();
            combined.total_files += stats.total_files;
            combined.images += stats.images;
            combined.videos += stats.videos;
            combined.audio += stats.audio;
            combined.documents += stats.documents;
            combined.code += stats.code;
            combined.archives += stats.archives;
            combined.config += stats.config;
            combined.executables += stats.executables;
            combined.fonts += stats.fonts;
            combined.files += stats.files;
            combined.directories += stats.directories;
            combined.trash += stats.trash;
            combined.memory_bytes += stats.memory_bytes;
        }

        combined
    }

    /// Search by file extension using O(1) bitmap lookup
    ///
    /// This is much faster than text search for extension-based queries.
    /// Extension should be without the dot (e.g., "log", "dmg", "rs").
    ///
    /// Returns files matching the extension across all indexes.
    pub fn search_by_extension(&self, ext: &str, limit: usize) -> Vec<SearchResult> {
        self.search_by_extension_filtered(ext, limit, false)
    }

    /// Search by file extension with trash filtering option
    ///
    /// This uses O(1) bitmap lookup for the extension, then resolves to FileNodes.
    pub fn search_by_extension_filtered(
        &self,
        ext: &str,
        limit: usize,
        include_trash: bool,
    ) -> Vec<SearchResult> {
        if self.indexes.is_empty() {
            return Vec::new();
        }

        let ext_lower = ext.to_lowercase();

        // Collect results from all indexes in parallel
        let all_results: Vec<SearchResult> = self
            .indexes
            .par_iter()
            .flat_map(|(_, index)| {
                let type_index = index.type_index();

                // Get the extension bitmap
                let Some(ext_bitmap) = type_index.get_by_extension(&ext_lower) else {
                    return Vec::new();
                };

                // Filter out trash if requested
                let indices_iter: Box<dyn Iterator<Item = u32> + Send> = if include_trash {
                    Box::new(ext_bitmap.iter())
                } else {
                    let filtered = ext_bitmap.clone() - type_index.trash_bitmap();
                    Box::new(filtered.into_iter())
                };

                // Convert indices to SearchResults
                indices_iter
                    .take(limit)
                    .filter_map(|idx| {
                        index.get(idx as usize).map(|node| SearchResult {
                            node: node.clone(),
                            score: 100, // Extension match score
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        // Sort by path for consistency
        let mut results = all_results;
        results.sort_by(|a, b| a.node.path.cmp(&b.node.path));

        // Apply limit
        if limit > 0 && results.len() > limit {
            results.truncate(limit);
        }

        results
    }

    /// Get count of files with a specific extension
    pub fn extension_count(&self, ext: &str) -> u64 {
        let ext_lower = ext.to_lowercase();
        self.indexes
            .values()
            .map(|idx| idx.type_index().count_by_extension(&ext_lower))
            .sum()
    }

    /// Search by path component (directory name) using O(1) bitmap lookup
    ///
    /// This is much faster than text search for folder-based queries like `**/node_modules`.
    /// Component should be the directory name (e.g., "node_modules", "target", ".Trash").
    ///
    /// Returns files whose path contains the specified directory component.
    pub fn search_by_path_component(&self, component: &str, limit: usize) -> Vec<SearchResult> {
        self.search_by_path_component_filtered(component, limit, false)
    }

    /// Search by path component with trash filtering option
    ///
    /// This uses O(1) bitmap lookup for the path component, then resolves to FileNodes.
    pub fn search_by_path_component_filtered(
        &self,
        component: &str,
        limit: usize,
        include_trash: bool,
    ) -> Vec<SearchResult> {
        if self.indexes.is_empty() {
            return Vec::new();
        }

        let component_lower = component.to_lowercase();

        // Collect results from all indexes in parallel
        let all_results: Vec<SearchResult> = self
            .indexes
            .par_iter()
            .flat_map(|(_, index)| {
                let type_index = index.type_index();

                // Get the path component bitmap
                let Some(component_bitmap) = type_index.get_by_path_component(&component_lower)
                else {
                    return Vec::new();
                };

                // Filter out trash if requested
                let indices_iter: Box<dyn Iterator<Item = u32> + Send> = if include_trash {
                    Box::new(component_bitmap.iter())
                } else {
                    let filtered = component_bitmap.clone() - type_index.trash_bitmap();
                    Box::new(filtered.into_iter())
                };

                // Convert indices to SearchResults
                indices_iter
                    .take(limit)
                    .filter_map(|idx| {
                        index.get(idx as usize).map(|node| SearchResult {
                            node: node.clone(),
                            score: 100, // Path component match score
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        // Sort by path for consistency
        let mut results = all_results;
        results.sort_by(|a, b| a.node.path.cmp(&b.node.path));

        // Apply limit
        if limit > 0 && results.len() > limit {
            results.truncate(limit);
        }

        results
    }

    /// Get count of files with a specific path component
    pub fn path_component_count(&self, component: &str) -> u64 {
        let component_lower = component.to_lowercase();
        self.indexes
            .values()
            .map(|idx| idx.type_index().count_by_path_component(&component_lower))
            .sum()
    }

    /// Get the most recently modified files across all indexes
    pub fn recent_files(&self, limit: usize) -> Vec<SearchResult> {
        if self.indexes.is_empty() {
            return Vec::new();
        }

        // Get recent files from each index in parallel using search_by_type
        // We use "files" type to get all files, then sort by mtime
        let mut all_results: Vec<SearchResult> = self
            .indexes
            .par_iter()
            .flat_map(|(_, index)| {
                // Get all files and sort by mtime within each index
                let mut results = index.search_by_type("files", limit);
                results.sort_by_key(|a| std::cmp::Reverse(a.node.mtime));
                results.truncate(limit);
                results
            })
            .collect();

        // Sort by mtime (most recent first)
        all_results.sort_by_key(|a| std::cmp::Reverse(a.node.mtime));

        // Truncate to limit
        all_results.truncate(limit);

        all_results
    }

    /// Clear all indexes
    pub fn clear(&mut self) {
        // Remove all index files/directories
        for entry in self.roots_cache.values() {
            let etch_dir = self.indexes_dir.join(&entry.hash);
            let _ = fs::remove_dir_all(&etch_dir);
        }

        // Remove all roots from ControlDb
        let root_paths: Vec<_> = self.roots_cache.keys().cloned().collect();
        for path in &root_paths {
            let _ = self.control_db.remove_indexed_root(path);
        }

        // Clear in-memory state
        self.indexes.clear();
        self.roots_cache.clear();
    }

    /// Rebuild index for a specific root
    ///
    /// This removes the old index and builds a fresh one.
    pub fn rebuild_root(&mut self, root: &str) -> Result<usize, IndexError> {
        // Remove old index
        self.remove_root(root);

        // Build fresh
        self.add_root(root)
    }

    /// Rebuild all indexes
    pub fn rebuild_all(&mut self) -> Result<usize, IndexError> {
        self.rebuild_all_with_progress(None)
    }

    /// Rebuild all indexes with optional progress tracking
    ///
    /// The same `CrawlProgress` is shared across all roots, so
    /// counters accumulate across the entire rebuild.
    pub fn rebuild_all_with_progress(
        &mut self,
        progress: Option<Arc<CrawlProgress>>,
    ) -> Result<usize, IndexError> {
        let roots: Vec<_> = self.roots();
        let mut total_files = 0;

        for root in roots {
            // Remove old index first
            self.remove_root(&root);

            match self.add_root_with_progress(&root, progress.clone()) {
                Ok(count) => total_files += count,
                Err(e) => eprintln!("Warning: Failed to rebuild index for {}: {}", root, e),
            }
        }

        Ok(total_files)
    }

    /// Get a reference to a specific index
    pub fn get_index(&self, root: &str) -> Option<&SearchIndex> {
        self.indexes.get(root)
    }

    /// Get a mutable reference to a specific index
    /// Used by the file watcher to update indexes in real-time
    pub fn get_index_mut(&mut self, root: &str) -> Option<&mut SearchIndex> {
        self.indexes.get_mut(root)
    }

    /// Iterate over all loaded indexes
    /// Used for cross-index operations like todo search
    pub fn indexes(&self) -> impl Iterator<Item = &SearchIndex> {
        self.indexes.values()
    }
}

/// Thread-safe wrapper around IndexManager
pub struct SharedIndexManager {
    inner: Arc<RwLock<IndexManager>>,
}

impl SharedIndexManager {
    pub fn new(manager: IndexManager) -> Self {
        Self {
            inner: Arc::new(RwLock::new(manager)),
        }
    }

    pub fn load() -> Result<Self, IndexError> {
        Ok(Self::new(IndexManager::load()?))
    }

    /// Unified search (read lock)
    pub fn query(&self, q: SearchQuery) -> Vec<SearchResult> {
        let guard = self.inner.read().unwrap();
        guard.query(q)
    }

    /// Search across all indexes (read lock)
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let guard = self.inner.read().unwrap();
        guard.search(query, limit)
    }

    /// Search by type (read lock)
    pub fn search_by_type(&self, type_filter: &str, limit: usize) -> Vec<SearchResult> {
        let guard = self.inner.read().unwrap();
        guard.search_by_type(type_filter, limit)
    }

    /// Combined search: text query + type filter (read lock)
    pub fn search_with_type(
        &self,
        query: &str,
        type_filter: &str,
        limit: usize,
    ) -> Vec<SearchResult> {
        let guard = self.inner.read().unwrap();
        guard.search_with_type(query, type_filter, limit)
    }

    /// Add a root (write lock)
    pub fn add_root(&self, root: &str) -> Result<usize, IndexError> {
        let mut guard = self.inner.write().unwrap();
        guard.add_root(root)
    }

    /// Remove a root (write lock)
    pub fn remove_root(&self, root: &str) -> usize {
        let mut guard = self.inner.write().unwrap();
        guard.remove_root(root)
    }

    /// Check if a root exists (read lock)
    pub fn has_root(&self, root: &str) -> bool {
        let guard = self.inner.read().unwrap();
        guard.has_root(root)
    }

    /// Get all roots (read lock)
    pub fn roots(&self) -> Vec<String> {
        let guard = self.inner.read().unwrap();
        guard.roots()
    }

    /// Get stats (read lock)
    pub fn stats(&self) -> Vec<RootStats> {
        let guard = self.inner.read().unwrap();
        guard.stats()
    }

    /// Get total file count (read lock)
    pub fn total_file_count(&self) -> usize {
        let guard = self.inner.read().unwrap();
        guard.total_file_count()
    }

    /// Get type count (read lock)
    pub fn type_count(&self, type_filter: &str) -> u64 {
        let guard = self.inner.read().unwrap();
        guard.type_count(type_filter)
    }

    /// Clear all indexes (write lock)
    pub fn clear(&self) {
        let mut guard = self.inner.write().unwrap();
        guard.clear()
    }

    /// Rebuild a root (write lock)
    pub fn rebuild_root(&self, root: &str) -> Result<usize, IndexError> {
        let mut guard = self.inner.write().unwrap();
        guard.rebuild_root(root)
    }

    /// Clone the Arc for sharing
    pub fn clone_shared(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

// Helper functions

/// Get the default indexes directory
pub fn default_indexes_dir() -> Option<PathBuf> {
    crate::dirs::indexes_dir()
}

/// Load a single root's index from its etch directory.
///
/// Pure function — safe to call on a background thread. Returns the
/// deserialized `SearchIndex` without mutating any shared state.
pub fn load_index_from_etch(indexes_dir: &Path, hash: &str) -> Result<SearchIndex, IndexError> {
    let etch_dir = indexes_dir.join(hash);
    if !etch_dir.is_dir() {
        return Err(IndexError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Index dir not found: {:?}", etch_dir),
        )));
    }
    let store = open_index_store(&etch_dir)?;
    Ok(store.read().clone())
}

pub fn hash_path(path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    let result = hasher.finalize();
    // Use first 16 bytes (32 hex chars) for reasonable uniqueness
    hex::encode(&result[..16])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_hash_path() {
        let hash1 = hash_path("/Users/foo");
        let hash2 = hash_path("/Users/bar");
        let hash3 = hash_path("/Users/foo");

        assert_ne!(hash1, hash2);
        assert_eq!(hash1, hash3);
        assert_eq!(hash1.len(), 32); // 16 bytes = 32 hex chars
    }

    #[test]
    fn test_manager_new() {
        let dir = TempDir::new().unwrap();
        let manager = IndexManager::with_dir(dir.path().to_path_buf()).unwrap();

        assert!(manager.roots().is_empty());
        assert_eq!(manager.total_file_count(), 0);
    }

    #[test]
    fn test_manager_add_remove_root() {
        let dir = TempDir::new().unwrap();
        let indexes_dir = dir.path().join("indexes");
        let test_root = dir.path().join("test_root");

        // Create test directory with some files
        fs::create_dir_all(&test_root).unwrap();
        fs::write(test_root.join("file1.txt"), "hello").unwrap();
        fs::write(test_root.join("file2.txt"), "world").unwrap();

        let mut manager = IndexManager::with_dir(indexes_dir).unwrap();

        // Add root
        let count = manager.add_root(test_root.to_str().unwrap()).unwrap();
        assert_eq!(count, 2);
        assert!(manager.has_root(test_root.to_str().unwrap()));
        assert_eq!(manager.roots().len(), 1);

        // Remove root
        let removed = manager.remove_root(test_root.to_str().unwrap());
        assert_eq!(removed, 2);
        assert!(!manager.has_root(test_root.to_str().unwrap()));
        assert!(manager.roots().is_empty());
    }

    #[test]
    fn test_manager_search() {
        let dir = TempDir::new().unwrap();
        let indexes_dir = dir.path().join("indexes");

        // Create two test directories
        let root1 = dir.path().join("root1");
        let root2 = dir.path().join("root2");
        fs::create_dir_all(&root1).unwrap();
        fs::create_dir_all(&root2).unwrap();

        fs::write(root1.join("report.pdf"), "data").unwrap();
        fs::write(root2.join("report.txt"), "data").unwrap();
        fs::write(root2.join("other.txt"), "data").unwrap();

        let mut manager = IndexManager::with_dir(indexes_dir).unwrap();
        manager.add_root(root1.to_str().unwrap()).unwrap();
        manager.add_root(root2.to_str().unwrap()).unwrap();

        // Search should find files from both roots
        let results = manager.search("report", 10);
        assert_eq!(results.len(), 2);

        // Search for specific file
        let results = manager.search("other", 10);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_manager_persistence() {
        use crate::cache::ControlDb;

        let dir = TempDir::new().unwrap();
        let indexes_dir = dir.path().join("indexes");
        let db_dir = dir.path().join("db");
        let test_root = dir.path().join("test_root");

        fs::create_dir_all(&test_root).unwrap();
        fs::write(test_root.join("file.txt"), "data").unwrap();

        // Create and populate manager with a disk-backed ControlDb
        {
            let control_db = ControlDb::open_at(&db_dir).unwrap();
            let mut manager =
                IndexManager::with_dir_and_db(indexes_dir.clone(), control_db).unwrap();
            manager.add_root(test_root.to_str().unwrap()).unwrap();
        }

        // Load fresh manager - should have the root (same ControlDb path)
        {
            let control_db = ControlDb::open_at(&db_dir).unwrap();
            let manager = IndexManager::load_from_with_db(indexes_dir, control_db).unwrap();
            assert!(manager.has_root(test_root.to_str().unwrap()));
            assert_eq!(manager.total_file_count(), 1);

            // Search should work
            let results = manager.search("file", 10);
            assert_eq!(results.len(), 1);
        }
    }

    #[test]
    fn test_manager_stats() {
        let dir = TempDir::new().unwrap();
        let indexes_dir = dir.path().join("indexes");
        let test_root = dir.path().join("test_root");

        fs::create_dir_all(&test_root).unwrap();
        fs::write(test_root.join("file.txt"), "hello world").unwrap();

        let mut manager = IndexManager::with_dir(indexes_dir).unwrap();
        manager.add_root(test_root.to_str().unwrap()).unwrap();

        let stats = manager.stats();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].file_count, 1);
        assert!(stats[0].last_indexed > 0);
    }

    #[test]
    fn test_shared_manager() {
        let dir = TempDir::new().unwrap();
        let indexes_dir = dir.path().join("indexes");
        let test_root = dir.path().join("test_root");

        fs::create_dir_all(&test_root).unwrap();
        fs::write(test_root.join("file.txt"), "data").unwrap();

        let manager = IndexManager::with_dir(indexes_dir).unwrap();
        let shared = SharedIndexManager::new(manager);

        // Add root
        shared.add_root(test_root.to_str().unwrap()).unwrap();
        assert!(shared.has_root(test_root.to_str().unwrap()));

        // Search
        let results = shared.search("file", 10);
        assert_eq!(results.len(), 1);

        // Clone and use from another "thread"
        let shared2 = shared.clone_shared();
        let results2 = shared2.search("file", 10);
        assert_eq!(results2.len(), 1);
    }

    #[test]
    fn test_manager_search_with_type() {
        let dir = TempDir::new().unwrap();
        let indexes_dir = dir.path().join("indexes");
        let test_root = dir.path().join("test_root");

        // Create test directory structure with files and folders
        fs::create_dir_all(&test_root).unwrap();
        fs::create_dir_all(test_root.join("zero")).unwrap();
        fs::create_dir_all(test_root.join("zero_backup")).unwrap();
        fs::create_dir_all(test_root.join("other")).unwrap();
        fs::write(test_root.join("zero.txt"), "data").unwrap();
        fs::write(test_root.join("zero.pdf"), "data").unwrap();
        fs::write(test_root.join("zero").join("nested.txt"), "data").unwrap();

        let mut manager = IndexManager::with_dir(indexes_dir).unwrap();
        manager.add_root(test_root.to_str().unwrap()).unwrap();

        // Search for "zero" without type filter - should find files and folders
        let all_results = manager.search("zero", 100);
        assert!(all_results.len() >= 4); // zero/, zero_backup/, zero.txt, zero.pdf

        // Search for folders named "zero" - should only find directories
        let folder_results = manager.search_with_type("zero", "folder", 100);
        assert_eq!(folder_results.len(), 2); // zero/, zero_backup/
        for result in &folder_results {
            assert!(
                result.node.is_directory(),
                "Expected directory, got file: {}",
                result.node.name
            );
            assert!(
                result.node.name.contains("zero"),
                "Expected name to contain 'zero': {}",
                result.node.name
            );
        }

        // Search for files named "zero" - should only find files
        let file_results = manager.search_with_type("zero", "file", 100);
        assert_eq!(file_results.len(), 2); // zero.txt, zero.pdf
        for result in &file_results {
            assert!(
                result.node.is_file(),
                "Expected file, got directory: {}",
                result.node.name
            );
        }

        // Search for documents named "zero" - should find .txt and .pdf
        let doc_results = manager.search_with_type("zero", "documents", 100);
        assert!(!doc_results.is_empty()); // At least zero.pdf (and possibly zero.txt depending on doc extensions)
    }
}
