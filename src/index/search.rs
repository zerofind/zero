//! Search index - Cardinal-style fast file search
//!
//! This is the main search engine that stores compact file nodes in a slab
//! with a contiguous PathArena, sorted name/mtime indexes, and roaring bitmaps.
//!
//! ## Memory Layout (compact)
//!
//! - PathArena: single `Vec<u8>` for all paths (~150MB for 2.5M files)
//! - CompactNode slab: 24 bytes per node (~60MB for 2.5M files)
//! - CompactNameIndex: sorted flat arrays (~72MB for 2M unique names)
//! - CompactMtimeIndex: sorted flat arrays (~10MB)
//! - TypeIndex bitmaps: ~8MB
//!
//! ## Fast Type Filtering
//!
//! Type queries (e.g., `--type images`) use roaring bitmaps for O(result_count)
//! performance instead of O(total_files).
//!
//! ## Fast Recent Files
//!
//! The mtime index enables O(K) queries for "most recent K files".

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use tracing::instrument;

use crate::scanner::CrawlProgress;
use jwalk::WalkDir;

use super::arena::PathArena;
use super::node::{CompactNode, FileNode, NodeType};
use super::type_index::{FileTypeCategory, TypeIndex, TypeIndexStats};

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

// ============================================================================
// Compact Name Index — sorted flat arrays instead of BTreeMap<String, Vec<usize>>
// ============================================================================

/// Entry in the compact name index — 16 bytes per unique filename
#[derive(Clone, Copy)]
#[repr(C)]
struct NameEntry {
    name_offset: u32,
    name_len: u16,
    _pad: u16,
    indices_start: u32,
    indices_count: u32,
}

/// Compact name index using contiguous sorted arrays
#[derive(Clone)]
struct CompactNameIndex {
    name_data: Vec<u8>,
    entries: Vec<NameEntry>,
    indices: Vec<u32>,
    /// Small overflow buffer for watcher inserts between rebuilds
    overflow: BTreeMap<String, Vec<u32>>,
}

impl std::fmt::Debug for CompactNameIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompactNameIndex")
            .field("entries", &self.entries.len())
            .field("overflow", &self.overflow.len())
            .finish()
    }
}

impl CompactNameIndex {
    fn new() -> Self {
        Self {
            name_data: Vec::new(),
            entries: Vec::new(),
            indices: Vec::new(),
            overflow: BTreeMap::new(),
        }
    }

    /// Build from a BTreeMap (used after bulk inserts)
    fn build_from(map: &BTreeMap<String, Vec<u32>>) -> Self {
        let total_names_bytes: usize = map.keys().map(|k| k.len()).sum();
        let total_indices: usize = map.values().map(|v| v.len()).sum();

        let mut name_data = Vec::with_capacity(total_names_bytes);
        let mut entries = Vec::with_capacity(map.len());
        let mut indices = Vec::with_capacity(total_indices);

        for (name, idx_list) in map {
            let name_offset = name_data.len() as u32;
            let name_len = name.len() as u16;
            name_data.extend_from_slice(name.as_bytes());

            let indices_start = indices.len() as u32;
            let indices_count = idx_list.len() as u32;
            indices.extend_from_slice(idx_list);

            entries.push(NameEntry {
                name_offset,
                name_len,
                _pad: 0,
                indices_start,
                indices_count,
            });
        }

        Self {
            name_data,
            entries,
            indices,
            overflow: BTreeMap::new(),
        }
    }

    /// Get name string for an entry
    #[inline]
    fn entry_name(&self, entry: &NameEntry) -> &str {
        let start = entry.name_offset as usize;
        let end = start + entry.name_len as usize;
        // SAFETY: all names come from to_lowercase() which produces valid UTF-8
        unsafe { std::str::from_utf8_unchecked(&self.name_data[start..end]) }
    }

    /// Get slab indices for an entry
    #[inline]
    fn entry_indices(&self, entry: &NameEntry) -> &[u32] {
        let start = entry.indices_start as usize;
        let end = start + entry.indices_count as usize;
        &self.indices[start..end]
    }

