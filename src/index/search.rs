//! Search index - Cardinal-style fast file search
//!
//! This is the main search engine that stores file nodes in a slab
//! and maintains a BTreeMap name index for fast lookups.
//!
//! ## Fast Type Filtering
//!
//! Type queries (e.g., `--type images`) use roaring bitmaps for O(result_count)
//! performance instead of O(total_files). For 1.3M files:
//! - Before: ~1.7 seconds
//! - After: <1ms
//!
//! ## Fast Recent Files
//!
//! The mtime_index (BTreeMap<mtime, Vec<slab_index>>) enables O(K) queries for
//! "most recent K files" instead of O(n) full scan. Built on index load from
//! existing mtime data in file nodes.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use jwalk::WalkDir;
use serde::{Deserialize, Serialize};

use crate::scanner::CrawlProgress;

use super::node::{FileNode, NodeType};
use super::type_index::{FileTypeCategory, TypeIndex, TypeIndexStats};

/// Index of filename -> slab indices for fast lookup
type NameIndex = BTreeMap<String, Vec<usize>>;

/// Index of mtime -> slab indices for fast "recent files" queries
/// BTreeMap is sorted, so iterating from end gives most recent first
type MtimeIndex = BTreeMap<u64, Vec<usize>>;

/// A search result with relevance score
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The matching file node
    pub node: FileNode,
    /// Relevance score (higher = better match)
    pub score: u32,
}

/// Options for search queries
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Maximum number of results to return
    pub limit: usize,
    /// Include directories in results
    pub include_dirs: bool,
    /// Include files in results
    pub include_files: bool,
    /// Case-sensitive search
    pub case_sensitive: bool,
    /// Filter by extension (e.g., "pdf", "rs")
    pub extension_filter: Option<String>,
    /// Include files in Trash (default: true for backward compatibility)
    pub include_trash: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 100,
            include_dirs: true,
            include_files: true,
            case_sensitive: false,
            extension_filter: None,
            include_trash: true, // Default true for backward compatibility
        }
    }
}

impl SearchOptions {
    /// Create options with a specific limit
    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit,
            ..Default::default()
        }
    }

    /// Only search for files
    pub fn files_only(mut self) -> Self {
        self.include_dirs = false;
        self.include_files = true;
        self
    }

    /// Only search for directories
    pub fn dirs_only(mut self) -> Self {
        self.include_dirs = true;
        self.include_files = false;
        self
    }

    /// Filter by file extension
    pub fn with_extension(mut self, ext: &str) -> Self {
        self.extension_filter = Some(ext.to_lowercase());
        self
    }

    /// Exclude files in Trash from results
    pub fn exclude_trash(mut self) -> Self {
        self.include_trash = false;
        self
    }

    /// Include files in Trash in results (default)
    pub fn include_trash(mut self) -> Self {
        self.include_trash = true;
        self
    }
}

/// Sort order for search results
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortBy {
    /// Sort by relevance score (default for text queries)
    #[default]
    Relevance,
    /// Sort by modification time, most recent first
    RecentFirst,
    /// No explicit sort (bitmap iteration order)
    None,
}

/// Unified search query — one struct, one method, all dimensions.
///
/// Use builder constructors for common patterns:
/// ```ignore
/// SearchQuery::text("report", 50)
/// SearchQuery::by_type(FileTypeCategory::Images, 100)
/// SearchQuery::recent(20)
/// SearchQuery::all(100)
/// SearchQuery::text("*.rs", 50).with_type(FileTypeCategory::Code)
/// ```
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    /// Text to match against filenames (empty = no text filter)
    pub text: String,
    /// Filter by file type category
    pub type_filter: Option<FileTypeCategory>,
    /// Filter by file extension (lowercase, no dot)
    pub extension: Option<String>,
    /// Sort order
    pub sort: SortBy,
    /// Include directories in results
    pub include_dirs: bool,
    /// Include files in results
    pub include_files: bool,
    /// Include files in Trash
    pub include_trash: bool,
    /// Maximum number of results
    pub limit: usize,
}

impl SearchQuery {
    /// Search by text query
    pub fn text(query: &str, limit: usize) -> Self {
        Self {
            text: query.to_string(),
            limit,
            include_dirs: true,
            include_files: true,
            include_trash: true,
            ..Default::default()
        }
    }

    /// Search by file type category (ultra-fast bitmap lookup)
    pub fn by_type(category: FileTypeCategory, limit: usize) -> Self {
        Self {
            type_filter: Some(category),
            limit,
            include_dirs: true,
            include_files: true,
            include_trash: true,
            sort: SortBy::None,
            ..Default::default()
        }
    }

    /// Get most recently modified files
    pub fn recent(limit: usize) -> Self {
        Self {
            limit,
            include_dirs: false,
            include_files: true,
            include_trash: true,
            sort: SortBy::RecentFirst,
            ..Default::default()
        }
    }

