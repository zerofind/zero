//! FSEvents watcher integration for search index
//!
//! This module provides real-time index updates by watching for file system
//! changes. It uses the `zero_watcher` crate for the core file watching
//! functionality and applies changes to the search index.
//!
//! ## Usage
//!
//! ```no_run
//! use std::sync::Arc;
//! use std::path::Path;
//! use zero::index::{SearchIndex, IndexWatcher};
//!
//! // Build initial index
//! let mut index = SearchIndex::new();
//! index.build_from_path(Path::new("/Users/me/Documents")).unwrap();
//! let index = Arc::new(std::sync::RwLock::new(index));
//!
//! // Start watching for changes
//! let mut watcher = IndexWatcher::new(Arc::clone(&index)).unwrap();
//! watcher.watch(Path::new("/Users/me/Documents")).unwrap();
//!
//! // Process events (call periodically)
//! watcher.process_pending_events();
//! ```

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use zero_watcher::{FileChangeKind, FileEvent, FileWatchConfig, FileWatcher};

use super::node::{FileNode, NodeType};
use super::search::SearchIndex;

/// Configuration for the index watcher
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Debounce duration in milliseconds
    pub debounce_ms: u64,
    /// Patterns to ignore (e.g., ".DS_Store", ".git")
    pub ignore_patterns: Vec<String>,
    /// Whether to watch recursively
    pub recursive: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 100,
            ignore_patterns: vec![
                ".DS_Store".to_string(),
                ".git".to_string(),
                ".svn".to_string(),
                "node_modules".to_string(),
                ".Spotlight-V100".to_string(),
                ".fseventsd".to_string(),
                ".Trashes".to_string(),
                "*.swp".to_string(),
                "*.tmp".to_string(),
                "*~".to_string(),
                ".zero".to_string(),
            ],
            recursive: true,
        }
    }
}

impl From<WatcherConfig> for FileWatchConfig {
    fn from(config: WatcherConfig) -> Self {
        FileWatchConfig {
            debounce_ms: config.debounce_ms,
            recursive: config.recursive,
            ignore_patterns: config.ignore_patterns,
        }
    }
}

/// Statistics about index watcher activity
#[derive(Debug, Default, Clone)]
pub struct WatcherStats {
    /// Number of events received
    pub events_received: u64,
    /// Number of events processed (after filtering)
    pub events_processed: u64,
    /// Number of files added to index
    pub files_added: u64,
    /// Number of files removed from index
    pub files_removed: u64,
    /// Number of files updated in index
    pub files_updated: u64,
    /// Number of directories added
    pub dirs_added: u64,
    /// Number of directories removed
    pub dirs_removed: u64,
}

/// Real-time index watcher using the zero_watcher crate
///
/// This wraps the core FileWatcher and applies file system events
/// to the search index.
pub struct IndexWatcher {
    /// The underlying file watcher from zero_watcher crate
    watcher: FileWatcher,
    /// Shared index to update
    index: Arc<RwLock<SearchIndex>>,
    /// Root paths being watched
    watched_roots: Vec<PathBuf>,
    /// Statistics
    stats: WatcherStats,
}

impl IndexWatcher {
    /// Create a new index watcher
    pub fn new(index: Arc<RwLock<SearchIndex>>) -> Result<Self, WatcherError> {
        Self::with_config(index, WatcherConfig::default())
    }

    /// Create a new index watcher with custom configuration
    pub fn with_config(
        index: Arc<RwLock<SearchIndex>>,
        config: WatcherConfig,
    ) -> Result<Self, WatcherError> {
        let file_config: FileWatchConfig = config.into();
        let watcher =
            FileWatcher::with_config(file_config).map_err(|e| WatcherError::Init(e.to_string()))?;

        Ok(Self {
            watcher,
            index,
            watched_roots: Vec::new(),
            stats: WatcherStats::default(),
        })
    }