    /// Insert into overflow (for watcher updates)
    fn insert_overflow(&mut self, name: String, idx: u32) {
        self.overflow.entry(name).or_default().push(idx);
    }

    /// Total unique names (main + overflow)
    fn len(&self) -> usize {
        self.entries.len() + self.overflow.len()
    }

    /// Remove an index from overflow by name
    fn remove_from_overflow(&mut self, name: &str, idx: u32) {
        if let Some(indices) = self.overflow.get_mut(name) {
            indices.retain(|&i| i != idx);
            if indices.is_empty() {
                self.overflow.remove(name);
            }
        }
    }

    fn clear(&mut self) {
        self.name_data.clear();
        self.entries.clear();
        self.indices.clear();
        self.overflow.clear();
    }

    fn shrink_to_fit(&mut self) {
        self.name_data.shrink_to_fit();
        self.entries.shrink_to_fit();
        self.indices.shrink_to_fit();
    }
}

// ============================================================================
// Compact Mtime Index — sorted flat arrays instead of BTreeMap<u64, Vec<usize>>
// ============================================================================

/// Compact mtime index using contiguous sorted arrays
#[derive(Clone)]
struct CompactMtimeIndex {
    /// (mtime, start_in_indices, count) sorted by mtime ascending
    groups: Vec<(u64, u32, u32)>,
    /// Flat slab indices grouped by mtime
    indices: Vec<u32>,
    /// Overflow for watcher inserts
    overflow: Vec<(u64, u32)>,
}

impl std::fmt::Debug for CompactMtimeIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompactMtimeIndex")
            .field("groups", &self.groups.len())
            .field("overflow", &self.overflow.len())
            .finish()
    }
}

impl CompactMtimeIndex {
    fn new() -> Self {
        Self {
            groups: Vec::new(),
            indices: Vec::new(),
            overflow: Vec::new(),
        }
    }

    /// Build from a BTreeMap (used after bulk inserts)
    fn build_from(map: &BTreeMap<u64, Vec<u32>>) -> Self {
        let total_indices: usize = map.values().map(|v| v.len()).sum();

        let mut groups = Vec::with_capacity(map.len());
        let mut indices = Vec::with_capacity(total_indices);

        for (&mtime, idx_list) in map {
            let start = indices.len() as u32;
            let count = idx_list.len() as u32;
            indices.extend_from_slice(idx_list);
            groups.push((mtime, start, count));
        }

        Self {
            groups,
            indices,
            overflow: Vec::new(),
        }
    }

    /// Get slab indices for a group
    #[inline]
    fn group_indices(&self, start: u32, count: u32) -> &[u32] {
        let s = start as usize;
        let e = s + count as usize;
        &self.indices[s..e]
    }

    /// Insert into overflow
    fn insert_overflow(&mut self, mtime: u64, idx: u32) {
        self.overflow.push((mtime, idx));
    }

    fn clear(&mut self) {
        self.groups.clear();
        self.indices.clear();
        self.overflow.clear();
    }

    fn shrink_to_fit(&mut self) {
        self.groups.shrink_to_fit();
        self.indices.shrink_to_fit();
    }
}

// ============================================================================
// SearchIndex
// ============================================================================

/// Fast file search index using compact slab storage, arena paths, and sorted indexes
///
/// Optimized for minimal memory usage while preserving sub-millisecond search:
/// - CompactNode slab (24 bytes/node) + PathArena (contiguous paths)
/// - Sorted flat name/mtime indexes instead of BTreeMaps
/// - Roaring bitmaps for type filtering
#[derive(Debug, Clone)]
pub struct SearchIndex {
    /// Compact slab storage (24 bytes per node, no heap)
    slab: Vec<CompactNode>,

    /// Contiguous path storage
    path_arena: PathArena,

    /// Compact name index: sorted flat arrays
    name_index: CompactNameIndex,

