use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui::*;

/// Cached git status for a bookmarked repository.
pub struct RepoStatus {
    pub dirty: bool,
    #[allow(dead_code)] // stored for future sidebar display (branch label)
    pub branch: String,
}

pub enum GitEvent {
    StatusChanged,
}

impl EventEmitter<GitEvent> for GitService {}

/// Lightweight service that tracks dirty state for bookmarked git repos.
///
/// Sidebar reads from this during render. Refresh is triggered by:
/// 1. App startup (initial check)
/// 2. Window focus (catches external git operations)
pub struct GitService {
    statuses: HashMap<PathBuf, RepoStatus>,
    last_refresh: Option<Instant>,
}

impl GitService {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            statuses: HashMap::new(),
            last_refresh: None,
        }
    }

    /// Get cached status for a bookmark path.
    pub fn status(&self, path: &Path) -> Option<&RepoStatus> {
        self.statuses.get(path)
    }

    /// Check git status for the given paths in the background.
    pub fn refresh(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.last_refresh = Some(Instant::now());

        cx.spawn(async move |this, cx| {
            let results = cx
                .background_executor()
                .spawn(async move { check_paths(&paths) })
                .await;

            this.update(cx, |svc, cx| {
                svc.statuses = results;
                cx.emit(GitEvent::StatusChanged);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Refresh only if more than `min_interval` has elapsed since last check.
    pub fn refresh_if_stale(
        &mut self,
        paths: Vec<PathBuf>,
        min_interval: Duration,
        cx: &mut Context<Self>,
    ) {
        let stale = self
            .last_refresh
            .is_none_or(|t| t.elapsed() >= min_interval);
        if stale {
            self.refresh(paths, cx);
        }
    }
}

/// Check each path for git dirty status. Runs on background executor.
fn check_paths(paths: &[PathBuf]) -> HashMap<PathBuf, RepoStatus> {
    let mut out = HashMap::new();
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(false)
        .include_ignored(false);

    for path in paths {
        let Ok(repo) = git2::Repository::discover(path) else {
            continue;
        };
        let branch = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from))
            .unwrap_or_default();
        let dirty = repo.statuses(Some(&mut opts)).is_ok_and(|s| !s.is_empty());
        out.insert(path.clone(), RepoStatus { dirty, branch });
    }
    out
}