    /// Start watching a path
    pub fn watch(&mut self, path: &Path) -> Result<(), WatcherError> {
        self.watcher
            .watch(path)
            .map_err(|e| WatcherError::Watch(e.to_string()))?;

        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.watched_roots.push(canonical);

        Ok(())
    }

    /// Stop watching a path
    pub fn unwatch(&mut self, path: &Path) -> Result<(), WatcherError> {
        self.watcher
            .unwatch(path)
            .map_err(|e| WatcherError::Watch(e.to_string()))?;

        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.watched_roots.retain(|p| p != &canonical);

        Ok(())
    }

    /// Get current statistics
    pub fn stats(&self) -> &WatcherStats {
        &self.stats
    }

    /// Get watched roots
    pub fn watched_roots(&self) -> &[PathBuf] {
        &self.watched_roots
    }

    /// Process all pending events (non-blocking)
    ///
    /// Call this periodically to apply file system changes to the index.
    /// Returns the number of events processed.
    pub fn process_pending_events(&mut self) -> usize {
        let mut processed = 0;

        while let Some(event) = self.watcher.try_next_event() {
            self.stats.events_received += 1;
            self.apply_event(&event);
            processed += 1;
        }

        processed
    }

    /// Process events with a timeout (blocking)
    ///
    /// Blocks until an event is received or the timeout expires.
    /// Returns the number of events processed.
    pub fn process_events_timeout(&mut self, timeout: std::time::Duration) -> usize {
        let mut processed = 0;

        // First, drain any pending events
        while let Some(event) = self.watcher.try_next_event() {
            self.stats.events_received += 1;
            self.apply_event(&event);
            processed += 1;
        }

        // Then wait for one event with timeout
        if let Some(event) = self.watcher.next_event_timeout(timeout) {
            self.stats.events_received += 1;
            self.apply_event(&event);
            processed += 1;

            // And drain any more that came in
            while let Some(event) = self.watcher.try_next_event() {
                self.stats.events_received += 1;
                self.apply_event(&event);
                processed += 1;
            }
        }

        processed
    }

    /// Apply a single file event to the index
    fn apply_event(&mut self, event: &FileEvent) {
        // Get write lock on index
        let mut index = match self.index.write() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!("Failed to acquire index write lock: {}", e);
                return;
            }
        };

        self.stats.events_processed += 1;

        match event.kind {
            FileChangeKind::Created => {
                for path in &event.paths {
                    if let Some(node) = Self::create_node_from_path(path) {
                        match node.node_type {
                            NodeType::File => self.stats.files_added += 1,
                            NodeType::Directory => self.stats.dirs_added += 1,
                            _ => {}
                        }
                        index.insert(node);
                    }
                }
            }
            FileChangeKind::Modified => {
                for path in &event.paths {
                    // For modifications, remove old and add new
                    let path_str = path.to_string_lossy().to_string();
                    index.remove(&path_str);

                    if let Some(node) = Self::create_node_from_path(path) {
                        self.stats.files_updated += 1;
                        index.insert(node);
                    }
                }
            }
            FileChangeKind::Deleted => {
                for path in &event.paths {
                    let path_str = path.to_string_lossy().to_string();

                    // Check if it was a directory (might not exist anymore)
                    // Try to remove as both file and directory
                    if index.remove(&path_str) {
                        self.stats.files_removed += 1;
                    } else {
                        // Try as directory
                        let removed = index.remove_dir(&path_str);
                        if removed > 0 {
                            self.stats.dirs_removed += 1;
                            self.stats.files_removed += removed.saturating_sub(1) as u64;
                        }
                    }
                }
            }
            FileChangeKind::Renamed => {
                // Rename events typically have two paths: [old, new]
                if event.paths.len() >= 2 {
                    let old_path = &event.paths[0];
                    let new_path = &event.paths[1];

                    // Remove old
                    let old_str = old_path.to_string_lossy().to_string();
                    index.remove(&old_str);

                    // Add new
                    if let Some(node) = Self::create_node_from_path(new_path) {
                        self.stats.files_updated += 1;
                        index.insert(node);
                    }
                } else {
                    // Single path rename (treated as delete + potential create)
                    for path in &event.paths {
                        if path.exists() {
                            // File was renamed TO this path
                            if let Some(node) = Self::create_node_from_path(path) {
                                self.stats.files_added += 1;
                                index.insert(node);
                            }
                        } else {
                            // File was renamed FROM this path (deleted)
                            let path_str = path.to_string_lossy().to_string();
                            if index.remove(&path_str) {
                                self.stats.files_removed += 1;
                            }
                        }
                    }
                }
            }
            FileChangeKind::Metadata | FileChangeKind::Other => {
                // Ignore metadata-only changes and other events
            }
        }
    }

    /// Create a FileNode from a filesystem path
    fn create_node_from_path(path: &Path) -> Option<FileNode> {
        let metadata = match path.metadata() {
            Ok(m) => m,
            Err(_) => return None,
        };

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if name.is_empty() {
            return None;
        }

        let path_str = path.to_string_lossy().to_string();

        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if metadata.is_dir() {
            Some(FileNode::directory(name, path_str, mtime))
        } else if metadata.is_file() {
            Some(FileNode::file(name, path_str, metadata.len(), mtime))
        } else {
            // Skip symlinks and special files
            None
        }
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = WatcherStats::default();
    }
}