    /// Type index: roaring bitmaps for ultra-fast type filtering
    type_index: TypeIndex,

    /// Compact mtime index: sorted flat arrays
    mtime_index: CompactMtimeIndex,

    /// Whether we've been finalized (compact indexes built from overflow)
    finalized: bool,

    /// Root paths this index was built from
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
            path_arena: PathArena::new(),
            name_index: CompactNameIndex::new(),
            type_index: TypeIndex::new(),
            mtime_index: CompactMtimeIndex::new(),
            finalized: false,
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
            path_arena: PathArena::with_capacity(capacity * 60),
            name_index: CompactNameIndex::new(),
            type_index: TypeIndex::new(),
            mtime_index: CompactMtimeIndex::new(),
            finalized: false,
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

    // ========================================================================
    // Internal helpers for CompactNode access
    // ========================================================================

    /// Get path for a slab index (zero-copy)
    #[inline]
    fn node_path(&self, idx: usize) -> &str {
        let node = &self.slab[idx];
        self.path_arena.get(node.path_offset, node.path_len)
    }

    /// Finalize compact indexes after bulk inserts.
    ///
    /// Moves overflow BTreeMaps into flat sorted compact arrays for better
    /// cache locality and lower memory usage. Called automatically at the end
    /// of `build_from_path_with_progress()` and after `load_index()`.
    pub fn finalize(&mut self) {
        if self.finalized {
            return;
        }
        // Build compact name index from overflow
        self.name_index = CompactNameIndex::build_from(&self.name_index.overflow);
        // Build compact mtime index from overflow
        let mtime_overflow: BTreeMap<u64, Vec<u32>> = {
            let mut map: BTreeMap<u64, Vec<u32>> = BTreeMap::new();
            for &(mtime, idx) in &self.mtime_index.overflow {
                map.entry(mtime).or_default().push(idx);
            }
            map
        };
        self.mtime_index = CompactMtimeIndex::build_from(&mtime_overflow);
        self.path_arena.shrink_to_fit();
        self.name_index.shrink_to_fit();
        self.mtime_index.shrink_to_fit();
        self.slab.shrink_to_fit();
        self.finalized = true;
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
    #[instrument(skip(self, progress), fields(root = %root.display()))]
    pub fn add_root_with_progress(
        &mut self,
        root: &Path,
        progress: Option<Arc<CrawlProgress>>,
    ) -> Result<(), IndexError> {
        let root_str = root.to_string_lossy().to_string();

        // Check if already indexed
        if self.has_root(&root_str) {
            tracing::info!(root = %root_str, "Root already indexed, skipping");
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

                    // Store ABSOLUTE path for multi-root support
                    let abs_path = entry_path.to_string_lossy().to_string();

                    // Skip entries with no filename (root directory)
                    if entry_path.file_name().is_none() {
                        continue;
                    }

                    if file_type.is_dir() {
                        // Index directory
                        let mtime = entry
                            .metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);

                        let node = FileNode::directory(abs_path, mtime);
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

                        let node = FileNode::file(abs_path, size, mtime);
                        self.insert(node);

                        if let Some(ref p) = progress {
                            p.add_file(size);
                        }
                    }
                    // Skip symlinks and special files
                }
                Err(e) => {
                    tracing::warn!(depth = e.depth(), "Walk error: {}", e);
                }
            }
        }

        // Finalize compact indexes
        self.finalize();

        Ok(())
    }

    /// Remove all entries from a specific root directory
    ///
    /// Returns the number of entries removed.
    #[instrument(skip(self), fields(root = %root.display()))]
    pub fn remove_root(&mut self, root: &Path) -> usize {
        let root_str = root.to_string_lossy().to_string();
        let root_prefix = if root_str.ends_with('/') {
            root_str.clone()
        } else {
            format!("{}/", root_str)
        };

        // Remove from roots list
        self.roots.retain(|r| r != &root_str);

        // Collect indices to remove
        let indices_to_remove: Vec<usize> = self
            .slab
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                let path = node.path(&self.path_arena);
                path.starts_with(&root_prefix) || path == root_str
            })
            .map(|(idx, _)| idx)
            .collect();

        let count = indices_to_remove.len();
        for idx in indices_to_remove {
            let path = self.node_path(idx).to_string();
            self.remove(&path);
        }

        count
    }

    /// Insert a file node into the index
    pub fn insert(&mut self, node: FileNode) {
        let index = self.slab.len();
        let name_lower = node.name().to_lowercase();
        let is_directory = node.is_directory();
        let extension = node.extension().map(|s| s.to_string());
        let mtime = node.mtime;

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

        // Push path into arena, create compact node
        let (path_offset, path_len) = self.path_arena.push(&node.path);
        let compact = CompactNode {
            path_offset,
            path_len,
            node_type: node.node_type,
            size: node.size,
            mtime: node.mtime,
        };
        self.slab.push(compact);

        // Always insert into overflow; finalize() compacts them
        let idx32 = index as u32;
        self.name_index.insert_overflow(name_lower, idx32);
        self.mtime_index.insert_overflow(mtime, idx32);

        // Add to type index for fast type filtering (also tracks trash)
        self.type_index.add_file(
            idx32,
            self.path_arena.get(path_offset, path_len),
            extension.as_deref(),
            is_directory,
        );
    }

    /// Clear the index
    pub fn clear(&mut self) {
        self.slab.clear();
        self.path_arena = PathArena::new();
        self.name_index.clear();
        self.type_index.clear();
        self.mtime_index.clear();
        self.finalized = false;
        self.roots.clear();
        self.file_count = 0;
        self.dir_count = 0;
        self.total_bytes = 0;
    }

    /// Consume this index and return materialized FileNodes.
    pub fn into_nodes(self) -> Vec<FileNode> {
        self.slab
            .iter()
            .map(|node| node.to_file_node(&self.path_arena))
            .collect()
    }

    /// Insert a batch of nodes. Equivalent to calling `insert()` per node
    /// but communicates intent and allows future batch optimizations.
    pub fn insert_batch(&mut self, nodes: Vec<FileNode>) {
        self.slab.reserve(nodes.len());
        for node in nodes {
            self.insert(node);
        }
    }

    /// Remove a file from the index by path
    ///
    /// Returns true if the file was found and removed, false otherwise.
    pub fn remove(&mut self, path: &str) -> bool {
        // Find the node by path
        let mut found_idx: Option<usize> = None;
        let mut found_name: Option<String> = None;

        for (idx, node) in self.slab.iter().enumerate() {
            if node.path(&self.path_arena) == path {
                found_idx = Some(idx);
                found_name = Some(node.name(&self.path_arena).to_lowercase());
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

        // Remove from overflow name index (main entries are immutable)
        self.name_index
            .remove_from_overflow(&name_lower, idx as u32);

        // Free the arena slot
        let node = &self.slab[idx];
        self.path_arena.remove(node.path_offset, node.path_len);

        // Remove from type index
        self.type_index.remove_file(idx as u32);

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

        // Collect indices to remove
        let indices_to_remove: Vec<usize> = self
            .slab
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                let path = node.path(&self.path_arena);
                path.starts_with(&prefix) || path == dir_path
            })
            .map(|(idx, _)| idx)
            .collect();

        let count = indices_to_remove.len();
        for idx in indices_to_remove {
            let path = self.node_path(idx).to_string();
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
    #[instrument(skip(self), fields(query = %q.text, limit = q.limit))]
    pub fn query(&self, q: SearchQuery) -> Vec<SearchResult> {
        match (q.text.is_empty(), &q.type_filter, &q.sort) {
            (_, _, SortBy::RecentFirst) => {
                if q.text.is_empty() {
                    self.search_recent(q.limit, q.type_filter)
                } else {
                    self.search_recent_with_query(&q.text, q.limit, q.type_filter)
                }
            }
            (true, Some(cat), _) => self.bitmap_search(*cat, q.limit, q.include_trash),
            (false, Some(cat), _) => self.text_with_bitmap(&q.text, *cat, q.limit, q.include_trash),
            (true, None, _) if q.extension.is_none() => self.list_all(q.limit),
            _ => {
                let opts = self.query_to_options(&q);
                self.search_with_options(&q.text, opts)
            }
        }
    }

    /// Type-only search via bitmap
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
                    node: node.to_file_node(&self.path_arena),
                    score: 100,
                })
            })
            .collect()
    }

    /// Text + type bitmap intersection search
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

        // Search main compact name index
        for entry in &self.name_index.entries {
            let name = self.name_index.entry_name(entry);
            if !name.contains(&query_lower) {
                continue;
            }

            let score = Self::calculate_score_static(name, &query_lower);

            for &idx in self.name_index.entry_indices(entry) {
                if !type_bitmap.contains(idx) {
                    continue;
                }
                if !include_trash && self.type_index.is_in_trash(idx) {
                    continue;
                }
                if let Some(node) = self.slab.get(idx as usize) {
                    results.push(SearchResult {
                        node: node.to_file_node(&self.path_arena),
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

        // Search overflow
        for (name, indices) in &self.name_index.overflow {
            if !name.contains(&query_lower) {
                continue;
            }
            let score = Self::calculate_score_static(name, &query_lower);
            for &idx in indices {
                if !type_bitmap.contains(idx) {
                    continue;
                }
                if !include_trash && self.type_index.is_in_trash(idx) {
                    continue;
                }
                if let Some(node) = self.slab.get(idx as usize) {
                    results.push(SearchResult {
                        node: node.to_file_node(&self.path_arena),
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

    fn query_to_options(&self, q: &SearchQuery) -> SearchOptions {
        let mut opts = SearchOptions::with_limit(q.limit);
        opts.include_dirs = q.include_dirs;
        opts.include_files = q.include_files;
        opts.include_trash = q.include_trash;
        opts.extension_filter = q.extension.clone();
        opts
    }

    /// Ultra-fast type-only search using roaring bitmaps
    pub fn search_by_type(&self, type_name: &str, limit: usize) -> Vec<SearchResult> {
        self.search_by_type_filtered(type_name, limit, true)
    }

    /// Ultra-fast type-only search with trash filtering
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
                    node: node.to_file_node(&self.path_arena),
                    score: 100,
                })
            })
            .collect()
    }

    /// Search by multiple types (union)
    pub fn search_by_types(&self, type_names: &[&str], limit: usize) -> Vec<SearchResult> {
        self.search_by_types_filtered(type_names, limit, true)
    }

    /// Search by multiple types with trash filtering
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

        let mut union = self.type_index.union_categories(&categories);

        if !include_trash {
            union -= self.type_index.trash_bitmap();
        }

        union
            .iter()
            .take(limit)
            .filter_map(|idx| {
                self.slab.get(idx as usize).map(|node| SearchResult {
                    node: node.to_file_node(&self.path_arena),
                    score: 100,
                })
            })
            .collect()
    }

    /// Combined search: text query + type filter
    pub fn search_with_type(
        &self,
        query: &str,
        type_name: &str,
        limit: usize,
    ) -> Vec<SearchResult> {
        self.search_with_type_filtered(query, type_name, limit, true)
    }

    /// Combined search: text query + type filter with trash filtering
    pub fn search_with_type_filtered(
        &self,
        query: &str,
        type_name: &str,
        limit: usize,
        include_trash: bool,
    ) -> Vec<SearchResult> {
        let Some(category) = FileTypeCategory::parse_str(type_name) else {
            let mut opts = SearchOptions::with_limit(limit);
            opts.include_trash = include_trash;
            return self.search_with_options(query, opts);
        };

        let Some(type_bitmap) = self.type_index.get_indices(category) else {
            return Vec::new();
        };

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        // Search through compact name index
        for entry in &self.name_index.entries {
            let name = self.name_index.entry_name(entry);
            if !name.contains(&query_lower) {
                continue;
            }
            let score = Self::calculate_score_static(name, &query_lower);
            for &idx in self.name_index.entry_indices(entry) {
                if !type_bitmap.contains(idx) {
                    continue;
                }
                if !include_trash && self.type_index.is_in_trash(idx) {
                    continue;
                }
                if let Some(node) = self.slab.get(idx as usize) {
                    results.push(SearchResult {
                        node: node.to_file_node(&self.path_arena),
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

        // Search overflow
        for (name, indices) in &self.name_index.overflow {
            if !name.contains(&query_lower) {
                continue;
            }
            let score = Self::calculate_score_static(name, &query_lower);
            for &idx in indices {
                if !type_bitmap.contains(idx) {
                    continue;
                }
                if !include_trash && self.type_index.is_in_trash(idx) {
                    continue;
                }
                if let Some(node) = self.slab.get(idx as usize) {
                    results.push(SearchResult {
                        node: node.to_file_node(&self.path_arena),
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

    /// Search for most recent files
    ///
    /// Returns the N most recently modified files, optionally filtered by type.
    pub fn search_recent(
        &self,
        limit: usize,
        type_filter: Option<FileTypeCategory>,
    ) -> Vec<SearchResult> {
        let mut results = Vec::with_capacity(limit);
        let type_bitmap = type_filter.and_then(|cat| self.type_index.get_indices(cat));

        // Iterate compact mtime groups from end (most recent first)
        for &(_, start, count) in self.mtime_index.groups.iter().rev() {
            let indices = self.mtime_index.group_indices(start, count);
            for &idx in indices.iter().rev() {
                if let Some(bitmap) = type_bitmap
                    && !bitmap.contains(idx)
                {
                    continue;
                }
                if let Some(node) = self.slab.get(idx as usize) {
                    if node.node_type == NodeType::Directory {
                        continue;
                    }
                    results.push(SearchResult {
                        node: node.to_file_node(&self.path_arena),
                        score: 100,
                    });
                    if results.len() >= limit {
                        return results;
                    }
                }
            }
        }

        // Also check overflow
        let mut overflow_sorted: Vec<_> = self.mtime_index.overflow.clone();
        overflow_sorted.sort_by_key(|b| std::cmp::Reverse(b.0));
        for (_, idx) in overflow_sorted {
            if let Some(bitmap) = type_bitmap
                && !bitmap.contains(idx)
            {
                continue;
            }
            if let Some(node) = self.slab.get(idx as usize) {
                if node.node_type == NodeType::Directory {
                    continue;
                }
                results.push(SearchResult {
                    node: node.to_file_node(&self.path_arena),
                    score: 100,
                });
                if results.len() >= limit {
                    return results;
                }
            }
        }

        results
    }

    /// Search for most recent files matching a query
    pub fn search_recent_with_query(
        &self,
        query: &str,
        limit: usize,
        type_filter: Option<FileTypeCategory>,
    ) -> Vec<SearchResult> {
        if query.is_empty() {
            return self.search_recent(limit, type_filter);
        }

        let query_lower = query.to_lowercase();
        let type_bitmap = type_filter.and_then(|cat| self.type_index.get_indices(cat));
        let mut results = Vec::new();

        for &(_, start, count) in self.mtime_index.groups.iter().rev() {
            let indices = self.mtime_index.group_indices(start, count);
            for &idx in indices.iter().rev() {
                if let Some(bitmap) = type_bitmap
                    && !bitmap.contains(idx)
                {
                    continue;
                }
                if let Some(node) = self.slab.get(idx as usize) {
                    if node.node_type == NodeType::Directory {
                        continue;
                    }
                    let name = node.name(&self.path_arena);
                    if !name.to_lowercase().contains(&query_lower) {
                        continue;
                    }
                    let score = Self::calculate_score_static(&name.to_lowercase(), &query_lower);
                    results.push(SearchResult {
                        node: node.to_file_node(&self.path_arena),
                        score,
                    });
                    if results.len() >= limit {
                        return results;
                    }
                }
            }
        }

        results
    }

    /// List all files in the index
    pub fn list_all(&self, limit: usize) -> Vec<SearchResult> {
        self.slab
            .iter()
            .take(limit)
            .map(|node| SearchResult {
                node: node.to_file_node(&self.path_arena),
                score: 50,
            })
            .collect()
    }

    /// Search with full options
    pub fn search_with_options(&self, query: &str, options: SearchOptions) -> Vec<SearchResult> {
        let query_lower = query.to_lowercase();
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
        let limit2 = options.limit * 2;

        // Search main compact name index
        for entry in &self.name_index.entries {
            let name = self.name_index.entry_name(entry);
            let indices = self.name_index.entry_indices(entry);
            self.search_name_entry(
                name,
                indices,
                query,
                &query_cmp,
                &options,
                &mut results,
                limit2,
            );
            if results.len() >= limit2 {
                break;
            }
        }

        // Search overflow
        if results.len() < limit2 {
            for (name, indices) in &self.name_index.overflow {
                self.search_name_entry(
                    name,
                    indices,
                    query,
                    &query_cmp,
                    &options,
                    &mut results,
                    limit2,
                );
                if results.len() >= limit2 {
                    break;
                }
            }
        }

        results.sort_by_key(|a| std::cmp::Reverse(a.score));
        results.truncate(options.limit);
        results
    }

    #[allow(clippy::too_many_arguments)]
    fn search_name_entry(
        &self,
        name: &str,
        indices: &[u32],
        query: &str,
        query_cmp: &str,
        options: &SearchOptions,
        results: &mut Vec<SearchResult>,
        limit2: usize,
    ) {
        if !query.is_empty() {
            let name_cmp = if options.case_sensitive {
                if let Some(&idx) = indices.first() {
                    if let Some(node) = self.slab.get(idx as usize) {
                        node.name(&self.path_arena).to_string()
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            } else {
                name.to_string()
            };
            if !name_cmp.contains(query_cmp) {
                return;
            }
        }

        let score = if query.is_empty() {
            100u32.saturating_sub(name.len() as u32)
        } else {
            let name_cmp = if options.case_sensitive {
                if let Some(&idx) = indices.first() {
                    if let Some(node) = self.slab.get(idx as usize) {
                        node.name(&self.path_arena).to_string()
                    } else {
                        name.to_string()
                    }
                } else {
                    name.to_string()
                }
            } else {
                name.to_string()
            };
            Self::calculate_score_static(&name_cmp, query_cmp)
        };

        for &idx in indices {
            if let Some(node) = self.slab.get(idx as usize) {
                if !self.matches_compact_filters(node, idx as usize, options) {
                    continue;
                }
                results.push(SearchResult {
                    node: node.to_file_node(&self.path_arena),
                    score,
                });
                if results.len() >= limit2 {
                    return;
                }
            }
        }
    }

    /// Check if a compact node matches filters
    fn matches_compact_filters(
        &self,
        node: &CompactNode,
        slab_index: usize,
        options: &SearchOptions,
    ) -> bool {
        if !options.include_trash && self.type_index.is_in_trash(slab_index as u32) {
            return false;
        }
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
                if !options.include_files {
                    return false;
                }
            }
        }
        if let Some(ref ext_filter) = options.extension_filter {
            if let Some(ext) = node.extension(&self.path_arena) {
                if ext.to_lowercase() != *ext_filter {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    /// Calculate relevance score (static, no &self needed)
    fn calculate_score_static(name: &str, query: &str) -> u32 {
        let mut score = 0u32;
        if name == query {
            score += 1000;
        } else if name.starts_with(query) {
            score += 500;
        } else {
            score += 100;
        }
        score += 100u32.saturating_sub(name.len() as u32);
        score
    }

    /// Get a node by its slab index (materializes to FileNode)
    pub fn get(&self, index: usize) -> Option<FileNode> {
        self.slab
            .get(index)
            .map(|node| node.to_file_node(&self.path_arena))
    }

    /// Iterate over all nodes (materializes each to FileNode)
    pub fn iter(&self) -> impl Iterator<Item = FileNode> + '_ {
        self.slab
            .iter()
            .map(|node| node.to_file_node(&self.path_arena))
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

        index.insert(FileNode::file("Documents/report.pdf".into(), 1024, 0));
        index.insert(FileNode::file("Documents/report_final.pdf".into(), 2048, 0));
        index.insert(FileNode::file("Documents/notes.txt".into(), 512, 0));

        assert_eq!(index.file_count(), 3);

        // Search for "report"
        let results = index.search("report", 10);
        assert_eq!(results.len(), 2);

        // "report.pdf" should score higher (shorter name)
        assert!(results[0].node.name() == "report.pdf");
    }

    #[test]
    fn test_case_insensitive_search() {
        let mut index = SearchIndex::new();

        index.insert(FileNode::file("README.md".into(), 100, 0));

        let results = index.search("readme", 10);
        assert_eq!(results.len(), 1);

        let results = index.search("README", 10);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_extension_filter() {
        let mut index = SearchIndex::new();

        index.insert(FileNode::file("doc.pdf".into(), 100, 0));
        index.insert(FileNode::file("doc.txt".into(), 100, 0));
        index.insert(FileNode::file("doc.md".into(), 100, 0));

        let options = SearchOptions::default().with_extension("pdf");
        let results = index.search_with_options("doc", options);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].node.name(), "doc.pdf");
    }

    #[test]
    fn test_files_only_filter() {
        let mut index = SearchIndex::new();

        index.insert(FileNode::file("docs.txt".into(), 100, 0));
        index.insert(FileNode::directory("docs".into(), 0));

        let options = SearchOptions::default().files_only();
        let results = index.search_with_options("docs", options);

        assert_eq!(results.len(), 1);
        assert!(results[0].node.is_file());
    }

    #[test]
    fn test_dirs_only_filter() {
        let mut index = SearchIndex::new();

        index.insert(FileNode::file("docs.txt".into(), 100, 0));
        index.insert(FileNode::directory("docs".into(), 0));

        let options = SearchOptions::default().dirs_only();
        let results = index.search_with_options("docs", options);

        assert_eq!(results.len(), 1);
        assert!(results[0].node.is_directory());
    }

    #[test]
    fn test_empty_query() {
        let mut index = SearchIndex::new();
        index.insert(FileNode::file("test.txt".into(), 100, 0));

        let results = index.search("", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_no_results() {
        let mut index = SearchIndex::new();
        index.insert(FileNode::file("test.txt".into(), 100, 0));

        let results = index.search("nonexistent", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_score_ranking() {
        let mut index = SearchIndex::new();

        // Exact match should rank highest
        index.insert(FileNode::file("test".into(), 100, 0));
        // Starts with should rank second
        index.insert(FileNode::file("testing".into(), 100, 0));
        // Contains should rank lowest
        index.insert(FileNode::file("mytest".into(), 100, 0));

        let results = index.search("test", 10);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].node.name(), "test"); // Exact match
        assert_eq!(results[1].node.name(), "testing"); // Starts with
        assert_eq!(results[2].node.name(), "mytest"); // Contains
    }

    #[test]
    fn test_limit() {
        let mut index = SearchIndex::new();

        for i in 0..100 {
            index.insert(FileNode::file(format!("file{}.txt", i), 100, 0));
        }

        let results = index.search("file", 10);
        assert_eq!(results.len(), 10);
    }

    #[test]
    fn test_clear() {
        let mut index = SearchIndex::new();
        index.insert(FileNode::file("test.txt".into(), 100, 0));

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
        index.insert(FileNode::file("project1/config.json".into(), 100, 0));
        index.insert(FileNode::file("project2/config.json".into(), 200, 0));

        let results = index.search("config", 10);
        assert_eq!(results.len(), 2);

        // Both should have same score
        assert_eq!(results[0].score, results[1].score);
    }
}
