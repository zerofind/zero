use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, Icon, IconName, Sizable as _, button::Button, h_flex, v_flex,
};

use profiles::CleanupGroup;

use crate::services::{SearchEvent, SearchService};
use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, ICON_MD, RADIUS};
use crate::ui::{EmptyState, StatusBar, StatusBarMode, format_size};

use gpui_component::table::TableState;

use super::delegate::{CleanupDetailDelegate, CleanupRow};
use super::detail::{render_detail_view, shorten_path};
use super::{
    CategoryFileItem, CategoryResult, CleanupProgress, CleanupViewEvent, DevDirEntry, GroupResult,
};

pub struct CleanupView {
    pub(super) search: Entity<SearchService>,
    pub(super) groups: Vec<GroupResult>,
    pub(super) scanning: bool,
    pub(super) cleaning: bool,
    // Drill-in state (set by app via set_detail_group)
    pub(super) detail_group: Option<usize>,
    pub(super) detail_table: Option<Entity<TableState<CleanupDetailDelegate>>>,
    /// Pending rows waiting for table materialization (needs Window from render).
    pending_rows: Option<Vec<CleanupRow>>,
    /// Result of the last clean operation (ok, failed) for footer display.
    pub(super) last_clean_result: Option<(usize, usize)>,
    pub(super) focus_handle: FocusHandle,
    _search_sub: Option<Subscription>,
}

impl CleanupView {
    pub fn new(search: Entity<SearchService>, cx: &mut Context<Self>) -> Self {
        let is_loading = search.read(cx).is_loading();

        let mut view = Self {
            search,
            groups: Vec::new(),
            scanning: false,
            cleaning: false,
            detail_group: None,
            detail_table: None,
            pending_rows: None,
            last_clean_result: None,
            focus_handle: cx.focus_handle(),
            _search_sub: None,
        };

        if is_loading {
            // Index still loading from disk — show scanning state and wait
            view.scanning = true;
            cx.emit(CleanupViewEvent::ScanStarted);
            cx.notify();
            let search_entity = view.search.clone();
            view._search_sub = Some(cx.subscribe(
                &search_entity,
                |this, _, event: &SearchEvent, cx| {
                    if matches!(event, SearchEvent::IndexLoaded) {
                        this._search_sub = None;
                        this.start_scan(cx);
                    }
                },
            ));
        } else {
            view.start_scan(cx);
        }

        view
    }

    pub fn is_scanning(&self) -> bool {
        self.scanning
    }

