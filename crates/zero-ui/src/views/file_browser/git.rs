use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Git status for a single file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    New,
    Deleted,
    Ignored,
}

impl GitFileStatus {
    /// Priority for propagation — higher wins when merging into parent dirs.
    fn priority(self) -> u8 {
        match self {
            Self::Ignored => 0,
            Self::Deleted => 1,
            Self::New => 2,
            Self::Modified => 3,
        }
    }
}

/// Git state for a directory: branch name and per-file statuses.
pub struct GitInfo {
    branch: Option<String>,
    statuses: HashMap<PathBuf, GitFileStatus>,
}

impl GitInfo {
    /// Discover git info for the directory containing `dir`.
    /// Returns `None` if the path is not inside a git repository.
    pub fn discover(dir: &Path) -> Option<Self> {
        let repo = git2::Repository::discover(dir).ok()?;

        let branch = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from));

        let mut statuses = HashMap::new();

        let opts = &mut git2::StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(false)
            .include_ignored(true);

        if let Ok(git_statuses) = repo.statuses(Some(opts)) {
            let workdir = repo.workdir().unwrap_or(dir);

            for entry in git_statuses.iter() {
                let Some(rel_path) = entry.path() else {
                    continue;
                };
                let status = entry.status();
                let Some(file_status) = classify_status(status) else {
                    continue;
                };
                let abs_path = workdir.join(rel_path);
                statuses.insert(abs_path, file_status);
            }

            // Propagate statuses up to parent directories so that
            // e.g. `packages/` shows as Modified when a child file is modified.
            let file_entries: Vec<(PathBuf, GitFileStatus)> =
                statuses.iter().map(|(p, s)| (p.clone(), *s)).collect();
            for (path, status) in file_entries {
                let mut ancestor = path.parent();
                while let Some(dir) = ancestor {
                    if dir == workdir || dir.as_os_str().is_empty() {
                        break;
                    }
                    let existing = statuses.get(dir).copied();
                    if existing.is_none_or(|e| status.priority() > e.priority()) {
                        statuses.insert(dir.to_path_buf(), status);
                    }
                    ancestor = dir.parent();
                }
            }
        }

        Some(Self { branch, statuses })
    }

    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    pub fn status(&self, path: &Path) -> Option<&GitFileStatus> {
        self.statuses.get(path)
    }
}

pub(super) fn classify_status(s: git2::Status) -> Option<GitFileStatus> {
    if s.contains(git2::Status::IGNORED) {
        return Some(GitFileStatus::Ignored);
    }
    if s.intersects(git2::Status::WT_DELETED | git2::Status::INDEX_DELETED) {
        return Some(GitFileStatus::Deleted);
    }
    if s.intersects(
        git2::Status::WT_MODIFIED
            | git2::Status::INDEX_MODIFIED
            | git2::Status::INDEX_RENAMED
            | git2::Status::INDEX_TYPECHANGE
            | git2::Status::CONFLICTED,
    ) {
        return Some(GitFileStatus::Modified);
    }
    if s.intersects(git2::Status::WT_NEW | git2::Status::INDEX_NEW) {
        return Some(GitFileStatus::New);
    }
    None
}

#[cfg(test)]
#[path = "git_test.rs"]
mod git_test;
