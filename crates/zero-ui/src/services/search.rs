use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::*;

use zero::prelude::{
    IndexManager, SearchIndex, SearchResult, hash_path, save_index_via_etch,
};
use zero::scanner::CrawlProgress;

// -- Events ------------------------------------------------------------------

pub enum SearchEvent {
    IndexLoaded,
    IndexingStarted,
    IndexingFinished,
    IndexCleared,
    /// Live watcher detected changes; N files affected in rebuilt root(s).
    IndexUpdated(()),
}

impl EventEmitter<SearchEvent> for SearchService {}

// -- Service -----------------------------------------------------------------

pub struct SearchService {
    manager: IndexManager,
    roots: Vec<PathBuf>,
    loading: bool,
    indexing: bool,
    file_count: u64,
    watcher_active: bool,
}

impl SearchService {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let manager = IndexManager::new().unwrap_or_else(|_| {
            let tmp = std::env::temp_dir().join("zero-index");
            IndexManager::with_dir(tmp).expect("Failed to create index manager")
        });

        let mut svc = Self {
            manager,
            roots: Vec::new(),
            loading: false,
            indexing: false,
            file_count: 0,
            watcher_active: false,
        };
        svc.loading = true;
        svc.async_load(cx);
        svc
    }

    // -- Queries (synchronous, fast) ------------------------------------------

    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        self.manager.search(query, limit)
    }

    #[allow(dead_code)]
    pub fn search_by_type(
        &self,
        file_type: &str,
        limit: usize,
    ) -> Vec<SearchResult> {
        self.manager.search_by_type(file_type, limit)
    }

    #[allow(dead_code)]
    pub fn roots(&self) -> Vec<String> {
        self.manager.roots()
    }

    #[allow(dead_code)]
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn is_indexing(&self) -> bool {
        self.indexing
    }

    #[allow(dead_code)]
    pub fn file_count(&self) -> u64 {
        self.file_count
    }

    /// Provide read access to the IndexManager for operations that need it
    /// (e.g., cleanup scan which takes &IndexManager).
    #[allow(dead_code)]
    pub fn with_manager<R>(&self, f: impl FnOnce(&IndexManager) -> R) -> R {
        f(&self.manager)
    }

    /// Clone the IndexManager (for background work that needs a snapshot).
    pub fn clone_manager(&self) -> IndexManager {
        self.manager.clone()
    }

    // -- Mutations (async, background) ----------------------------------------

    pub fn rebuild(&mut self, settings_roots: Vec<PathBuf>, cx: &mut Context<Self>) -> Arc<CrawlProgress> {
        self.indexing = true;
        self.file_count = 0;
        cx.notify();

        let progress = Arc::new(CrawlProgress::new());
        cx.emit(SearchEvent::IndexingStarted);

        let roots_strings: Vec<String> = settings_roots
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        let p = progress.clone();
        cx.spawn(async move |this, cx| {
            // Get roots from the manager if settings_roots is empty
            let roots = if roots_strings.is_empty() {
                this.update(cx, |svc, _| svc.manager.roots()).unwrap_or_default()
            } else {
                roots_strings
            };

            if roots.is_empty() {
                eprintln!("[zero-ui] rebuild: no search roots configured");
                this.update(cx, |svc, cx| {
                    svc.indexing = false;
                    cx.emit(SearchEvent::IndexingFinished);
                    cx.notify();
                }).ok();
                return;
            }

            eprintln!("[zero-ui] rebuild: indexing {} root(s)", roots.len());

            for root in &roots {
                eprintln!("[zero-ui] rebuild: indexing {}", root);

                // Remove old index on main thread (fast HashMap remove)
                let indexes_dir = this.update(cx, |svc, _| {
                    svc.manager.remove_root(root);
                    svc.manager.indexes_dir().to_path_buf()
                });
                let Ok(indexes_dir) = indexes_dir else { continue };

                // Build index on background thread
                let root_clone = root.clone();
                let progress_clone = p.clone();
                let build_result = cx.background_executor().spawn(async move {
                    let root_path = std::path::Path::new(&root_clone);
                    let mut index = SearchIndex::new();
                    index.build_from_path_with_progress(root_path, Some(progress_clone))?;
                    let count = index.file_count();
                    let hash = hash_path(&root_clone);
                    let etch_dir = indexes_dir.join(&hash);
                    save_index_via_etch(&index, &etch_dir)?;
                    Ok::<_, zero::index::IndexError>((root_clone, index, count))
                }).await;

                // Apply result on main thread (fast memory insert)
                match build_result {
                    Ok((root_str, index, count)) => {
                        eprintln!("[zero-ui] rebuild: {} — {} files", root_str, count);
                        this.update(cx, |svc, _| {
                            svc.manager.insert_index_memory_only(&root_str, index, count);
                        }).ok();
                    }
                    Err(e) => {
                        eprintln!("[zero-ui] rebuild: {} — error: {}", root, e);
                    }
                }
            }

            eprintln!("[zero-ui] rebuild: complete");
            this.update(cx, |svc, cx| {
                svc.indexing = false;
                cx.emit(SearchEvent::IndexingFinished);
                cx.notify();
            }).ok();
        })
        .detach();

        progress
    }

    pub fn add_root(&mut self, path: &str, cx: &mut Context<Self>) {
        let path_owned = path.to_string();
        let indexes_dir = self.manager.indexes_dir().to_path_buf();

        self.indexing = true;
        cx.emit(SearchEvent::IndexingStarted);
        cx.notify();

        cx.spawn(async move |this, cx| {
            // Build index on background thread
            let build_result = cx.background_executor().spawn(async move {
                let root_path = std::path::Path::new(&path_owned);
                let mut index = SearchIndex::new();
                index.build_from_path_with_progress(root_path, None)?;
                let count = index.file_count();
                let hash = hash_path(&path_owned);
                let etch_dir = indexes_dir.join(&hash);
                save_index_via_etch(&index, &etch_dir)?;
                Ok::<_, zero::index::IndexError>((path_owned, index, count))
            }).await;

            // Apply result on main thread
            this.update(cx, |svc, cx| {
                match build_result {
                    Ok((root_str, index, count)) => {
                        eprintln!("[zero-ui] add_root: {} — {} files", root_str, count);
                        svc.manager.insert_index_memory_only(&root_str, index, count);
                    }
                    Err(e) => {
                        eprintln!("[zero-ui] add_root: error: {}", e);
                    }
                }
                svc.roots = svc.manager.roots().into_iter().map(PathBuf::from).collect();
                svc.indexing = false;
                cx.emit(SearchEvent::IndexingFinished);
                cx.notify();
            }).ok();
        }).detach();
    }

    pub fn remove_root(&mut self, path: &str, cx: &mut Context<Self>) {
        self.manager.remove_root(path);
        self.roots = self.manager.roots().into_iter().map(PathBuf::from).collect();
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.manager.clear();
        self.roots.clear();
        cx.emit(SearchEvent::IndexCleared);
        cx.notify();
    }

    // -- Internal -------------------------------------------------------------

    fn async_load(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            // Load from disk on background thread
            let loaded = cx
                .background_executor()
                .spawn(async { IndexManager::load().ok() })
                .await;

            if let Some(loaded) = loaded {
                this.update(cx, |svc, cx| {
                    let root_count = loaded.roots().len();
                    svc.manager = loaded;
                    svc.roots = svc.manager.roots().into_iter().map(PathBuf::from).collect();
                    svc.loading = false;
                    eprintln!("[zero-ui] index loaded: {} root(s)", root_count);
                    cx.emit(SearchEvent::IndexLoaded);
                    cx.notify();
                }).ok();
            } else {
                this.update(cx, |svc, cx| {
                    svc.loading = false;
                    cx.emit(SearchEvent::IndexLoaded);
                    cx.notify();
                }).ok();
            }

            // Check if we already have indexed roots
            let has_roots = this
                .update(cx, |svc, _| !svc.roots.is_empty())
                .unwrap_or(false);

            if has_roots {
                return;
            }

            // Auto-index home directory
            let Some(home) = dirs::home_dir() else {
                return;
            };

            let home_str = home.to_string_lossy().to_string();
            eprintln!("[zero-ui] no indexed roots, auto-indexing {}", home_str);

            let progress = Arc::new(CrawlProgress::new());
            let p = progress.clone();

            // Persist as a search root
            let mut settings = crate::session::Settings::load();
            if !settings.search_roots.contains(&home) {
                settings.search_roots.push(home);
                settings.save();
            }

            // Extract indexes_dir on main thread before going to background
            let indexes_dir = this.update(cx, |svc, cx| {
                svc.indexing = true;
                cx.emit(SearchEvent::IndexingStarted);
                cx.notify();
                svc.manager.indexes_dir().to_path_buf()
            });
            let Ok(indexes_dir) = indexes_dir else { return };

            // Build index on background thread
            let home_clone = home_str.clone();
            let build_result = cx.background_executor().spawn(async move {
                let root_path = std::path::Path::new(&home_clone);
                let mut index = SearchIndex::new();
                index.build_from_path_with_progress(root_path, Some(p))?;
                let count = index.file_count();
                let hash = hash_path(&home_clone);
                let etch_dir = indexes_dir.join(&hash);
                save_index_via_etch(&index, &etch_dir)?;
                Ok::<_, zero::index::IndexError>((home_clone, index, count))
            }).await;

            // Apply result on main thread
            match build_result {
                Ok((root_str, index, count)) => {
                    eprintln!("[zero-ui] auto-index complete: {} files", count);
                    this.update(cx, |svc, _| {
                        svc.manager.insert_index_memory_only(&root_str, index, count);
                        svc.roots = svc.manager.roots().into_iter().map(PathBuf::from).collect();
                    }).ok();
                }
                Err(e) => {
                    eprintln!("[zero-ui] auto-index error: {}", e);
                }
            }

            this.update(cx, |svc, cx| {
                svc.indexing = false;
                cx.emit(SearchEvent::IndexingFinished);
                cx.notify();
            }).ok();
        })
        .detach();
    }

    /// Start a background file watcher on all indexed roots.
    ///
    /// Polls `FileWatcher::try_next_event()` every 500ms. When changes
    /// accumulate, waits 2s for a quiet period then rebuilds affected roots.
    pub fn start_watcher(&mut self, cx: &mut Context<Self>) {
        if self.watcher_active {
            return;
        }
        self.watcher_active = true;

        let roots: Vec<PathBuf> = self.roots.clone();
        if roots.is_empty() {
            self.watcher_active = false;
            return;
        }

        cx.spawn(async move |this, cx| {
            // Create watcher on the background executor (blocking init)
            let watcher_result = cx
                .background_executor()
                .spawn(async move {
                    let mut watcher = match zero_watcher::FileWatcher::new() {
                        Ok(w) => w,
                        Err(e) => return Err(format!("Failed to create watcher: {e}")),
                    };
                    for root in &roots {
                        if let Err(e) = watcher.watch(root) {
                            eprintln!("[zero-ui] watcher: failed to watch {}: {e}", root.display());
                        }
                    }
                    Ok(watcher)
                })
                .await;

            let mut watcher = match watcher_result {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("[zero-ui] watcher: {e}");
                    this.update(cx, |svc, _| svc.watcher_active = false).ok();
                    return;
                }
            };

            eprintln!("[zero-ui] watcher: started");

            // Collect changed roots, rebuild after quiet period
            let mut dirty_roots: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut last_event_time: Option<std::time::Instant> = None;

            loop {
                // Poll every 500ms
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;

                // Drain all pending events
                while let Some(event) = watcher.try_next_event() {
                    let root_str = event.watch_root.to_string_lossy().to_string();
                    dirty_roots.insert(root_str);
                    last_event_time = Some(std::time::Instant::now());
                }

                // If we have dirty roots and 2s have passed since last event, rebuild
                if !dirty_roots.is_empty() {
                    let elapsed = last_event_time
                        .map(|t| t.elapsed())
                        .unwrap_or(Duration::ZERO);

                    if elapsed >= Duration::from_secs(2) {
                        let roots_to_rebuild: Vec<String> = dirty_roots.drain().collect();
                        last_event_time = None;

                        eprintln!(
                            "[zero-ui] watcher: rebuilding {} root(s)",
                            roots_to_rebuild.len()
                        );

                        for root in &roots_to_rebuild {
                            // Remove old index on main thread (fast)
                            let indexes_dir = this.update(cx, |svc, _| {
                                svc.manager.remove_root(root);
                                svc.manager.indexes_dir().to_path_buf()
                            });
                            let Ok(indexes_dir) = indexes_dir else { continue };

                            // Build on background thread
                            let root_clone = root.clone();
                            let build_result = cx.background_executor().spawn(async move {
                                let root_path = std::path::Path::new(&root_clone);
                                let mut index = SearchIndex::new();
                                index.build_from_path_with_progress(root_path, None)?;
                                let count = index.file_count();
                                let hash = hash_path(&root_clone);
                                let etch_dir = indexes_dir.join(&hash);
                                save_index_via_etch(&index, &etch_dir)?;
                                Ok::<_, zero::index::IndexError>((root_clone, index, count))
                            }).await;

                            // Apply on main thread
                            match build_result {
                                Ok((root_str, index, count)) => {
                                    this.update(cx, |svc, _| {
                                        svc.manager.insert_index_memory_only(&root_str, index, count);
                                    }).ok();
                                }
                                Err(e) => {
                                    eprintln!("[zero-ui] watcher: rebuild {} — error: {}", root, e);
                                }
                            }
                        }

                        this.update(cx, |svc, cx| {
                            svc.roots = svc.manager.roots().into_iter().map(PathBuf::from).collect();
                            cx.emit(SearchEvent::IndexUpdated(()));
                            cx.notify();
                        })
                        .ok();
                    }
                }
            }
        })
        .detach();
    }
}