/// Errors from the index watcher
#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("Failed to initialize watcher: {0}")]
    Init(String),

    #[error("Watch error: {0}")]
    Watch(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_watcher_config_default() {
        let config = WatcherConfig::default();
        assert_eq!(config.debounce_ms, 100);
        assert!(config.recursive);
        assert!(!config.ignore_patterns.is_empty());
    }

    #[test]
    fn test_watcher_config_to_file_watch_config() {
        let config = WatcherConfig {
            debounce_ms: 200,
            recursive: false,
            ignore_patterns: vec!["*.log".to_string()],
        };

        let file_config: FileWatchConfig = config.into();
        assert_eq!(file_config.debounce_ms, 200);
        assert!(!file_config.recursive);
        assert_eq!(file_config.ignore_patterns, vec!["*.log".to_string()]);
    }

    #[test]
    fn test_create_node_from_path_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"hello world").unwrap();
        drop(file);

        let node = IndexWatcher::create_node_from_path(&file_path).unwrap();
        assert_eq!(node.name, "test.txt");
        assert!(node.is_file());
        assert_eq!(node.size, 11);
    }

    #[test]
    fn test_create_node_from_path_directory() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("subdir");
        fs::create_dir(&dir_path).unwrap();

        let node = IndexWatcher::create_node_from_path(&dir_path).unwrap();
        assert_eq!(node.name, "subdir");
        assert!(node.is_directory());
    }

    #[test]
    fn test_create_node_from_nonexistent() {
        let node = IndexWatcher::create_node_from_path(Path::new("/nonexistent/path"));
        assert!(node.is_none());
    }

    #[test]
    fn test_watcher_stats_default() {
        let stats = WatcherStats::default();
        assert_eq!(stats.events_received, 0);
        assert_eq!(stats.events_processed, 0);
        assert_eq!(stats.files_added, 0);
    }

    #[test]
    fn test_index_watcher_creation() {
        let index = Arc::new(RwLock::new(SearchIndex::new()));
        let watcher = IndexWatcher::new(index);
        assert!(watcher.is_ok());
    }

    #[test]
    fn test_index_watcher_with_config() {
        let index = Arc::new(RwLock::new(SearchIndex::new()));
        let config = WatcherConfig {
            debounce_ms: 50,
            recursive: false,
            ignore_patterns: vec![],
        };
        let watcher = IndexWatcher::with_config(index, config);
        assert!(watcher.is_ok());
    }
}