    pub fn group_name(&self, idx: usize) -> &'static str {
        self.groups
            .get(idx)
            .map(|g| g.group.display_name())
            .unwrap_or("Detail")
    }

    /// Called by ZeroApp when navigating to CleanupDetail(idx).
    pub fn set_detail_group(&mut self, idx: Option<usize>, cx: &mut Context<Self>) {
        if self.detail_group == idx {
            return;
        }
        self.detail_group = idx;
        self.detail_table = None;

        if let Some(i) = idx
            && let Some(group) = self.groups.get(i)
        {
            if group.detail_loaded {
                // Rebuild rows from cached group data
                self.pending_rows = Some(Self::rows_from_group(group));
            } else {
                self.load_group_detail(i, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn start_scan(&mut self, cx: &mut Context<Self>) {
        tracing::info!("cleanup: scan starting");
        self.scanning = true;
        self.groups.clear();
        self.detail_group = None;
        self.detail_table = None;
        cx.emit(CleanupViewEvent::ScanStarted);
        cx.notify();

        let search = self.search.clone();

        cx.spawn(async move |this, cx| {
            let manager = this
                .update(cx, |_, cx| search.read(cx).shared_manager())
                .ok();
            let Some(manager) = manager else { return };

            let active_groups: Vec<CleanupGroup> = profiles::load_cleanup()
                .map(|p| p.active_groups())
                .unwrap_or_default();

            for group in active_groups {
                let mgr = Arc::clone(&manager);
                let summary = cx
                    .background_executor()
                    .spawn(async move { cleanup::execute_group_cleanup(&mgr, group).ok() })
                    .await;

                if let Some(summary) = summary
                    && summary.total_count > 0
                {
                    this.update(cx, |view, cx| {
                        view.groups.push(GroupResult {
                            group: summary.group,
                            total_bytes: summary.total_bytes,
                            item_count: summary.total_count,
                            detail_loaded: false,
                            dev_dirs: Vec::new(),
                            categories: Vec::new(),
                            cached_summary: Some(summary),
                        });
                        cx.notify();
                    })
                    .ok();
                }
            }

            this.update(cx, |view, cx| {
                let group_count = view.groups.len();
                let total_bytes: u64 = view.groups.iter().map(|g| g.total_bytes).sum();
                let total_items: usize = view.groups.iter().map(|g| g.item_count).sum();
                tracing::info!(
                    groups = group_count,
                    items = total_items,
                    bytes = total_bytes,
                    "cleanup: scan finished"
                );
                view.scanning = false;
                cx.emit(CleanupViewEvent::ScanFinished);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn load_group_detail(&mut self, idx: usize, cx: &mut Context<Self>) {
        let Some(group) = self.groups.get_mut(idx) else {
            return;
        };
        let cleanup_group = group.group;
        let cached = group.cached_summary.take();
        let has_cache = cached.is_some();
        tracing::info!(
            group = cleanup_group.display_name(),
            cached = has_cache,
            "cleanup: loading group detail"
        );

        // If we have no cached summary, fall back to re-querying
        let search = self.search.clone();

        cx.spawn(async move |this, cx| {
            let summary = if let Some(s) = cached {
                Some(s)
            } else {
                let manager = this
                    .update(cx, |_, cx| search.read(cx).shared_manager())
                    .ok();
                let Some(manager) = manager else { return };
                cx.background_executor()
                    .spawn(
                        async move { cleanup::execute_group_cleanup(&manager, cleanup_group).ok() },
                    )
                    .await
            };

            let Some(summary) = summary else { return };

            // ALL processing on background thread — build rows + group data
            let processed = cx
                .background_executor()
                .spawn(async move {
                    if cleanup_group == CleanupGroup::Developer {
                        let dirs = group_dev_results(&summary);
                        let rows: Vec<CleanupRow> = dirs
                            .iter()
                            .map(|d| CleanupRow {
                                display_name: shorten_path(&d.path),
                                subtitle: d.category_name.clone(),
                                size: d.total_bytes,
                                path: d.path.clone(),
                            })
                            .collect();
                        (DetailResult::Dev(dirs), rows)
                    } else {
                        let categories: Vec<CategoryResult> = summary
                            .categories
                            .into_iter()
                            .map(|cat| CategoryResult {
                                name: cat.category_name.to_string(),
                                group: cleanup_group.display_name().to_string(),
                                file_count: cat.count,
                                total_bytes: cat.total_bytes,
                                files: cat
                                    .items
                                    .iter()
                                    .map(|item| CategoryFileItem {
                                        path: PathBuf::from(&item.node.path),
                                        name: item.node.name().to_string(),
                                        size: item.node.size,
                                    })
                                    .collect(),
                            })
                            .collect();
                        let rows: Vec<CleanupRow> = categories
                            .iter()
                            .flat_map(|cat| {
                                cat.files.iter().map(|f| CleanupRow {
                                    display_name: f.name.clone(),
                                    subtitle: f.path.parent().map(shorten_path).unwrap_or_default(),
                                    size: f.size,
                                    path: f.path.clone(),
                                })
                            })
                            .collect();
                        (DetailResult::Categories(categories), rows)
                    }
                })
                .await;

            let (detail, rows) = processed;

            // Main thread: assign data + create table state
            this.update(cx, |view, cx| {
                let Some(group) = view.groups.get_mut(idx) else {
                    return;
                };
                if group.group != cleanup_group {
                    return;
                }
                let row_count = rows.len();
                match detail {
                    DetailResult::Dev(dirs) => group.dev_dirs = dirs,
                    DetailResult::Categories(cats) => group.categories = cats,
                }
                group.detail_loaded = true;
                tracing::info!(
                    group = cleanup_group.display_name(),
                    rows = row_count,
                    "cleanup: group detail loaded"
                );
                view.pending_rows = Some(rows);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn toggle_detail_select_all(&mut self, cx: &mut Context<Self>) {
        if let Some(ref table) = self.detail_table {
            table.update(cx, |state, cx| {
                state.delegate_mut().toggle_all();
                cx.notify();
            });
        }
        cx.notify();
    }

    /// Clean selected items in detail view.
    pub(super) fn clean_detail_selected(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("cleanup: clean_detail_selected called");
        if self.cleaning {
            tracing::warn!("cleanup: already cleaning, ignoring");
            return;
        }
        let Some(ref table) = self.detail_table else {
            tracing::warn!("cleanup: no detail_table, ignoring");
            return;
        };

        let paths: Vec<PathBuf> = table.read(cx).delegate().selected_paths();
        if paths.is_empty() {
            tracing::warn!("cleanup: no paths selected, ignoring");
            return;
        }

        let count = paths.len();
        let total_bytes: u64 = table
            .read(cx)
            .delegate()
            .selected
            .iter()
            .filter_map(|&i| table.read(cx).delegate().rows.get(i))
            .map(|r| r.size)
            .sum();

        tracing::info!(
            count,
            bytes = total_bytes,
            "cleanup: cleaning selected items"
        );

        let progress = CleanupProgress::new(count as u64, total_bytes);
        self.cleaning = true;
        self.last_clean_result = None;
        cx.emit(CleanupViewEvent::CleanStarted {
            message: format!("Cleaning {count} items ({})...", format_size(total_bytes)),
            progress: Arc::clone(&progress),
        });
        cx.notify();

        let detail_idx = self.detail_group;
        let selected_set: std::collections::HashSet<PathBuf> = paths.iter().cloned().collect();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { trash_paths(&paths, &progress) })
                .await;

            tracing::info!(
                ok = result.ok,
                fail = result.fail,
                cancelled = result.cancelled,
                "cleanup: clean detail finished"
            );

            this.update(cx, |view, cx| {
                view.cleaning = false;
                view.last_clean_result = Some((result.ok, result.fail));

                // Remove successfully deleted rows from the table + index
                if result.ok > 0 {
                    let failed: std::collections::HashSet<PathBuf> =
                        result.failed_paths.into_iter().collect();
                    let deleted: std::collections::HashSet<PathBuf> = selected_set
                        .into_iter()
                        .filter(|p| !failed.contains(p))
                        .collect();

                    // Remove from in-memory search index
                    let deleted_strs: Vec<String> = deleted
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();
                    view.search.update(cx, |svc, _| {
                        svc.remove_paths(&deleted_strs);
                    });

                    if let Some(ref table) = view.detail_table {
                        let bytes_removed = table.update(cx, |state, cx| {
                            let removed = state.delegate_mut().remove_deleted(&deleted);
                            cx.notify();
                            removed
                        });
                        if let Some(idx) = detail_idx
                            && let Some(group) = view.groups.get_mut(idx)
                        {
                            group.item_count = group.item_count.saturating_sub(result.ok);
                            group.total_bytes = group.total_bytes.saturating_sub(bytes_removed);
                        }
                    }
                }

                cx.emit(CleanupViewEvent::CleanFinished {
                    ok: result.ok,
                    failed: result.fail,
                    cancelled: result.cancelled,
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Clean all items in a group (from group overview).
    fn clean_group(&mut self, group_idx: usize, cx: &mut Context<Self>) {
        tracing::debug!(group_idx, "cleanup: clean_group called");
        if self.cleaning {
            tracing::warn!("cleanup: already cleaning, ignoring");
            return;
        }
        let Some(group) = self.groups.get_mut(group_idx) else {
            tracing::warn!(group_idx, "cleanup: group index out of bounds");
            return;
        };

        let group_name = group.group.display_name();
        let group_bytes = group.total_bytes;
        let group_count = group.item_count;

        tracing::info!(
            group = group_name,
            items = group_count,
            bytes = group_bytes,
            "cleanup: cleaning group"
        );

        let message = format!(
            "Cleaning {group_name} ({group_count} items, {})...",
            format_size(group_bytes)
        );

        // For groups without detail loaded, use cached summary or re-query
        if !group.detail_loaded {
            let cleanup_group = group.group;
            let cached = group.cached_summary.take();
            let search = self.search.clone();
            let progress = CleanupProgress::new(group_count as u64, group_bytes);
            self.cleaning = true;
            self.last_clean_result = None;
            cx.emit(CleanupViewEvent::CleanStarted {
                message,
                progress: Arc::clone(&progress),
            });
            cx.notify();

            cx.spawn(async move |this, cx| {
                let summary = if let Some(s) = cached {
                    Some(s)
                } else {
                    let manager = this
                        .update(cx, |_, cx| search.read(cx).shared_manager())
                        .ok();
                    let Some(manager) = manager else { return };
                    cx.background_executor()
                        .spawn(async move {
                            cleanup::execute_group_cleanup(&manager, cleanup_group).ok()
                        })
                        .await
                };

                let Some(summary) = summary else { return };

                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let paths: Vec<PathBuf> = if cleanup_group == CleanupGroup::Developer {
                            group_dev_results(&summary)
                                .into_iter()
                                .map(|d| d.path)
                                .collect()
                        } else {
                            summary
                                .categories
                                .iter()
                                .flat_map(|cat| {
                                    cat.items.iter().map(|item| PathBuf::from(&item.node.path))
                                })
                                .collect()
                        };

                        // Update total now that we know exact path count
                        progress
                            .files_total
                            .store(paths.len() as u64, Ordering::Relaxed);

                        trash_paths(&paths, &progress)
                    })
                    .await;

                tracing::info!(
                    ok = result.ok,
                    fail = result.fail,
                    cancelled = result.cancelled,
                    "cleanup: clean group finished"
                );

                this.update(cx, |view, cx| {
                    view.cleaning = false;
                    view.last_clean_result = Some((result.ok, result.fail));

                    // Remove trashed paths from index
                    if result.ok > 0 {
                        let deleted_strs: Vec<String> = result
                            .ok_paths
                            .iter()
                            .map(|p| p.to_string_lossy().to_string())
                            .collect();
                        view.search.update(cx, |svc, _| {
                            svc.remove_paths(&deleted_strs);
                        });
                    }

                    cx.emit(CleanupViewEvent::CleanFinished {
                        ok: result.ok,
                        failed: result.fail,
                        cancelled: result.cancelled,
                    });
                    view.start_scan(cx);
                })
                .ok();
            })
            .detach();
            return;
        }

        let progress = CleanupProgress::new(group_count as u64, group_bytes);
        self.cleaning = true;
        self.last_clean_result = None;
        cx.emit(CleanupViewEvent::CleanStarted {
            message,
            progress: Arc::clone(&progress),
        });
        cx.notify();

        let paths: Vec<PathBuf> = if group.group == CleanupGroup::Developer {
            group.dev_dirs.iter().map(|d| d.path.clone()).collect()
        } else {
            group
                .categories
                .iter()
                .flat_map(|c| c.files.iter().map(|f| f.path.clone()))
                .collect()
        };

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { trash_paths(&paths, &progress) })
                .await;

            tracing::info!(
                ok = result.ok,
                fail = result.fail,
                cancelled = result.cancelled,
                "cleanup: clean group finished"
            );

            this.update(cx, |view, cx| {
                view.cleaning = false;
                view.last_clean_result = Some((result.ok, result.fail));

                // Remove trashed paths from index
                if result.ok > 0 {
                    let deleted_strs: Vec<String> = result
                        .ok_paths
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();
                    view.search.update(cx, |svc, _| {
                        svc.remove_paths(&deleted_strs);
                    });
                }

                cx.emit(CleanupViewEvent::CleanFinished {
                    ok: result.ok,
                    failed: result.fail,
                    cancelled: result.cancelled,
                });
                view.start_scan(cx);
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn detail_focus_prev(&mut self, cx: &mut Context<Self>) {
        if let Some(ref table) = self.detail_table {
            table.update(cx, |state, cx| {
                state.delegate_mut().focus_prev();
                cx.notify();
            });
        }
    }

    pub(super) fn detail_focus_next(&mut self, cx: &mut Context<Self>) {
        if let Some(ref table) = self.detail_table {
            table.update(cx, |state, cx| {
                state.delegate_mut().focus_next();
                cx.notify();
            });
        }
    }

    pub(super) fn detail_toggle_focused(&mut self, cx: &mut Context<Self>) {
        if let Some(ref table) = self.detail_table {
            table.update(cx, |state, cx| {
                state.delegate_mut().toggle_focused();
                cx.notify();
            });
        }
    }

    pub(super) fn detail_open_focused(&mut self, cx: &mut Context<Self>) {
        if let Some(ref table) = self.detail_table {
            table.read(cx).delegate().open_focused();
        }
    }

    pub(super) fn detail_item_count(&self, cx: &App) -> usize {
        self.detail_table
            .as_ref()
            .map(|t| t.read(cx).delegate().rows.len())
            .unwrap_or(0)
    }

    pub(super) fn detail_selected_count(&self, cx: &App) -> usize {
        self.detail_table
            .as_ref()
            .map(|t| t.read(cx).delegate().selected.len())
            .unwrap_or(0)
    }

    pub(super) fn detail_selected_bytes(&self, cx: &App) -> u64 {
        self.detail_table
            .as_ref()
            .map(|t| t.read(cx).delegate().selected_bytes())
            .unwrap_or(0)
    }

    /// Build CleanupRow list from already-loaded group data.
    fn rows_from_group(group: &GroupResult) -> Vec<CleanupRow> {
        if group.group == CleanupGroup::Developer {
            group
                .dev_dirs
                .iter()
                .map(|d| CleanupRow {
                    display_name: shorten_path(&d.path),
                    subtitle: d.category_name.clone(),
                    size: d.total_bytes,
                    path: d.path.clone(),
                })
                .collect()
        } else {
            group
                .categories
                .iter()
                .flat_map(|cat| {
                    cat.files.iter().map(|f| CleanupRow {
                        display_name: f.name.clone(),
                        subtitle: f.path.parent().map(shorten_path).unwrap_or_default(),
                        size: f.size,
                        path: f.path.clone(),
                    })
                })
                .collect()
        }
    }

    /// Create the TableState from pending rows (requires window access from render).
    fn materialize_table(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(rows) = self.pending_rows.take() {
            let delegate = CleanupDetailDelegate::new(rows);
            let state = cx.new(|cx| TableState::new(delegate, window, cx).col_selectable(false));
            self.detail_table = Some(state);
        }
    }

    fn group_icon(group: CleanupGroup) -> IconName {
        match group {
            CleanupGroup::Developer => IconName::SquareTerminal,
            CleanupGroup::System => IconName::Settings,
            CleanupGroup::Documents => IconName::File,
            CleanupGroup::Media => IconName::Eye,
            CleanupGroup::Mail => IconName::Inbox,
            CleanupGroup::Messages => IconName::Inbox,
            CleanupGroup::Ios => IconName::ArrowDown,
            CleanupGroup::Trash => IconName::Delete,
        }
    }
}

/// Result of a trash operation.
struct TrashResult {
    ok: usize,
    fail: usize,
    cancelled: bool,
    /// Paths that failed to trash (kept on disk).
    failed_paths: Vec<PathBuf>,
    /// Paths successfully trashed (for index removal).
    ok_paths: Vec<PathBuf>,
}

/// Trash a list of paths with atomic progress tracking.
fn trash_paths(paths: &[PathBuf], progress: &CleanupProgress) -> TrashResult {
    let start = std::time::Instant::now();
    let mut ok = 0usize;
    let mut fail = 0usize;
    let mut failed_paths = Vec::new();
    let mut ok_paths = Vec::new();

    for path in paths {
        if progress.is_cancelled() {
            tracing::info!(
                ok,
                fail,
                remaining = paths.len() - ok - fail,
                "cleanup: cancelled by user"
            );
            break;
        }

        // Path already gone (stale index) — count as success
        if !path.exists() {
            tracing::debug!(path = %path.display(), "cleanup: path already gone, skipping");
            ok += 1;
            ok_paths.push(path.clone());
            progress.files_done.fetch_add(1, Ordering::Relaxed);
            continue;
        }

        tracing::debug!(path = %path.display(), "cleanup: trashing");
        match crate::platform::trash::move_to_trash(path) {
            Ok(()) => {
                ok += 1;
                ok_paths.push(path.clone());
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "cleanup: trash failed");
                fail += 1;
                failed_paths.push(path.clone());
                progress.failed.fetch_add(1, Ordering::Relaxed);
            }
        }
        progress.files_done.fetch_add(1, Ordering::Relaxed);
    }

    let elapsed = start.elapsed();
    tracing::info!(
        ok,
        fail,
        elapsed_ms = elapsed.as_millis() as u64,
        "cleanup: trash_paths completed"
    );

    TrashResult {
        ok,
        fail,
        cancelled: progress.is_cancelled(),
        failed_paths,
        ok_paths,
    }
}

/// Intermediate result built entirely on background thread.
enum DetailResult {
    Dev(Vec<DevDirEntry>),
    Categories(Vec<CategoryResult>),
}

/// Canonical display name for an artifact directory.
fn artifact_display_name(dir_name: &str) -> &'static str {
    match dir_name {
        "target" => "Rust build cache",
        "node_modules" => "Node modules",
        ".next" => "Next.js cache",
        "build" => "Build output",
        ".gradle" => "Gradle cache",
        "__pycache__" => "Python cache",
        ".tox" => "Tox cache",
        "dist" => "Dist output",
        ".cache" => "Cache",
        "Pods" => "CocoaPods",
        ".build" => "Swift build",
        "DerivedData" => "Xcode derived data",
        ".dart_tool" => "Dart tool cache",
        "zig-cache" => "Zig cache",
        "_build" => "Elixir build",
        "deps" => "Dependencies",
        ".venv" | "venv" => "Python virtualenv",
        _ => "Build artifacts",
    }
}

/// Group Developer cleanup results by artifact directory.
fn group_dev_results(summary: &cleanup::GroupSummary) -> Vec<DevDirEntry> {
    let dev_dir_names: &[&str] = &[
        "target",
        "node_modules",
        ".next",
        "build",
        ".gradle",
        "__pycache__",
        ".tox",
        "dist",
        ".cache",
        "Pods",
        ".build",
        "DerivedData",
        ".dart_tool",
        "zig-cache",
        "_build",
        "deps",
        ".venv",
        "venv",
    ];

    let mut dir_map: HashMap<PathBuf, (u64, usize, &str)> = HashMap::new();

    for cat_result in &summary.categories {
        for item in &cat_result.items {
            let path = Path::new(&item.node.path);
            if let Some((artifact_root, dir_name)) = find_artifact_root(path, dev_dir_names) {
                let entry =
                    dir_map
                        .entry(artifact_root)
                        .or_insert((0, 0, artifact_display_name(dir_name)));
                entry.0 += item.node.size;
                entry.1 += 1;
            }
        }
    }

    let mut dirs: Vec<DevDirEntry> = dir_map
        .into_iter()
        .map(|(path, (bytes, count, name))| DevDirEntry {
            path,
            total_bytes: bytes,
            file_count: count,
            category_name: name.to_string(),
        })
        .collect();

    dirs.sort_by_key(|d| std::cmp::Reverse(d.total_bytes));
    dirs
}

/// Find the outermost artifact directory in a path (root→leaf order).
/// Returns the full path to the artifact dir and its directory name.
fn find_artifact_root<'a>(path: &Path, dir_names: &'a [&str]) -> Option<(PathBuf, &'a str)> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component);
        if let Some(name_os) = component.as_os_str().to_str()
            && let Some(&matched) = dir_names.iter().find(|&&d| d == name_os)
        {
            return Some((prefix, matched));
        }
    }
    None
}

impl Render for CleanupView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Materialize table if rows arrived from background
        if self.pending_rows.is_some() {
            self.materialize_table(window, cx);
        }

        // Drill-in detail view (when app pushes CleanupDetail)
        if self.detail_group.is_some() {
            return render_detail_view(self, window, cx).into_any_element();
        }

        let muted = cx.theme().muted_foreground;

        let has_results = !self.groups.is_empty();
        let total_bytes: u64 = self.groups.iter().map(|g| g.total_bytes).sum();

        let group_count = self.groups.len();

        div()
            .relative()
            .size_full()
            .child(
                v_flex()
                    .size_full()
                    .bg(theme::content_bg(cx))
                    // Content
                    .when(!has_results && !self.scanning, |el| {
                        el.child(
                            EmptyState::new(IconName::Search, "No cleanable files found")
                                .subtitle("All scanned categories are clean"),
                        )
                    })
                    .when(self.scanning && !has_results, |el| {
                        el.child(
                            EmptyState::new(IconName::Search, "Analyzing your files...")
                                .subtitle("Scanning categories for cleanable files"),
                        )
                    })
                    .when(has_results, |el| {
                        el.child(
                            v_flex()
                                .id("cleanup-groups")
                                .flex_1()
                                .overflow_y_scroll()
                                .p_4()
                                .gap_1()
                                .children(self.groups.iter().enumerate().map(|(i, group)| {
                                    let icon = Self::group_icon(group.group);
                                    let name = group.group.display_name();
                                    let item_count = group.item_count;
                                    let total = group.total_bytes;
                                    let cleaning = self.cleaning;

                                    h_flex()
                                        .id(SharedString::from(format!("group-{i}")))
                                        .w_full()
                                        .px_3()
                                        .py_2p5()
                                        .gap_3()
                                        .items_center()
                                        .rounded(RADIUS)
                                        .cursor_pointer()
                                        .hover(|s| s.bg(theme::surface_hover(cx)))
                                        .on_click(cx.listener(move |_this, _, _, cx| {
                                            cx.emit(CleanupViewEvent::DrillInto(i));
                                        }))
                                        // Icon
                                        .child(div().flex_shrink_0().child(
                                            Icon::new(icon).with_size(ICON_MD).text_color(muted),
                                        ))
                                        // Name + count
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .child(
                                                    div()
                                                        .text_size(FONT_SIZE_BODY)
                                                        .child(SharedString::from(name)),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(FONT_SIZE_CAPTION)
                                                        .text_color(muted)
                                                        .child(SharedString::from(format!(
                                                            "{item_count} items"
                                                        ))),
                                                ),
                                        )
                                        // Size
                                        .child(
                                            div()
                                                .text_size(FONT_SIZE_BODY)
                                                .min_w(px(80.0))
                                                .text_right()
                                                .child(SharedString::from(format_size(total))),
                                        )
                                        // Review button
                                        .child(
                                            Button::new(SharedString::from(format!("review-{i}")))
                                                .label("Review")
                                                .compact()
                                                .small()
                                                .on_click(cx.listener(move |_this, _, _, cx| {
                                                    cx.emit(CleanupViewEvent::DrillInto(i));
                                                })),
                                        )
                                        // Clean button
                                        .child(
                                            Button::new(SharedString::from(format!(
                                                "clean-group-{i}"
                                            )))
                                            .label("Clean")
                                            .compact()
                                            .small()
                                            .disabled(cleaning)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.clean_group(i, cx);
                                            })),
                                        )
                                })),
                        )
                        .child(StatusBar::new(StatusBarMode::Cleanup {
                            total_items: group_count,
                            total_size: total_bytes,
                            selected_count: 0,
                            selected_size: 0,
                            label: "groups",
                            last_result: self.last_clean_result,
                        }))
                    }),
            )
            .into_any_element()
    }
}
