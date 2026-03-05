use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::*;
use zero::prelude::AtomicProgress;

use crate::models::{ClipboardOperation, FileClipboard};

use super::render::FileBrowserView;
use super::state::BrowserEntry;

// -- Events emitted to the parent ZeroApp ------------------------------------

pub enum FileBrowserEvent {
    NavigateToDir(PathBuf),
    OpenFile(PathBuf),
    SetClipboard(FileClipboard),
    AddBookmark(PathBuf),
    PasteStarted(Arc<AtomicProgress>),
    PasteFinished,
    NewTodoFile(PathBuf),
    FindDuplicatesHere(PathBuf),
    MoveToOtherPane(Vec<PathBuf>),
    CopyToOtherPane(Vec<PathBuf>),
}

impl EventEmitter<FileBrowserEvent> for FileBrowserView {}

// -- Action implementations --------------------------------------------------

impl FileBrowserView {
    /// Open the selected entry: navigate into dirs, edit text files, OS-open others.
    pub fn open_selected(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry(cx) else {
            return;
        };
        tracing::debug!(path = %entry.path.display(), is_dir = entry.is_dir, "browser: open selected");
        if entry.is_dir {
            cx.emit(FileBrowserEvent::NavigateToDir(entry.path.clone()));
        } else if crate::views::data_table::is_data_table(&entry.path)
            || crate::views::editor::is_editable(&entry.path)
        {
            cx.emit(FileBrowserEvent::OpenFile(entry.path.clone()));
        } else {
            #[cfg(target_os = "macos")]
            crate::platform::open::open_path(&entry.path);
        }
    }

    /// Reveal the selected entry in Finder.
    pub fn reveal_selected(&mut self, cx: &mut Context<Self>) {
        let Some(_entry) = self.selected_entry(cx) else {
            return;
        };
        tracing::debug!(path = %_entry.path.display(), "browser: reveal in finder");
        #[cfg(target_os = "macos")]
        crate::platform::open::reveal_in_finder(&_entry.path);
    }

    /// Show QuickLook for the selected file.
    pub fn quick_look_selected(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry(cx) else {
            return;
        };
        tracing::debug!(path = %entry.path.display(), "browser: quick look");
        if !entry.is_dir {
            #[cfg(target_os = "macos")]
            crate::platform::quicklook::preview_file(&entry.path);
        }
    }