    /// List all files (for path-only filtering)
    pub fn all(limit: usize) -> Self {
        Self {
            limit,
            include_dirs: true,
            include_files: true,
            include_trash: true,
            sort: SortBy::None,
            ..Default::default()
        }
    }

    /// Add a type filter
    pub fn with_type(mut self, t: FileTypeCategory) -> Self {
        self.type_filter = Some(t);
        self
    }

    /// Add an optional type filter
    pub fn with_type_opt(mut self, t: Option<FileTypeCategory>) -> Self {
        self.type_filter = t;
        self
    }

    /// Add an extension filter
    pub fn with_extension(mut self, ext: &str) -> Self {
        self.extension = Some(ext.to_lowercase());
        self
    }

    /// Only return files (not directories)
    pub fn files_only(mut self) -> Self {
        self.include_files = true;
        self.include_dirs = false;
        self
    }

    /// Only return directories
    pub fn dirs_only(mut self) -> Self {
        self.include_files = false;
        self.include_dirs = true;
        self
    }

    /// Exclude files in Trash
    pub fn exclude_trash(mut self) -> Self {
        self.include_trash = false;
        self
    }

    /// Set sort order
    pub fn sort(mut self, sort: SortBy) -> Self {
        self.sort = sort;
        self
    }
}

/// Fast file search index using slab storage and name index
///
/// This is a Cardinal-style implementation optimized for:
/// - Fast substring matching on filenames
/// - Low memory overhead via slab storage
/// - O(n) search where n = unique filenames (not total files)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndex {
    /// Slab storage for file nodes (index -> FileNode)
    slab: Vec<FileNode>,

    /// Name index: lowercase filename -> list of slab indices
    /// Multiple files can have the same name in different directories
    name_index: NameIndex,

    /// Type index: roaring bitmaps for ultra-fast type filtering
    type_index: TypeIndex,

    /// Mtime index: mtime -> list of slab indices for fast "recent files" queries
    /// Built on load, not persisted (derived from mtime already in FileNode)
    mtime_index: MtimeIndex,

    /// Root paths this index was built from (supports multiple roots)
    roots: Vec<String>,

    /// Total number of files indexed
    file_count: usize,

    /// Total number of directories indexed
    dir_count: usize,

    /// Total bytes of all indexed files
    total_bytes: u64,
}

impl SearchIndex {
    /// Create a new empty search index
    pub fn new() -> Self {
        Self {
            slab: Vec::new(),
            name_index: BTreeMap::new(),
            type_index: TypeIndex::new(),
            mtime_index: BTreeMap::new(),
            roots: Vec::new(),
            file_count: 0,
            dir_count: 0,
            total_bytes: 0,
        }
    }

