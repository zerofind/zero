use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::*;

use zero::index::{load_index_snapshot, persistence};
use zero::prelude::{IndexManager, SearchIndex, SearchResult, hash_path};
use zero::scanner::CrawlProgress;

// -- Events ------------------------------------------------------------------

pub enum SearchEvent {
    IndexLoaded,
    /// A single root's index finished loading from disk.
    RootLoaded {
        root: String,
        file_count: usize,
    },
    IndexingStarted {
        progress: Arc<CrawlProgress>,
        path: String,
    },
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
            watcher_active: false,
        };
        svc.loading = true;
        svc.async_load(cx);
        svc
    }

    /// Create without loading indexes or touching the filesystem.
    /// Call `activate()` once FDA is confirmed.
    pub fn new_deferred(_cx: &mut Context<Self>) -> Self {
        let manager = IndexManager::new().unwrap_or_else(|_| {
            let tmp = std::env::temp_dir().join("zero-index");
            IndexManager::with_dir(tmp).expect("Failed to create index manager")
        });

        Self {
            manager,
            roots: Vec::new(),
            loading: false,
            indexing: false,
            watcher_active: false,
        }
    }

    /// Start loading indexes after FDA is confirmed.
    pub fn activate(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        self.async_load(cx);
    }

    // -- Queries (synchronous, fast) ------------------------------------------

    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        tracing::debug!(query, limit, "search_svc: query");
        self.manager.search(query, limit)
    }

    pub fn search_by_type(&self, file_type: &str, limit: usize) -> Vec<SearchResult> {
        tracing::debug!(file_type, limit, "search_svc: query by type");
        self.manager.search_by_type(file_type, limit)
    }

    pub fn search_with_type(
        &self,
        query: &str,
        type_filter: &str,
        limit: usize,
    ) -> Vec<SearchResult> {
        tracing::debug!(query, type_filter, limit, "search_svc: query with type");
        self.manager.search_with_type(query, type_filter, limit)
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

    pub fn file_count(&self) -> u64 {
        self.manager.total_file_count() as u64
    }

    /// Number of in-memory indexes currently loaded.
    pub fn indexes_count(&self) -> usize {
        self.manager.indexes_count()
    }

    /// Total number of registered roots (including not-yet-loaded).
    pub fn roots_count(&self) -> usize {
        self.manager.roots_count()
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

    pub fn rebuild(
        &mut self,
        settings_roots: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Arc<CrawlProgress> {
        tracing::info!(roots = settings_roots.len(), "rebuild requested");
        self.indexing = true;
        cx.notify();

        let progress = Arc::new(CrawlProgress::new());

        let roots_strings: Vec<String> = settings_roots
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        let path_label = if roots_strings.len() == 1 {
            roots_strings[0].clone()
        } else if roots_strings.is_empty() {
            "search roots".to_string()
        } else {
            format!("{} roots", roots_strings.len())
        };
        cx.emit(SearchEvent::IndexingStarted {
            progress: progress.clone(),
            path: path_label,
        });

        let p = progress.clone();
        cx.spawn(async move |this, cx| {
            // Get roots from the manager if settings_roots is empty
            let roots = if roots_strings.is_empty() {
                this.update(cx, |svc, _| svc.manager.roots())
                    .unwrap_or_default()
            } else {
                roots_strings
            };

            if roots.is_empty() {
                tracing::warn!("rebuild: no search roots configured");
                this.update(cx, |svc, cx| {
                    svc.indexing = false;
                    cx.emit(SearchEvent::IndexingFinished);
                    cx.notify();
                })
                .ok();
                return;
            }

            tracing::info!(roots = roots.len(), "rebuild: indexing");

            for root in &roots {
                tracing::debug!(root = %root, "rebuild: indexing root");

                // Remove old index on main thread (fast HashMap remove)
                let indexes_dir = this.update(cx, |svc, _| {
                    svc.manager.remove_root(root);
                    svc.manager.indexes_dir().to_path_buf()
                });
                let Ok(indexes_dir) = indexes_dir else {
                    continue;
                };

                // Build index on background thread
                let root_clone = root.clone();
                let progress_clone = p.clone();
                let build_result = cx
                    .background_executor()
                    .spawn(async move {
                        let root_path = std::path::Path::new(&root_clone);
                        let mut index = SearchIndex::new();
                        index.build_from_path_with_progress(root_path, Some(progress_clone))?;
                        let count = index.file_count();
                        let hash = hash_path(&root_clone);
                        let snapshot = indexes_dir.join(format!("{hash}.zidx"));
                        persistence::save_index(&index, &snapshot)?;
                        Ok::<_, zero::index::IndexError>((root_clone, index, count))
                    })
                    .await;

                // Apply result on main thread (fast memory insert)
                match build_result {
                    Ok((root_str, index, count)) => {
                        tracing::info!(root = %root_str, files = count, "rebuild: root complete");
                        this.update(cx, |svc, _| {
                            svc.manager
                                .insert_index_memory_only(&root_str, index, count);
                        })
                        .ok();
                    }
                    Err(e) => {
                        tracing::error!(root = %root, error = %e, "rebuild: root failed");
                    }
                }
            }

            tracing::info!("rebuild: complete");
            this.update(cx, |svc, cx| {
                svc.indexing = false;
                cx.emit(SearchEvent::IndexingFinished);
                cx.notify();
            })
            .ok();
        })
        .detach();

        progress
    }

    pub fn add_root(&mut self, path: &str, cx: &mut Context<Self>) {
        tracing::info!(root = %path, "add_root");
        let path_owned = path.to_string();
        let indexes_dir = self.manager.indexes_dir().to_path_buf();

        let progress = Arc::new(CrawlProgress::new());
        self.indexing = true;
        cx.emit(SearchEvent::IndexingStarted {
            progress: progress.clone(),
            path: path_owned.clone(),
        });
        cx.notify();

        let p = progress;
        cx.spawn(async move |this, cx| {
            // Build index on background thread
            let build_result = cx
                .background_executor()
                .spawn(async move {
                    let root_path = std::path::Path::new(&path_owned);
                    let mut index = SearchIndex::new();
                    index.build_from_path_with_progress(root_path, Some(p))?;
                    let count = index.file_count();
                    let hash = hash_path(&path_owned);
                    let snapshot = indexes_dir.join(format!("{hash}.zidx"));
                    persistence::save_index(&index, &snapshot)?;
                    Ok::<_, zero::index::IndexError>((path_owned, index, count))
                })
                .await;

            // Apply result on main thread
            this.update(cx, |svc, cx| {
                match build_result {
                    Ok((root_str, index, count)) => {
                        tracing::info!(root = %root_str, files = count, "add_root: complete");
                        svc.manager
                            .insert_index_memory_only(&root_str, index, count);
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "add_root: failed");
                    }
                }
                svc.roots = svc.manager.roots().into_iter().map(PathBuf::from).collect();
                svc.indexing = false;
                cx.emit(SearchEvent::IndexingFinished);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn remove_root(&mut self, path: &str, cx: &mut Context<Self>) {
        tracing::debug!(path, "search_svc: remove root");
        self.manager.remove_root(path);
        self.roots = self
            .manager
            .roots()
            .into_iter()
            .map(PathBuf::from)
            .collect();
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("search_svc: clear");
        self.manager.clear();
        self.roots.clear();
        cx.emit(SearchEvent::IndexCleared);
        cx.notify();
    }

    // -- Internal -------------------------------------------------------------

    fn async_load(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            // 1. Gather root metadata (already cached from IndexManager::new())
            let root_info: Vec<(String, String)> = this
                .update(cx, |svc, _| {
                    svc.manager
                        .roots()
                        .into_iter()
                        .filter_map(|root| {
                            svc.manager
                                .root_stats(&root)
                                .map(|_| (root.clone(), hash_path(&root)))
                        })
                        .collect()
                })
                .unwrap_or_default();

            let indexes_dir = this
                .update(cx, |svc, _| svc.manager.indexes_dir().to_path_buf())
                .unwrap_or_default();

            if !root_info.is_empty() {
                tracing::info!(roots = root_info.len(), "loading indexes");

                // 2. Load each root: deserialize + build indices on background thread
                for (root, hash) in &root_info {
                    let dir = indexes_dir.clone();
                    let h = hash.clone();

                    // Full load on background thread (deserialize + build all indices)
                    let load_result = cx
                        .background_executor()
                        .spawn(async move { load_index_snapshot(&dir, &h) })
                        .await;

                    match load_result {
                        Ok(index) => {
                            let file_count = index.file_count();
                            let r = root.clone();
                            this.update(cx, |svc, cx| {
                                svc.manager.insert_index_memory_only(&r, index, file_count);
                                svc.roots =
                                    svc.manager.roots().into_iter().map(PathBuf::from).collect();
                                cx.emit(SearchEvent::RootLoaded {
                                    root: r,
                                    file_count,
                                });
                                cx.notify();
                            })
                            .ok();
                            tracing::info!(root = %root, files = file_count, "root loaded");
                        }
                        Err(e) => {
                            tracing::warn!(root = %root, error = %e, "failed to load root index");
                            this.update(cx, |svc, _| svc.manager.remove_stale_root(root))
                                .ok();
                        }
                    }
                }
            }

            // 3. Finalize: clear loading state, emit IndexLoaded
            this.update(cx, |svc, cx| {
                svc.roots = svc.manager.roots().into_iter().map(PathBuf::from).collect();
                svc.loading = false;
                cx.emit(SearchEvent::IndexLoaded);
                cx.notify();
            })
            .ok();

            // Check if we already have indexed roots
            let has_roots = this
                .update(cx, |svc, _| !svc.roots.is_empty())
                .unwrap_or(false);

            if has_roots {
                return;
            }

            // Auto-index home directory (first launch)
            let Some(home) = dirs::home_dir() else {
                return;
            };

            let home_str = home.to_string_lossy().to_string();
            tracing::info!(path = %home_str, "no indexed roots, starting auto-index");

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
                tracing::debug!(path = %home_str, "emitting IndexingStarted");
                cx.emit(SearchEvent::IndexingStarted {
                    progress: progress.clone(),
                    path: home_str.clone(),
                });
                cx.notify();
                svc.manager.indexes_dir().to_path_buf()
            });
            let Ok(indexes_dir) = indexes_dir else {
                tracing::error!("auto-index: failed to get indexes_dir");
                return;
            };

            tracing::info!("auto-index: building index on background thread");
            // Build index on background thread
            let home_clone = home_str.clone();
            let build_result = cx
                .background_executor()
                .spawn(async move {
                    let root_path = std::path::Path::new(&home_clone);
                    let mut index = SearchIndex::new();
                    index.build_from_path_with_progress(root_path, Some(p))?;
                    let count = index.file_count();
                    let hash = hash_path(&home_clone);
                    let snapshot = indexes_dir.join(format!("{hash}.zidx"));
                    persistence::save_index(&index, &snapshot)?;
                    Ok::<_, zero::index::IndexError>((home_clone, index, count))
                })
                .await;

            // Apply result on main thread
            match build_result {
                Ok((root_str, index, count)) => {
                    tracing::info!(
                        root = %root_str,
                        files = count,
                        "auto-index complete"
                    );
                    this.update(cx, |svc, _| {
                        svc.manager
                            .insert_index_memory_only(&root_str, index, count);
                        svc.roots = svc.manager.roots().into_iter().map(PathBuf::from).collect();
                    })
                    .ok();
                }
                Err(e) => {
                    tracing::error!(error = %e, "auto-index failed");
                }
            }

            this.update(cx, |svc, cx| {
                svc.indexing = false;
                cx.emit(SearchEvent::IndexingFinished);
                cx.notify();
            })
            .ok();
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
                            tracing::warn!(path = %root.display(), error = %e, "watcher: failed to watch root");
                        }
                    }
                    Ok(watcher)
                })
                .await;

            let mut watcher = match watcher_result {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!(error = %e, "watcher: failed to start");
                    this.update(cx, |svc, _| svc.watcher_active = false).ok();
                    return;
                }
            };

            tracing::info!("watcher: started");

            // Collect changed roots, rebuild after quiet period
            let mut dirty_roots: std::collections::HashSet<String> =
                std::collections::HashSet::new();
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

                        tracing::info!(roots = roots_to_rebuild.len(), "watcher: rebuilding");

                        for root in &roots_to_rebuild {
                            // Get indexes_dir (old index stays searchable during rebuild)
                            let indexes_dir = this
                                .update(cx, |svc, _| svc.manager.indexes_dir().to_path_buf());
                            let Ok(indexes_dir) = indexes_dir else {
                                continue;
                            };

                            // Shadow-build on background thread
                            let root_clone = root.clone();
                            let build_result = cx
                                .background_executor()
                                .spawn(async move {
                                    let root_path = std::path::Path::new(&root_clone);
                                    let mut index = SearchIndex::new();
                                    index.build_from_path_with_progress(root_path, None)?;
                                    let count = index.file_count();
                                    let hash = hash_path(&root_clone);
                                    let snapshot = indexes_dir.join(format!("{hash}.zidx"));
                                    persistence::save_index(&index, &snapshot)?;
                                    Ok::<_, zero::index::IndexError>((root_clone, index, count))
                                })
                                .await;

                            // Apply on main thread
                            match build_result {
                                Ok((root_str, index, count)) => {
                                    this.update(cx, |svc, _| {
                                        svc.manager
                                            .insert_index_memory_only(&root_str, index, count);
                                    })
                                    .ok();
                                }
                                Err(e) => {
                                    tracing::error!(root = %root, error = %e, "watcher: rebuild failed");
                                }
                            }
                        }

                        this.update(cx, |svc, cx| {
                            svc.roots =
                                svc.manager.roots().into_iter().map(PathBuf::from).collect();
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
