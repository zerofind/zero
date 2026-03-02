use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::*;

use zero::dedup::DedupProgress;

use crate::ui::format::{format_bytes, format_number};

use super::{DedupEvent, DedupFilter, DedupSort, DedupView, DuplicateGroup};

impl DedupView {
    pub(super) fn start_scan(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.scan_path.clone() else {
            return;
        };
        self.scanning = true;
        self.scan_status = Some("Scanning...".to_string());
        self.groups.clear();
        self.selected_for_deletion.clear();
        cx.notify();

        let progress = Arc::new(DedupProgress::new());
        cx.emit(DedupEvent::ScanStarted(progress.clone()));

        let filter = self.active_filter;

        // 150ms poll loop for phased status
        let poll_progress = progress.clone();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;

                let still_running = this
                    .update(cx, |view, cx| {
                        if !view.scanning {
                            return false;
                        }
                        let phase = poll_progress
                            .phase
                            .load(std::sync::atomic::Ordering::Acquire);
                        let status = match phase {
                            0 => {
                                let files = poll_progress
                                    .files_found
                                    .load(std::sync::atomic::Ordering::Relaxed);
                                format!("Scanning... {} files", format_number(files as u64))
                            }
                            1 => {
                                let pct = poll_progress.hash_percent();
                                let done = poll_progress
                                    .bytes_hashed
                                    .load(std::sync::atomic::Ordering::Relaxed);
                                let total = poll_progress
                                    .bytes_to_hash
                                    .load(std::sync::atomic::Ordering::Relaxed);
                                format!(
                                    "Hashing... {:.0}% ({} / {})",
                                    pct,
                                    format_bytes(done),
                                    format_bytes(total),
                                )
                            }
                            _ => "Finishing...".to_string(),
                        };
                        view.scan_status = Some(status);
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);

                if !still_running {
                    break;
                }
            }
        })
        .detach();

        // Background scan
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let options = match filter {
                        DedupFilter::All => zero::dedup::DedupOptions::default(),
                        DedupFilter::Images => zero::dedup::DedupOptions::images(),
                        DedupFilter::Videos => zero::dedup::DedupOptions::videos(),
                        DedupFilter::Audio => zero::dedup::DedupOptions::audio(),
                        DedupFilter::Documents => zero::dedup::DedupOptions::documents(),
                        DedupFilter::Code => zero::dedup::DedupOptions::code(),
                        DedupFilter::Archives => zero::dedup::DedupOptions::archives(),
                    };
                    zero::dedup::find_duplicates_with_progress(&path, options, Some(progress))
                })
                .await;

            this.update(cx, |view, cx| {
                view.scanning = false;
                view.scan_complete = true;
                view.scan_status = None;

                if let Ok(result) = result {
                    view.groups = result
                        .groups
                        .into_iter()
                        .map(|g| {
                            // Use keeper() to determine which file to keep
                            let keeper = g.keeper().cloned();
                            DuplicateGroup {
                                hash: g.hash,
                                size: g.size,
                                files: if let Some(ref k) = keeper {
                                    // Reorder: put keeper first
                                    let mut files = vec![k.clone()];
                                    files.extend(
                                        g.files.into_iter().filter(|f| Some(f) != keeper.as_ref()),
                                    );
                                    files
                                } else {
                                    g.files
                                },
                                expanded: false,
                            }
                        })
                        .collect();

                    view.sort_groups();
                }

                cx.emit(DedupEvent::ScanFinished);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn toggle_file_selection(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.selected_for_deletion.contains(&path) {
            self.selected_for_deletion.remove(&path);
        } else {
            self.selected_for_deletion.insert(path);
        }
        cx.notify();
    }

    /// Auto-select all duplicates for deletion (keep the keeper in each group).
    pub(super) fn select_all_duplicates(&mut self, cx: &mut Context<Self>) {
        self.selected_for_deletion.clear();
        for group in &self.groups {
            // Skip index 0 (the keeper)
            for file in group.files.iter().skip(1) {
                self.selected_for_deletion.insert(file.clone());
            }
        }
        cx.notify();
    }

    /// Clear all selections.
    pub(super) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selected_for_deletion.clear();
        cx.notify();
    }

    pub(super) fn selected_bytes(&self) -> u64 {
        self.selected_for_deletion
            .iter()
            .filter_map(|p| {
                self.groups
                    .iter()
                    .find(|g| g.files.contains(p))
                    .map(|g| g.size)
            })
            .sum()
    }

    pub(super) fn total_wasted(&self) -> u64 {
        self.groups
            .iter()
            .map(|g| g.size * (g.files.len() as u64 - 1))
            .sum()
    }

    pub(super) fn delete_selected(&mut self, cx: &mut Context<Self>) {
        if self.selected_for_deletion.is_empty() || self.deleting {
            return;
        }

        self.deleting = true;
        cx.notify();

        let paths: Vec<PathBuf> = self.selected_for_deletion.iter().cloned().collect();

        cx.spawn(async move |this, cx| {
            let _deleted = cx
                .background_executor()
                .spawn(async move {
                    let mut count = 0usize;
                    for path in &paths {
                        if trash::delete(path).is_ok() {
                            count += 1;
                        }
                    }
                    count
                })
                .await;

            this.update(cx, |view, cx| {
                view.deleting = false;
                view.start_scan(cx);
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn set_sort(&mut self, sort: DedupSort, cx: &mut Context<Self>) {
        self.sort_order = sort;
        self.sort_groups();
        cx.notify();
    }

    pub(super) fn sort_groups(&mut self) {
        match self.sort_order {
            DedupSort::Savings => {
                self.groups.sort_by(|a, b| {
                    let a_waste = a.size * (a.files.len() as u64 - 1);
                    let b_waste = b.size * (b.files.len() as u64 - 1);
                    b_waste.cmp(&a_waste)
                });
            }
            DedupSort::Size => {
                self.groups.sort_by_key(|b| std::cmp::Reverse(b.size));
            }
            DedupSort::Count => {
                self.groups
                    .sort_by_key(|b| std::cmp::Reverse(b.files.len()));
            }
            DedupSort::Name => {
                self.groups.sort_by(|a, b| {
                    let a_name = a
                        .files
                        .first()
                        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
                    let b_name = b
                        .files
                        .first()
                        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
                    a_name.cmp(&b_name)
                });
            }
        }
    }
}