    /// Create an index with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slab: Vec::with_capacity(capacity),
            name_index: BTreeMap::new(),
            type_index: TypeIndex::new(),
            mtime_index: BTreeMap::new(),
            roots: Vec::new(),
            file_count: 0,
            dir_count: 0,
            total_bytes: 0,
        }
    }

    /// Get the number of indexed files
    pub fn file_count(&self) -> usize {
        self.file_count
    }

    /// Get the number of indexed directories
    pub fn dir_count(&self) -> usize {
        self.dir_count
    }

    /// Get total bytes of all indexed files
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Get the root paths this index was built from
    pub fn roots(&self) -> &[String] {
        &self.roots
    }

    /// Get the first root path (for backwards compatibility)
    pub fn root(&self) -> Option<&str> {
        self.roots.first().map(|s| s.as_str())
    }

    /// Set the root paths (used when restoring from persistence)
    pub fn set_roots(&mut self, roots: Vec<String>) {
        self.roots = roots;
    }

    /// Add a root path (used when restoring from persistence)
    pub fn add_root_path(&mut self, root: String) {
        if !self.roots.contains(&root) {
            self.roots.push(root);
        }
    }

    /// Check if a root path is already indexed
    pub fn has_root(&self, root: &str) -> bool {
        self.roots.iter().any(|r| r == root)
    }

    /// Get the type index for fast type queries
    pub fn type_index(&self) -> &TypeIndex {
        &self.type_index
    }

    /// Get mutable type index
    pub fn type_index_mut(&mut self) -> &mut TypeIndex {
        &mut self.type_index
    }

    /// Get type index statistics
    pub fn type_stats(&self) -> TypeIndexStats {
        self.type_index.stats()
    }

    /// Get total number of nodes (files + directories)
    pub fn node_count(&self) -> usize {
        self.slab.len()
    }

    /// Get number of unique filenames in the index
    pub fn unique_names(&self) -> usize {
        self.name_index.len()
    }

    /// Build index from a directory path (clears existing index)
    ///
    /// This crawls the directory tree and indexes all files and directories.
    /// For adding to an existing index, use `add_root()` instead.
    pub fn build_from_path(&mut self, root: &Path) -> Result<(), IndexError> {
        self.clear();
        self.add_root(root)
    }

    /// Build index from a directory path with progress tracking (clears existing index)
    pub fn build_from_path_with_progress(
        &mut self,
        root: &Path,
        progress: Option<Arc<CrawlProgress>>,
    ) -> Result<(), IndexError> {
        self.clear();
        self.add_root_with_progress(root, progress)
    }

    /// Add a root directory to the index without clearing existing entries
    ///
    /// This allows indexing multiple directories (e.g., home folder + external drive)
    /// into a single searchable index.
    pub fn add_root(&mut self, root: &Path) -> Result<(), IndexError> {
        self.add_root_with_progress(root, None)
    }

    /// Add a root directory with progress tracking
    pub fn add_root_with_progress(
        &mut self,
        root: &Path,
        progress: Option<Arc<CrawlProgress>>,
    ) -> Result<(), IndexError> {
        let root_str = root.to_string_lossy().to_string();

        // Check if already indexed
        if self.has_root(&root_str) {
            tracing::info!("Root {} already indexed, skipping", root_str);
            return Ok(());
        }

        // Add to roots list
        self.roots.push(root_str);

        // Walk directory tree with jwalk (parallel)
        let walk_dir = WalkDir::new(root).skip_hidden(false).follow_links(false);

        for result in walk_dir {
            // Check for cancellation
            if let Some(ref p) = progress
                && p.is_cancelled()
            {
                break;
            }

            match result {
                Ok(entry) => {
                    let file_type = entry.file_type();
                    let entry_path = entry.path();

                    // Get name
                    let name = entry_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();

                    // Skip empty names (root directory)
                    if name.is_empty() {
                        continue;
                    }

                    // Store ABSOLUTE path for multi-root support
                    let abs_path = entry_path.to_string_lossy().to_string();

                    if file_type.is_dir() {
                        // Index directory
                        let mtime = entry
                            .metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);

                        let node = FileNode::directory(name, abs_path, mtime);
                        self.insert(node);

                        if let Some(ref p) = progress {
                            p.dirs_found
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    } else if file_type.is_file() {
                        // Index file
                        let metadata = match entry.metadata() {
                            Ok(m) => m,
                            Err(_) => continue,
                        };

                        let size = metadata.len();
                        let mtime = metadata
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);

                        let node = FileNode::file(name, abs_path, size, mtime);
                        self.insert(node);

                        if let Some(ref p) = progress {
                            p.add_file(size);
                        }
                    }
                    // Skip symlinks and special files
                }
                Err(e) => {
                    tracing::warn!("Walk error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Remove all entries from a specific root directory
    ///
    /// Returns the number of entries removed.
    pub fn remove_root(&mut self, root: &Path) -> usize {
        let root_str = root.to_string_lossy().to_string();
        let root_prefix = if root_str.ends_with('/') {
            root_str.clone()
        } else {
            format!("{}/", root_str)
        };

        // Remove from roots list
        self.roots.retain(|r| r != &root_str);

        // Collect paths to remove
        let paths_to_remove: Vec<String> = self
            .slab
            .iter()
            .filter(|node| node.path.starts_with(&root_prefix) || node.path == root_str)
            .map(|node| node.path.clone())
            .collect();

        let count = paths_to_remove.len();
        for path in paths_to_remove {
            self.remove(&path);
        }

        count
    }

    /// Insert a file node into the index
    pub fn insert(&mut self, node: FileNode) {
        // Add to slab first to get index
        let index = self.slab.len();
        let name_lower = node.name.to_lowercase();
        let is_directory = node.is_directory();
        let extension = node.extension().map(|s| s.to_string());
        let mtime = node.mtime;
        let path = node.path.clone();

        // Update stats
        match node.node_type {
            NodeType::File => {
                self.file_count += 1;
                self.total_bytes += node.size;
            }
            NodeType::Directory => {
                self.dir_count += 1;
            }
            NodeType::Symlink => {}
        }

        self.slab.push(node);

        // Add to name index
        self.name_index.entry(name_lower).or_default().push(index);

        // Add to type index for fast type filtering (also tracks trash)
        self.type_index
            .add_file(index as u32, &path, extension.as_deref(), is_directory);

        // Add to mtime index for fast "recent files" queries
        self.mtime_index.entry(mtime).or_default().push(index);
    }

    /// Clear the index
    pub fn clear(&mut self) {
        self.slab.clear();
        self.name_index.clear();
        self.type_index.clear();
        self.mtime_index.clear();
        self.roots.clear();
        self.file_count = 0;
        self.dir_count = 0;
        self.total_bytes = 0;
    }

    /// Remove a file from the index by path
    ///
    /// Returns true if the file was found and removed, false otherwise.
    pub fn remove(&mut self, path: &str) -> bool {
        // Find the node by path
        let mut found_idx: Option<usize> = None;
        let mut found_name: Option<String> = None;

        for (idx, node) in self.slab.iter().enumerate() {
            if node.path == path {
                found_idx = Some(idx);
                found_name = Some(node.name.to_lowercase());
                break;
            }
        }

        let (idx, name_lower) = match (found_idx, found_name) {
            (Some(idx), Some(name)) => (idx, name),
            _ => return false,
        };

        // Get node info before removal for stats update
        let node = &self.slab[idx];
        match node.node_type {
            NodeType::File => {
                self.file_count = self.file_count.saturating_sub(1);
                self.total_bytes = self.total_bytes.saturating_sub(node.size);
            }
            NodeType::Directory => {
                self.dir_count = self.dir_count.saturating_sub(1);
            }
            NodeType::Symlink => {}
        }

        // Remove from name index
        if let Some(indices) = self.name_index.get_mut(&name_lower) {
            indices.retain(|&i| i != idx);
            if indices.is_empty() {
                self.name_index.remove(&name_lower);
            }
        }

        // Remove from type index
        self.type_index.remove_file(idx as u32);

        // Note: We don't actually remove from the slab to avoid invalidating indices
        // In a production system, you'd want a more sophisticated approach
        // (e.g., mark as deleted, or use a proper slab crate with removal)

        true
    }

    /// Remove all files under a directory path
    ///
    /// Returns the number of entries removed.
    pub fn remove_dir(&mut self, dir_path: &str) -> usize {
        let prefix = if dir_path.ends_with('/') {
            dir_path.to_string()
        } else {
            format!("{}/", dir_path)
        };

        // Collect paths to remove (can't modify while iterating)
        let paths_to_remove: Vec<String> = self
            .slab
            .iter()
            .filter(|node| node.path.starts_with(&prefix) || node.path == dir_path)
            .map(|node| node.path.clone())
            .collect();

        let count = paths_to_remove.len();
        for path in paths_to_remove {
            self.remove(&path);
        }

        count
    }

    /// Search for files matching a query string
    ///
    /// Returns up to `limit` results sorted by relevance.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        self.search_with_options(query, SearchOptions::with_limit(limit))
    }

    /// Unified search — dispatches to optimized internal paths based on query shape.
    ///
    /// This is the preferred entry point. All other search methods are kept for
    /// backward compatibility.
    pub fn query(&self, q: SearchQuery) -> Vec<SearchResult> {
        match (q.text.is_empty(), &q.type_filter, &q.sort) {
            // Recent files (with optional text + type)
            (_, _, SortBy::RecentFirst) => {
                if q.text.is_empty() {
                    self.search_recent(q.limit, q.type_filter)
                } else {
                    self.search_recent_with_query(&q.text, q.limit, q.type_filter)
                }
            }
            // Type-only search (bitmap fast path)
            (true, Some(cat), _) => self.bitmap_search(*cat, q.limit, q.include_trash),
            // Text + type search (bitmap intersection)
            (false, Some(cat), _) => self.text_with_bitmap(&q.text, *cat, q.limit, q.include_trash),
            // List all (no text, no type, no extension)
            (true, None, _) if q.extension.is_none() => self.list_all(q.limit),
            // Extension or text search with options
            _ => {
                let opts = self.query_to_options(&q);
                self.search_with_options(&q.text, opts)
            }
        }
    }

    /// Type-only search via bitmap (internal, used by query())
    fn bitmap_search(
        &self,
        category: FileTypeCategory,
        limit: usize,
        include_trash: bool,
    ) -> Vec<SearchResult> {
        let Some(type_bitmap) = self.type_index.get_indices(category) else {
            return Vec::new();
        };

        let indices_iter: Box<dyn Iterator<Item = u32>> = if include_trash {
            Box::new(type_bitmap.iter())
        } else {
            let filtered = type_bitmap - self.type_index.trash_bitmap();
            Box::new(filtered.into_iter())
        };

        indices_iter
            .take(limit)
            .filter_map(|idx| {
                self.slab.get(idx as usize).map(|node| SearchResult {
                    node: node.clone(),
                    score: 100,
                })
            })
            .collect()
    }

    /// Text + type bitmap intersection search (internal, used by query())
    fn text_with_bitmap(
        &self,
        text: &str,
        category: FileTypeCategory,
        limit: usize,
        include_trash: bool,
    ) -> Vec<SearchResult> {
        let Some(type_bitmap) = self.type_index.get_indices(category) else {
            return Vec::new();
        };

        let query_lower = text.to_lowercase();
        let mut results = Vec::new();

        for (name, indices) in &self.name_index {
            if !name.contains(&query_lower) {
                continue;
            }

            let score = self.calculate_score(name, &query_lower);

            for &idx in indices {
                if !type_bitmap.contains(idx as u32) {
                    continue;
                }
                if !include_trash && self.type_index.is_in_trash(idx as u32) {
                    continue;
                }

                if let Some(node) = self.slab.get(idx) {
                    results.push(SearchResult {
                        node: node.clone(),
                        score,
                    });

                    if results.len() >= limit * 2 {
                        break;
                    }
                }
            }

            if results.len() >= limit * 2 {
                break;
            }
        }

        results.sort_by_key(|a| std::cmp::Reverse(a.score));
        results.truncate(limit);
        results
    }

    /// Convert SearchQuery to legacy SearchOptions
    fn query_to_options(&self, q: &SearchQuery) -> SearchOptions {
        let mut opts = SearchOptions::with_limit(q.limit);
        opts.include_dirs = q.include_dirs;
        opts.include_files = q.include_files;
        opts.include_trash = q.include_trash;
        opts.extension_filter = q.extension.clone();
        opts
    }

    /// Ultra-fast type-only search using roaring bitmaps
    ///
    /// This is O(result_count) instead of O(total_files).
    /// For 1.3M files: <1ms instead of ~1.7 seconds.
    ///
    /// # Arguments
    /// * `type_name` - Type category name (e.g., "images", "videos", "audio")
    /// * `limit` - Maximum results to return
    ///
    /// # Returns
    /// Vec of SearchResults sorted by path (for consistency)
    pub fn search_by_type(&self, type_name: &str, limit: usize) -> Vec<SearchResult> {
        self.search_by_type_filtered(type_name, limit, true)
    }

    /// Ultra-fast type-only search with trash filtering
    ///
    /// # Arguments
    /// * `type_name` - Type category name (e.g., "images", "videos", "audio")
    /// * `limit` - Maximum results to return
    /// * `include_trash` - Whether to include files in Trash
    ///
    /// # Returns
    /// Vec of SearchResults sorted by path (for consistency)
    pub fn search_by_type_filtered(
        &self,
        type_name: &str,
        limit: usize,
        include_trash: bool,
    ) -> Vec<SearchResult> {
        let Some(category) = FileTypeCategory::parse_str(type_name) else {
            return Vec::new();
        };

        let Some(type_bitmap) = self.type_index.get_indices(category) else {
            return Vec::new();
        };

        // If excluding trash, use bitmap difference for O(1) filtering
        let indices_iter: Box<dyn Iterator<Item = u32>> = if include_trash {
            Box::new(type_bitmap.iter())
        } else {
            // Exclude trash using bitmap difference
            let filtered = type_bitmap - self.type_index.trash_bitmap();
            Box::new(filtered.into_iter())
        };

        // Convert to SearchResults with limit
        indices_iter
            .take(limit)
            .filter_map(|idx| {
                self.slab.get(idx as usize).map(|node| SearchResult {
                    node: node.clone(),
                    score: 100, // All type matches have equal relevance
                })
            })
            .collect()
    }

    /// Search by multiple types (union)
    ///
    /// Returns files matching ANY of the given types.
    pub fn search_by_types(&self, type_names: &[&str], limit: usize) -> Vec<SearchResult> {
        self.search_by_types_filtered(type_names, limit, true)
    }

    /// Search by multiple types with trash filtering
    ///
    /// Returns files matching ANY of the given types.
    pub fn search_by_types_filtered(
        &self,
        type_names: &[&str],
        limit: usize,
        include_trash: bool,
    ) -> Vec<SearchResult> {
        let categories: Vec<FileTypeCategory> = type_names
            .iter()
            .filter_map(|name| FileTypeCategory::parse_str(name))
            .collect();

        if categories.is_empty() {
            return Vec::new();
        }

        // Union all type bitmaps
        let mut union = self.type_index.union_categories(&categories);

        // Exclude trash if requested
        if !include_trash {
            union -= self.type_index.trash_bitmap();
        }

        // Get indices from union bitmap
        union
            .iter()
            .take(limit)
            .filter_map(|idx| {
                self.slab.get(idx as usize).map(|node| SearchResult {
                    node: node.clone(),
                    score: 100,
                })
            })
            .collect()
    }

    /// Combined search: text query + type filter
    ///
    /// Uses bitmap intersection for fast filtering.
    pub fn search_with_type(
        &self,
        query: &str,
        type_name: &str,
        limit: usize,
    ) -> Vec<SearchResult> {
        self.search_with_type_filtered(query, type_name, limit, true)
    }

    /// Combined search: text query + type filter with trash filtering
    ///
    /// Uses bitmap intersection for fast filtering.
    ///
    /// # Arguments
    /// * `query` - Text query to search for
    /// * `type_name` - Type category name (e.g., "images", "videos")
    /// * `limit` - Maximum results to return
    /// * `include_trash` - Whether to include files in Trash
    pub fn search_with_type_filtered(
        &self,
        query: &str,
        type_name: &str,
        limit: usize,
        include_trash: bool,
    ) -> Vec<SearchResult> {
        let Some(category) = FileTypeCategory::parse_str(type_name) else {
            // Fall back to regular search if type is invalid
            let mut opts = SearchOptions::with_limit(limit);
            opts.include_trash = include_trash;
            return self.search_with_options(query, opts);
        };

        let Some(type_bitmap) = self.type_index.get_indices(category) else {
            return Vec::new();
        };

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        // Search through name index, but only consider files in the type bitmap
        for (name, indices) in &self.name_index {
            if !name.contains(&query_lower) {
                continue;
            }

            let score = self.calculate_score(name, &query_lower);

            for &idx in indices {
                // Fast bitmap check - O(1)
                if !type_bitmap.contains(idx as u32) {
                    continue;
                }

                // Trash filter - O(1) bitmap check
                if !include_trash && self.type_index.is_in_trash(idx as u32) {
                    continue;
                }

                if let Some(node) = self.slab.get(idx) {
                    results.push(SearchResult {
                        node: node.clone(),
                        score,
                    });

                    if results.len() >= limit * 2 {
                        break;
                    }
                }
            }

            if results.len() >= limit * 2 {
                break;
            }
        }

        // Sort by score and truncate
        results.sort_by_key(|a| std::cmp::Reverse(a.score));
        results.truncate(limit);
        results
    }

    /// Search for most recent files
    ///
    /// Returns the N most recently modified files, optionally filtered by type.
    /// Uses mtime_index for O(K) performance instead of O(n) full scan.
    ///
    /// # Arguments
    /// * `limit` - Maximum number of results
    /// * `type_filter` - Optional type category to filter by
    ///
    /// # Returns
    /// Vec of SearchResults sorted by mtime descending (most recent first)
    pub fn search_recent(
        &self,
        limit: usize,
        type_filter: Option<FileTypeCategory>,
    ) -> Vec<SearchResult> {
        let mut results = Vec::with_capacity(limit);

        // Get optional type bitmap for filtering
        let type_bitmap = type_filter.and_then(|cat| self.type_index.get_indices(cat));

        // Iterate mtime_index from end (most recent first)
        for (_mtime, indices) in self.mtime_index.iter().rev() {
            for &idx in indices.iter().rev() {
                // If type filter specified, check bitmap
                if let Some(bitmap) = type_bitmap
                    && !bitmap.contains(idx as u32)
                {
                    continue;
                }

                if let Some(node) = self.slab.get(idx) {
                    // Skip directories unless explicitly requested
                    if node.is_directory() {
                        continue;
                    }

                    results.push(SearchResult {
                        node: node.clone(),
                        score: 100, // All recent results have equal relevance
                    });

                    if results.len() >= limit {
                        return results;
                    }
                }
            }
        }

        results
    }

    /// Search for most recent files matching a query
    ///
    /// Combines text search with recency sorting.
    pub fn search_recent_with_query(
        &self,
        query: &str,
        limit: usize,
        type_filter: Option<FileTypeCategory>,
    ) -> Vec<SearchResult> {
        // If no query, just return recent files
        if query.is_empty() {
            return self.search_recent(limit, type_filter);
        }

        let query_lower = query.to_lowercase();
        let type_bitmap = type_filter.and_then(|cat| self.type_index.get_indices(cat));

        let mut results = Vec::new();

        // Iterate mtime_index from end (most recent first)
        for (_mtime, indices) in self.mtime_index.iter().rev() {
            for &idx in indices.iter().rev() {
                // Type filter check
                if let Some(bitmap) = type_bitmap
                    && !bitmap.contains(idx as u32)
                {
                    continue;
                }

                if let Some(node) = self.slab.get(idx) {
                    // Skip directories
                    if node.is_directory() {
                        continue;
                    }

                    // Query match check
                    if !node.name.to_lowercase().contains(&query_lower) {
                        continue;
                    }

                    results.push(SearchResult {
                        node: node.clone(),
                        score: self.calculate_score(&node.name.to_lowercase(), &query_lower),
                    });

                    if results.len() >= limit {
                        return results;
                    }
                }
            }
        }

        results
    }

    /// List all files in the index (for path-only filtering)
    ///
    /// Returns all files up to the limit, useful when filtering by path without a query.
    pub fn list_all(&self, limit: usize) -> Vec<SearchResult> {
        self.slab
            .iter()
            .take(limit)
            .map(|node| SearchResult {
                node: node.clone(),
                score: 50,
            })
            .collect()
    }

    /// Search with full options
    ///
    /// If query is empty but filters are set (extension, type), returns all matching files.
    pub fn search_with_options(&self, query: &str, options: SearchOptions) -> Vec<SearchResult> {
        let query_lower = query.to_lowercase();

        // If query is empty and no extension filter, return nothing
        // But if extension filter is set, we list all files with that extension
        let has_filter = options.extension_filter.is_some();
        if query.is_empty() && !has_filter {
            return Vec::new();
        }
        let query_cmp = if options.case_sensitive {
            query.to_string()
        } else {
            query_lower.clone()
        };

        let mut results = Vec::new();

        // Search through name index
        for (name, indices) in &self.name_index {
            // Check if name contains query (skip check if query is empty - listing mode)
            if !query.is_empty() {
                let name_cmp = if options.case_sensitive {
                    // Need to get original name from slab
                    if let Some(idx) = indices.first() {
                        if let Some(node) = self.slab.get(*idx) {
                            node.name.clone()
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                } else {
                    name.clone()
                };

                if !name_cmp.contains(&query_cmp) {
                    continue;
                }
            }

            // Calculate score for this name
            let score = if query.is_empty() {
                // Listing mode - score by name length (shorter = more relevant)
                100u32.saturating_sub(name.len() as u32)
            } else {
                let name_cmp = if options.case_sensitive {
                    if let Some(idx) = indices.first() {
                        if let Some(node) = self.slab.get(*idx) {
                            node.name.clone()
                        } else {
                            name.clone()
                        }
                    } else {
                        name.clone()
                    }
                } else {
                    name.clone()
                };
                self.calculate_score(&name_cmp, &query_cmp)
            };

            // Add all files with this name
            for &idx in indices {
                if let Some(node) = self.slab.get(idx) {
                    // Apply filters (pass slab index for bitmap lookups)
                    if !self.matches_filters(node, idx, &options) {
                        continue;
                    }

                    results.push(SearchResult {
                        node: node.clone(),
                        score,
                    });

                    // Early exit if we have enough results
                    if results.len() >= options.limit * 2 {
                        break;
                    }
                }
            }

            if results.len() >= options.limit * 2 {
                break;
            }
        }

        // Sort by score (descending)
        results.sort_by_key(|a| std::cmp::Reverse(a.score));

        // Truncate to limit
        results.truncate(options.limit);

        results
    }

    /// Check if a node matches the search filters
    ///
    /// # Arguments
    /// * `node` - The file node to check
    /// * `slab_index` - The index of the node in the slab (for bitmap lookups)
    /// * `options` - Search options including filters
    fn matches_filters(&self, node: &FileNode, slab_index: usize, options: &SearchOptions) -> bool {
        // Trash filter - use bitmap for O(1) check
        if !options.include_trash && self.type_index.is_in_trash(slab_index as u32) {
            return false;
        }

        // Type filter
        match node.node_type {
            NodeType::File => {
                if !options.include_files {
                    return false;
                }
            }
            NodeType::Directory => {
                if !options.include_dirs {
                    return false;
                }
            }
            NodeType::Symlink => {
                // Include symlinks with files for now
                if !options.include_files {
                    return false;
                }
            }
        }

        // Extension filter
        if let Some(ref ext_filter) = options.extension_filter {
            if let Some(ext) = node.extension() {
                if ext.to_lowercase() != *ext_filter {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    /// Calculate relevance score for a match
    ///
    /// Higher score = better match:
    /// - Exact match: 1000
    /// - Starts with query: 500
    /// - Contains query: 100
    /// - Bonus for shorter names (more specific)
    fn calculate_score(&self, name: &str, query: &str) -> u32 {
        let mut score = 0u32;

        // Exact match
        if name == query {
            score += 1000;
        }
        // Starts with query
        else if name.starts_with(query) {
            score += 500;
        }
        // Contains query
        else {
            score += 100;
        }

        // Bonus for shorter names (max 100 bonus for very short names)
        let length_bonus = 100u32.saturating_sub(name.len() as u32);
        score += length_bonus;

        score
    }

    /// Get a node by its slab index
    pub fn get(&self, index: usize) -> Option<&FileNode> {
        self.slab.get(index)
    }

    /// Iterate over all nodes
    pub fn iter(&self) -> impl Iterator<Item = &FileNode> {
        self.slab.iter()
    }
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during indexing
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("Scan error: {0}")]
    Scan(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialize(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_index() {
        let index = SearchIndex::new();
        assert_eq!(index.file_count(), 0);
        assert_eq!(index.dir_count(), 0);
        assert_eq!(index.node_count(), 0);
    }

    #[test]
    fn test_insert_and_search() {
        let mut index = SearchIndex::new();

        index.insert(FileNode::file(
            "report.pdf".into(),
            "Documents/report.pdf".into(),
            1024,
            0,
        ));
        index.insert(FileNode::file(
            "report_final.pdf".into(),
            "Documents/report_final.pdf".into(),
            2048,
            0,
        ));
        index.insert(FileNode::file(
            "notes.txt".into(),
            "Documents/notes.txt".into(),
            512,
            0,
        ));

        assert_eq!(index.file_count(), 3);

        // Search for "report"
        let results = index.search("report", 10);
        assert_eq!(results.len(), 2);

        // "report.pdf" should score higher (shorter name)
        assert!(results[0].node.name == "report.pdf");
    }

    #[test]
    fn test_case_insensitive_search() {
        let mut index = SearchIndex::new();

        index.insert(FileNode::file(
            "README.md".into(),
            "README.md".into(),
            100,
            0,
        ));

        let results = index.search("readme", 10);
        assert_eq!(results.len(), 1);

        let results = index.search("README", 10);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_extension_filter() {
        let mut index = SearchIndex::new();

        index.insert(FileNode::file("doc.pdf".into(), "doc.pdf".into(), 100, 0));
        index.insert(FileNode::file("doc.txt".into(), "doc.txt".into(), 100, 0));
        index.insert(FileNode::file("doc.md".into(), "doc.md".into(), 100, 0));

        let options = SearchOptions::default().with_extension("pdf");
        let results = index.search_with_options("doc", options);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node.name, "doc.pdf");
    }

    #[test]
    fn test_files_only_filter() {
        let mut index = SearchIndex::new();

        index.insert(FileNode::file("docs.txt".into(), "docs.txt".into(), 100, 0));
        index.insert(FileNode::directory("docs".into(), "docs".into(), 0));

        let options = SearchOptions::default().files_only();
        let results = index.search_with_options("docs", options);

        assert_eq!(results.len(), 1);
        assert!(results[0].node.is_file());
    }

    #[test]
    fn test_dirs_only_filter() {
        let mut index = SearchIndex::new();

        index.insert(FileNode::file("docs.txt".into(), "docs.txt".into(), 100, 0));
        index.insert(FileNode::directory("docs".into(), "docs".into(), 0));

        let options = SearchOptions::default().dirs_only();
        let results = index.search_with_options("docs", options);

        assert_eq!(results.len(), 1);
        assert!(results[0].node.is_directory());
    }

    #[test]
    fn test_empty_query() {
        let mut index = SearchIndex::new();
        index.insert(FileNode::file("test.txt".into(), "test.txt".into(), 100, 0));

        let results = index.search("", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_no_results() {
        let mut index = SearchIndex::new();
        index.insert(FileNode::file("test.txt".into(), "test.txt".into(), 100, 0));

        let results = index.search("nonexistent", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_score_ranking() {
        let mut index = SearchIndex::new();

        // Exact match should rank highest
        index.insert(FileNode::file("test".into(), "test".into(), 100, 0));
        // Starts with should rank second
        index.insert(FileNode::file("testing".into(), "testing".into(), 100, 0));
        // Contains should rank lowest
        index.insert(FileNode::file("mytest".into(), "mytest".into(), 100, 0));

        let results = index.search("test", 10);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].node.name, "test"); // Exact match
        assert_eq!(results[1].node.name, "testing"); // Starts with
        assert_eq!(results[2].node.name, "mytest"); // Contains
    }

    #[test]
    fn test_limit() {
        let mut index = SearchIndex::new();

        for i in 0..100 {
            index.insert(FileNode::file(
                format!("file{}.txt", i),
                format!("file{}.txt", i),
                100,
                0,
            ));
        }

        let results = index.search("file", 10);
        assert_eq!(results.len(), 10);
    }

    #[test]
    fn test_clear() {
        let mut index = SearchIndex::new();
        index.insert(FileNode::file("test.txt".into(), "test.txt".into(), 100, 0));

        assert_eq!(index.file_count(), 1);

        index.clear();

        assert_eq!(index.file_count(), 0);
        assert_eq!(index.node_count(), 0);
        assert!(index.search("test", 10).is_empty());
    }

    #[test]
    fn test_multiple_files_same_name() {
        let mut index = SearchIndex::new();

        // Same filename in different directories
        index.insert(FileNode::file(
            "config.json".into(),
            "project1/config.json".into(),
            100,
            0,
        ));
        index.insert(FileNode::file(
            "config.json".into(),
            "project2/config.json".into(),
            200,
            0,
        ));

        let results = index.search("config", 10);
        assert_eq!(results.len(), 2);

        // Both should have same score
        assert_eq!(results[0].score, results[1].score);
    }
}