    /// Copy the selected entry's path to the clipboard.
    pub fn copy_path_selected(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry(cx) else {
            return;
        };
        tracing::debug!(path = %entry.path.display(), "browser: copy path");
        let text = entry.path.to_string_lossy().to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    /// Show confirmation dialog before trashing the selected entry.
    pub fn trash_selected(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry(cx) else {
            return;
        };
        tracing::debug!(path = %entry.path.display(), "browser: trash selected (confirm)");
        self.pending_trash = Some(entry.path.clone());
        cx.notify();
    }

    /// Actually perform the trash operation (called after confirmation).
    pub fn confirm_trash(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.pending_trash.take()
            && trash::delete(&path).is_ok()
        {
            tracing::debug!(path = %path.display(), "browser: trash confirmed");
            self.reload(cx);
        }
        cx.notify();
    }

    /// Cancel the pending trash operation.
    pub fn cancel_trash(&mut self, cx: &mut Context<Self>) {
        self.pending_trash = None;
        cx.notify();
    }

    /// Navigate to a new directory in-place — no entity teardown.
    pub fn navigate(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        tracing::debug!(path = %path.display(), "browser: navigate");
        self.path = path.clone();

        // Clear modal/search state (meaningless in new folder)
        self.inline_edit = None;
        self.inline_input = None;
        self.pending_trash = None;
        self.search_active = false;
        self.search_input = None;
        self.display_mode = None;

        // Clear selection immediately
        self.table_state.update(cx, |state, cx| {
            state.delegate_mut().selected.clear();
            cx.notify();
        });

        // Don't set loading = true — old entries stay visible until new ones arrive
        let load_path = path;
        let start = std::time::Instant::now();

        cx.spawn(async move |this, cx| {
            let guard_path = load_path.clone();
            let entries = cx
                .background_executor()
                .spawn(async move { super::state::load_directory(&load_path) })
                .await;

            let elapsed = start.elapsed().as_secs_f64() * 1000.0;

            this.update(cx, |view, cx| {
                // Staleness guard: skip if user already navigated elsewhere
                if view.path != guard_path {
                    return;
                }
                view.loading = false;
                view.load_time_ms = elapsed;
                view.table_state.update(cx, |state, cx| {
                    let delegate = state.delegate_mut();
                    delegate.entries = entries;
                    delegate.selected.clear();
                    cx.notify();
                });
                cx.notify();
            })
            .ok();
        })
        .detach();

        cx.notify();
    }

    /// Reload the current directory listing (async).
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        cx.notify();

        let load_path = self.path.clone();
        cx.spawn(async move |this, cx| {
            let entries = cx
                .background_executor()
                .spawn(async move { super::state::load_directory(&load_path) })
                .await;

            this.update(cx, |view, cx| {
                view.loading = false;
                view.table_state.update(cx, |state, cx| {
                    let delegate = state.delegate_mut();
                    delegate.entries = entries;
                    delegate.selected.clear();
                    cx.notify();
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Get the first selected BrowserEntry (if any).
    pub(super) fn selected_entry(&self, cx: &Context<Self>) -> Option<BrowserEntry> {
        let delegate = self.table_state.read(cx).delegate();
        let idx = delegate.selected.first().copied()?;
        delegate.entries.get(idx).cloned()
    }

    /// Get all selected entries.
    fn selected_entries(&self, cx: &Context<Self>) -> Vec<BrowserEntry> {
        let delegate = self.table_state.read(cx).delegate();
        delegate
            .selected
            .iter()
            .filter_map(|&idx| delegate.entries.get(idx).cloned())
            .collect()
    }

    /// Copy selected files to the internal clipboard.
    pub fn copy_files(&mut self, cx: &mut Context<Self>) {
        let entries = self.selected_entries(cx);
        if entries.is_empty() {
            return;
        }
        tracing::debug!(count = entries.len(), "browser: copy files");
        let paths: Vec<PathBuf> = entries.into_iter().map(|e| e.path).collect();
        cx.emit(FileBrowserEvent::SetClipboard(FileClipboard::new(
            paths,
            ClipboardOperation::Copy,
        )));
    }

    /// Cut selected files to the internal clipboard.
    pub fn cut_files(&mut self, cx: &mut Context<Self>) {
        let entries = self.selected_entries(cx);
        if entries.is_empty() {
            return;
        }
        tracing::debug!(count = entries.len(), "browser: cut files");
        let paths: Vec<PathBuf> = entries.into_iter().map(|e| e.path).collect();
        cx.emit(FileBrowserEvent::SetClipboard(FileClipboard::new(
            paths,
            ClipboardOperation::Cut,
        )));
    }

    /// Paste files from the clipboard into the current directory (async).
    pub fn paste_files(&mut self, clipboard: &FileClipboard, cx: &mut Context<Self>) {
        tracing::debug!(dest = %self.path.display(), op = ?clipboard.operation, count = clipboard.paths.len(), "browser: paste files");
        let dest = self.path.clone();
        let sources = clipboard.paths.clone();
        let operation = clipboard.operation;

        let progress = Arc::new(AtomicProgress::empty());
        cx.emit(FileBrowserEvent::PasteStarted(progress.clone()));

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .spawn(async move {
                    // Count phase: walk sources to get totals
                    let mut total_files = 0usize;
                    let mut total_bytes = 0u64;
                    for src in &sources {
                        count_recursive(src, &mut total_files, &mut total_bytes);
                    }
                    progress.set_files_total(total_files);
                    progress.set_bytes_total(total_bytes);

                    // Copy/move phase
                    for src in &sources {
                        let Some(file_name) = src.file_name() else {
                            continue;
                        };
                        let target = unique_path(&dest.join(file_name));
                        let result = match operation {
                            ClipboardOperation::Copy => {
                                copy_recursive_with_progress(src, &target, &progress)
                            }
                            ClipboardOperation::Cut => {
                                let r = std::fs::rename(src, &target).map_err(|e| e.to_string());
                                if r.is_ok() {
                                    // For moves, count as instantly done
                                    let mut f = 0usize;
                                    let mut b = 0u64;
                                    count_recursive(&target, &mut f, &mut b);
                                    progress.add_bytes(b);
                                    for _ in 0..f {
                                        progress.file_done();
                                    }
                                }
                                r
                            }
                        };
                        if let Err(e) = result {
                            tracing::error!(error = %e, "paste failed");
                        }
                    }
                })
                .await;

            this.update(cx, |view, cx| {
                view.reload(cx);
                cx.emit(FileBrowserEvent::PasteFinished);
            })
            .ok();
        })
        .detach();
    }

    /// Add the selected directory to sidebar bookmarks.
    pub fn add_to_bookmarks(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry(cx) else {
            return;
        };
        if entry.is_dir {
            tracing::debug!(path = %entry.path.display(), "browser: add to bookmarks");
            cx.emit(FileBrowserEvent::AddBookmark(entry.path.clone()));
        }
    }

    /// Create a new .todo file in the current directory.
    pub fn new_todo_file(&mut self, cx: &mut Context<Self>) {
        let mut candidate = self.path.join("project.todo");
        let mut counter = 2u32;
        while candidate.exists() {
            candidate = self.path.join(format!("project {counter}.todo"));
            counter += 1;
        }
        // Create empty file
        if std::fs::write(&candidate, b"").is_ok() {
            cx.emit(FileBrowserEvent::NewTodoFile(candidate));
            self.reload(cx);
        }
    }

    /// Emit a request to find duplicates scoped to the selected folder.
    pub fn find_duplicates_here(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry(cx) else {
            return;
        };
        if entry.is_dir {
            cx.emit(FileBrowserEvent::FindDuplicatesHere(entry.path.clone()));
        }
    }

    /// Emit selected entries for move to the other split pane.
    pub fn move_to_other_pane(&mut self, cx: &mut Context<Self>) {
        let paths: Vec<PathBuf> = self
            .selected_entries(cx)
            .into_iter()
            .map(|e| e.path)
            .collect();
        if !paths.is_empty() {
            cx.emit(FileBrowserEvent::MoveToOtherPane(paths));
        }
    }

    /// Emit selected entries for copy to the other split pane.
    pub fn copy_to_other_pane(&mut self, cx: &mut Context<Self>) {
        let paths: Vec<PathBuf> = self
            .selected_entries(cx)
            .into_iter()
            .map(|e| e.path)
            .collect();
        if !paths.is_empty() {
            cx.emit(FileBrowserEvent::CopyToOtherPane(paths));
        }
    }

    /// Duplicate selected files in place.
    pub fn duplicate_files(&mut self, cx: &mut Context<Self>) {
        let entries = self.selected_entries(cx);
        if entries.is_empty() {
            return;
        }
        tracing::debug!(count = entries.len(), "browser: duplicate files");
        for entry in &entries {
            let file_name = match entry.path.file_name() {
                Some(n) => n,
                None => continue,
            };
            let parent = entry.path.parent().unwrap_or(&self.path);
            let target = unique_path(&parent.join(file_name));
            if let Err(e) = copy_recursive(&entry.path, &target) {
                tracing::error!(error = %e, "duplicate failed");
            }
        }
        self.reload(cx);
    }
}

// -- Helpers -----------------------------------------------------------------

/// Generate a unique path by appending " 2", " 3", etc. if the target exists.
fn unique_path(base: &Path) -> PathBuf {
    if !base.exists() {
        return base.to_path_buf();
    }
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = base.extension().map(|e| e.to_string_lossy().to_string());
    let parent = base.parent().unwrap_or(Path::new("."));

    for i in 2..1000 {
        let name = match &ext {
            Some(e) => format!("{} {}.{}", stem, i, e),
            None => format!("{} {}", stem, i),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    base.to_path_buf()
}

/// Recursively copy a file or directory.
pub(crate) fn copy_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    if src.is_dir() {
        std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
        let entries = std::fs::read_dir(src).map_err(|e| e.to_string())?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let child_dest = dest.join(entry.file_name());
            copy_recursive(&entry.path(), &child_dest)?;
        }
        Ok(())
    } else {
        std::fs::copy(src, dest)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Recursively count files and bytes under a path.
fn count_recursive(path: &Path, files: &mut usize, bytes: &mut u64) {
    if path.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                count_recursive(&entry.path(), files, bytes);
            }
        }
    } else if let Ok(meta) = path.metadata() {
        *files += 1;
        *bytes += meta.len();
    }
}

/// Recursively copy a file or directory, updating progress after each file.
fn copy_recursive_with_progress(
    src: &Path,
    dest: &Path,
    progress: &AtomicProgress,
) -> Result<(), String> {
    if src.is_dir() {
        std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
        let entries = std::fs::read_dir(src).map_err(|e| e.to_string())?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let child_dest = dest.join(entry.file_name());
            copy_recursive_with_progress(&entry.path(), &child_dest, progress)?;
        }
        Ok(())
    } else {
        let size = std::fs::copy(src, dest).map_err(|e| e.to_string())?;
        progress.add_bytes(size);
        progress.file_done();
        Ok(())
    }
}
