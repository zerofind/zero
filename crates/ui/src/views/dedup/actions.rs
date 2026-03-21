use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::*;
use gpui_component::table::TableState;

use dedup::{DedupProgress, DedupStreamEvent};
use search::IndexManager;

use crate::ui::format::{format_bytes, format_number};

use super::delegate::DedupDelegate;
use super::{DedupEvent, DedupFilter, DedupView};

impl DedupView {
    pub(super) fn start_scan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!("dedup: start scan");
        let Some(path) = self.scan_path.clone() else {
            return;
        };
        self.scanning = true;
        self.scan_complete = false;
        self.scan_status = Some("Scanning...".to_string());
        self.table = None;
        cx.notify();

        let progress = Arc::new(DedupProgress::new());
        cx.emit(DedupEvent::ScanStarted(progress.clone()));

        let filter = self.active_filter;
        let manager = self.search.read(cx).shared_manager();
        let (tx, rx) = crossfire::mpsc::unbounded_blocking();

        // Background: run the streaming dedup (uses index when available)
        let progress_bg = progress.clone();
        cx.background_executor()
            .spawn(async move {
                let options = filter_to_options(filter);
                let mgr_ref: Option<&IndexManager> = if manager.total_file_count() > 0 {
                    Some(&manager)
                } else {
                    None
                };
                if let Err(e) =
                    dedup::find_duplicates_streaming(&path, options, progress_bg, tx, mgr_ref)
                {
                    tracing::error!(error = %e, "dedup: streaming scan failed");
                }
            })
            .detach();

        // Create the table with an empty delegate
        let delegate = DedupDelegate::new();
        let table = cx.new(|cx| TableState::new(delegate, window, cx).col_selectable(false));
        self.table = Some(table.clone());

        // Foreground: consume channel events, update delegate incrementally
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;

                // Drain all available events
                let mut batch = Vec::new();
                while let Ok(event) = rx.try_recv() {
                    batch.push(event);
                }

                let is_done = this
                    .update(cx, |view, cx| {
                        let mut done = false;
                        let mut groups_added = false;

                        table.update(cx, |state, _| {
                            let d = state.delegate_mut();
                            for event in batch {
                                match event {
                                    DedupStreamEvent::ScanProgress { files_found, .. } => {
                                        view.scan_status = Some(format!(
                                            "Scanning... {} files",
                                            format_number(files_found as u64),
                                        ));
                                    }
                                    DedupStreamEvent::SizeGroupsReady {
                                        group_count,
                                        bytes_to_hash,
                                        ..
                                    } => {
                                        view.scan_status = Some(format!(
                                            "Hashing {} groups ({})...",
                                            format_number(group_count as u64),
                                            format_bytes(bytes_to_hash),
                                        ));
                                    }
                                    DedupStreamEvent::GroupVerified(g) => {
                                        // Reorder: keeper (shortest path) first
                                        let keeper = g.keeper().cloned();
                                        let files = if let Some(ref k) = keeper {
                                            let mut reordered = vec![k.clone()];
                                            reordered.extend(
                                                g.files
                                                    .into_iter()
                                                    .filter(|f| Some(f) != keeper.as_ref()),
                                            );
                                            reordered
                                        } else {
                                            g.files
                                        };
                                        if d.push_group(g.size, files) {
                                            groups_added = true;
                                        }
                                    }
                                    DedupStreamEvent::HashProgress {
                                        bytes_hashed,
                                        bytes_total,
                                    } => {
                                        let pct = if bytes_total > 0 {
                                            (bytes_hashed as f64 / bytes_total as f64) * 100.0
                                        } else {
                                            0.0
                                        };
                                        view.scan_status = Some(format!(
                                            "Hashing... {:.0}% ({} / {})",
                                            pct,
                                            format_bytes(bytes_hashed),
                                            format_bytes(bytes_total),
                                        ));
                                    }
                                    DedupStreamEvent::Complete { .. } => {
                                        done = true;
                                    }
                                    DedupStreamEvent::Error(e) => {
                                        tracing::warn!(error = %e, "dedup stream error");
                                    }
                                }
                            }
                            if groups_added {
                                d.sort_by_savings_desc();
                            }
                        });

                        cx.notify();
                        done
                    })
                    .unwrap_or(true);

                if is_done {
                    this.update(cx, |view, cx| {
                        view.scanning = false;
                        view.scan_complete = true;
                        view.scan_status = None;
                        cx.emit(DedupEvent::ScanFinished);
                        cx.notify();
                    })
                    .ok();
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn select_all_duplicates(&mut self, cx: &mut Context<Self>) {
        if let Some(table) = &self.table {
            table.update(cx, |state, cx| {
                state.delegate_mut().select_all_duplicates();
                cx.notify();
            });
        }
        cx.notify();
    }

    pub(super) fn clear_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(table) = &self.table {
            table.update(cx, |state, cx| {
                state.delegate_mut().clear_selection();
                cx.notify();
            });
        }
        cx.notify();
    }

    pub(super) fn selected_count(&self, cx: &App) -> usize {
        self.table
            .as_ref()
            .map(|t| t.read(cx).delegate().selected.len())
            .unwrap_or(0)
    }

    pub(super) fn selected_bytes(&self, cx: &App) -> u64 {
        self.table
            .as_ref()
            .map(|t| t.read(cx).delegate().selected_bytes())
            .unwrap_or(0)
    }

    pub(super) fn total_wasted(&self, cx: &App) -> u64 {
        self.table
            .as_ref()
            .map(|t| t.read(cx).delegate().total_wasted())
            .unwrap_or(0)
    }

    pub(super) fn group_count(&self, cx: &App) -> usize {
        self.table
            .as_ref()
            .map(|t| t.read(cx).delegate().groups.len())
            .unwrap_or(0)
    }

    pub(super) fn delete_selected(&mut self, cx: &mut Context<Self>) {
        let Some(table) = self.table.clone() else {
            return;
        };
        let paths: Vec<PathBuf> = table.read(cx).delegate().selected.iter().cloned().collect();
        if paths.is_empty() || self.deleting {
            return;
        }

        tracing::debug!(count = paths.len(), "dedup: delete selected");
        self.deleting = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let deleted_paths: HashSet<PathBuf> = cx
                .background_executor()
                .spawn(async move {
                    let mut deleted = HashSet::new();
                    for path in &paths {
                        if crate::platform::trash::move_to_trash(path).is_ok() {
                            deleted.insert(path.clone());
                        }
                    }
                    deleted
                })
                .await;

            this.update(cx, |view, cx| {
                view.deleting = false;
                if let Some(table) = &view.table {
                    table.update(cx, |state, cx| {
                        state.delegate_mut().remove_deleted(&deleted_paths);
                        cx.notify();
                    });
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn filter_to_options(filter: DedupFilter) -> dedup::DedupOptions {
    match filter {
        DedupFilter::All => dedup::DedupOptions::default(),
        DedupFilter::Images => dedup::DedupOptions::images(),
        DedupFilter::Videos => dedup::DedupOptions::videos(),
        DedupFilter::Audio => dedup::DedupOptions::audio(),
        DedupFilter::Documents => dedup::DedupOptions::documents(),
        DedupFilter::Code => dedup::DedupOptions::code(),
        DedupFilter::Archives => dedup::DedupOptions::archives(),
    }
}
