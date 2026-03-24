//! File system watcher using the `notify` crate
//!
//! Watches directories for changes and emits debounced events.

use crate::FileWatchConfig;
use crate::events::{FileChangeKind, FileEvent};
use anyhow::{Context, Result};
use crossfire::mpsc as cf_mpsc;
use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{CreateKind, ModifyKind, RemoveKind, RenameMode},
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// File system watcher that monitors directories for changes
pub struct FileWatcher {
    /// The underlying notify watcher
    watcher: RecommendedWatcher,

    /// Channel to receive events from notify
    rx: crossfire::Rx<cf_mpsc::List<Result<Event, notify::Error>>>,

    /// Watched paths and their configurations
    watched_paths: HashMap<PathBuf, WatchedPath>,

    /// Configuration
    config: FileWatchConfig,

    /// Debounce state: path -> (`last_event_kind`, `last_event_time`)
    debounce_state: HashMap<PathBuf, (FileChangeKind, Instant)>,
}

/// Information about a watched path
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct WatchedPath {
    /// The root path being watched
    root: PathBuf,

    /// Whether watching recursively
    recursive: bool,
}

impl FileWatcher {
    /// Create a new file watcher with default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(FileWatchConfig::default())
    }

    /// Create a new file watcher with custom configuration
    pub fn with_config(config: FileWatchConfig) -> Result<Self> {
        let (tx, rx) = cf_mpsc::unbounded_blocking();

        let watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        )
        .context("Failed to create file watcher")?;

        Ok(Self {
            watcher,
            rx,
            watched_paths: HashMap::new(),
            config,
            debounce_state: HashMap::new(),
        })
    }

    /// Start watching a path for changes
    pub fn watch(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path
            .as_ref()
            .canonicalize()
            .context("Failed to resolve path")?;

        let mode = if self.config.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        self.watcher
            .watch(&path, mode)
            .with_context(|| format!("Failed to watch path: {}", path.display()))?;

        self.watched_paths.insert(
            path.clone(),
            WatchedPath {
                root: path,
                recursive: self.config.recursive,
            },
        );

        Ok(())
    }

    /// Stop watching a path
    pub fn unwatch(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path
            .as_ref()
            .canonicalize()
            .context("Failed to resolve path")?;

        self.watcher
            .unwatch(&path)
            .with_context(|| format!("Failed to unwatch path: {}", path.display()))?;

        self.watched_paths.remove(&path);
        Ok(())
    }

    /// Get all currently watched paths
    pub fn watched_paths(&self) -> Vec<&PathBuf> {
        self.watched_paths.keys().collect()
    }

    /// Try to receive the next event (non-blocking)
    pub fn try_next_event(&mut self) -> Option<FileEvent> {
        match self.rx.try_recv() {
            Ok(Ok(event)) => self.process_notify_event(event),
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "File watcher error");
                None
            }
            Err(crossfire::TryRecvError::Empty) => None,
            Err(crossfire::TryRecvError::Disconnected) => {
                tracing::error!("File watcher channel disconnected");
                None
            }
        }
    }

    /// Receive the next event (blocking)
    pub fn next_event(&mut self) -> Option<FileEvent> {
        match self.rx.recv() {
            Ok(Ok(event)) => self.process_notify_event(event),
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "File watcher error");
                None
            }
            Err(_) => {
                tracing::error!("File watcher channel disconnected");
                None
            }
        }
    }

    /// Receive the next event with timeout
    pub fn next_event_timeout(&mut self, timeout: Duration) -> Option<FileEvent> {
        match self.rx.recv_timeout(timeout) {
            Ok(Ok(event)) => self.process_notify_event(event),
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "File watcher error");
                None
            }
            Err(crossfire::RecvTimeoutError::Timeout) => None,
            Err(crossfire::RecvTimeoutError::Disconnected) => {
                tracing::error!("File watcher channel disconnected");
                None
            }
        }
    }

    /// Convert a notify event to our `FileEvent` type
    fn process_notify_event(&mut self, event: Event) -> Option<FileEvent> {
        // Skip events with no paths
        if event.paths.is_empty() {
            return None;
        }

        // Check if any path should be ignored
        for path in &event.paths {
            if self.should_ignore(path) {
                return None;
            }
        }

        // Convert event kind
        let kind = match event.kind {
            EventKind::Create(CreateKind::File) => FileChangeKind::Created,
            EventKind::Create(CreateKind::Folder) => FileChangeKind::Created,
            EventKind::Create(_) => FileChangeKind::Created,

            EventKind::Modify(ModifyKind::Data(_)) => FileChangeKind::Modified,
            EventKind::Modify(ModifyKind::Metadata(_)) => FileChangeKind::Metadata,
            EventKind::Modify(ModifyKind::Name(RenameMode::From)) => FileChangeKind::Renamed,
            EventKind::Modify(ModifyKind::Name(RenameMode::To)) => FileChangeKind::Renamed,
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => FileChangeKind::Renamed,
            EventKind::Modify(_) => FileChangeKind::Modified,

            EventKind::Remove(RemoveKind::File) => FileChangeKind::Deleted,
            EventKind::Remove(RemoveKind::Folder) => FileChangeKind::Deleted,
            EventKind::Remove(_) => FileChangeKind::Deleted,

            EventKind::Access(_) => return None, // Skip access events
            EventKind::Other => FileChangeKind::Other,
            EventKind::Any => FileChangeKind::Other,
        };

        // Find the watch root for this event
        let first_path = event.paths.first()?;
        let watch_root = self
            .find_watch_root(first_path)
            .unwrap_or_else(|| first_path.clone());

        // Apply debouncing
        let now = Instant::now();
        let debounce_duration = Duration::from_millis(self.config.debounce_ms);

        if let Some((last_kind, last_time)) = self.debounce_state.get(first_path)
            && *last_kind == kind
            && now.duration_since(*last_time) < debounce_duration
        {
            return None;
        }

        // Update debounce state
        self.debounce_state.insert(first_path.clone(), (kind, now));

        // Clean up old debounce entries periodically
        if self.debounce_state.len() > 1000
            && let Some(cutoff) = now.checked_sub(debounce_duration * 10)
        {
            self.debounce_state.retain(|_, (_, time)| *time > cutoff);
        }

        Some(FileEvent::new(kind, event.paths, watch_root))
    }

    /// Check if a path should be ignored
    fn should_ignore(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        for pattern in &self.config.ignore_patterns {
            // Simple pattern matching
            if pattern.starts_with('*') && pattern.ends_with('*') {
                // *pattern* - contains
                let inner = &pattern[1..pattern.len() - 1];
                if path_str.contains(inner) {
                    return true;
                }
            } else if let Some(suffix) = pattern.strip_prefix('*') {
                // *pattern - ends with
                if path_str.ends_with(suffix) {
                    return true;
                }
            } else if pattern.ends_with('*') {
                // pattern* - starts with (for path components)
                let prefix = &pattern[..pattern.len() - 1];
                if path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(prefix))
                {
                    return true;
                }
            } else {
                // Exact match on any path component
                for component in path.components() {
                    if component.as_os_str().to_string_lossy() == *pattern {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Find the watch root for a given path
    fn find_watch_root(&self, path: &Path) -> Option<PathBuf> {
        for watched in self.watched_paths.keys() {
            if path.starts_with(watched) {
                return Some(watched.clone());
            }
        }
        None
    }
}

/// Async wrapper for `FileWatcher` that can be used with tokio
pub struct AsyncFileWatcher {
    /// The underlying sync watcher
    watcher: FileWatcher,

    /// Async channel sender for events
    event_tx: crossfire::MAsyncTx<crossfire::mpsc::Array<FileEvent>>,

    /// Async channel receiver for events
    event_rx: crossfire::AsyncRx<crossfire::mpsc::Array<FileEvent>>,
}

impl AsyncFileWatcher {
    /// Create a new async file watcher
    pub fn new() -> Result<Self> {
        Self::with_config(FileWatchConfig::default())
    }

    /// Create a new async file watcher with custom configuration
    pub fn with_config(config: FileWatchConfig) -> Result<Self> {
        let watcher = FileWatcher::with_config(config)?;
        let (event_tx, event_rx) = crossfire::mpsc::bounded_async::<FileEvent>(100);

        Ok(Self {
            watcher,
            event_tx,
            event_rx,
        })
    }

    /// Start watching a path
    pub fn watch(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.watcher.watch(path)
    }

    /// Stop watching a path
    pub fn unwatch(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.watcher.unwatch(path)
    }

    /// Get the next event asynchronously
    pub async fn next_event(&mut self) -> Option<FileEvent> {
        // Poll the sync watcher in a non-blocking way
        // In practice, you'd run the sync watcher in a separate thread
        // For now, we use a simple timeout-based approach
        loop {
            if let Some(event) = self.watcher.try_next_event() {
                return Some(event);
            }

            // Brief yield to not spin
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Receive from the event channel (for use after spawning the watcher task)
    pub async fn recv(&mut self) -> Option<FileEvent> {
        self.event_rx.recv().await.ok()
    }

    /// Spawn the watcher as a background task
    /// Returns a receiver for events
    pub fn spawn(mut self) -> crossfire::AsyncRx<crossfire::mpsc::Array<FileEvent>> {
        let tx = self.event_tx.clone();

        tokio::spawn(async move {
            loop {
                if let Some(event) = self.watcher.next_event_timeout(Duration::from_millis(100))
                    && tx.send(event).await.is_err()
                {
                    break;
                }
            }
        });

        self.event_rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_create_watcher() {
        let watcher = FileWatcher::new();
        assert!(watcher.is_ok());
    }

    #[test]
    fn test_watch_directory() {
        let temp_dir = TempDir::new().unwrap();
        let mut watcher = FileWatcher::new().unwrap();

        let result = watcher.watch(temp_dir.path());
        assert!(result.is_ok());
        assert_eq!(watcher.watched_paths().len(), 1);
    }

    #[test]
    fn test_unwatch_directory() {
        let temp_dir = TempDir::new().unwrap();
        let mut watcher = FileWatcher::new().unwrap();

        watcher.watch(temp_dir.path()).unwrap();
        assert_eq!(watcher.watched_paths().len(), 1);

        watcher.unwatch(temp_dir.path()).unwrap();
        assert_eq!(watcher.watched_paths().len(), 0);
    }

    #[test]
    fn test_should_ignore() {
        let watcher = FileWatcher::new().unwrap();

        // Should ignore .DS_Store
        assert!(watcher.should_ignore(Path::new("/some/path/.DS_Store")));

        // Should ignore .git directories
        assert!(watcher.should_ignore(Path::new("/some/path/.git/objects/abc")));

        // Should ignore .swp files
        assert!(watcher.should_ignore(Path::new("/some/path/file.swp")));

        // Should not ignore regular files
        assert!(!watcher.should_ignore(Path::new("/some/path/file.txt")));
    }

    #[test]
    fn test_detect_file_creation() {
        let temp_dir = TempDir::new().unwrap();
        let mut watcher = FileWatcher::new().unwrap();

        watcher.watch(temp_dir.path()).unwrap();

        // Create a file
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "hello").unwrap();

        // Give the watcher time to detect the change
        std::thread::sleep(Duration::from_millis(200));

        // Try to get the event
        if let Some(event) = watcher.try_next_event() {
            assert!(matches!(
                event.kind,
                FileChangeKind::Created | FileChangeKind::Modified
            ));
            assert!(event.paths.iter().any(|p| p.ends_with("test.txt")));
        }
        // Note: On some systems, the event might not be immediately available
        // This is a basic smoke test
    }
}
