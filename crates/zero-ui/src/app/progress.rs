use std::sync::Arc;
use std::time::Duration;

use gpui::*;

use zero::dedup::DedupProgress;
use zero::prelude::AtomicProgress;
use zero::scanner::CrawlProgress;

use crate::ui::{BannerData, BannerKind};

use super::ZeroApp;

impl ZeroApp {
    /// Start polling an `AtomicProgress` and updating the banner.
    #[allow(dead_code)]
    pub fn start_progress_polling(
        &mut self,
        kind: BannerKind,
        message: String,
        progress: Arc<AtomicProgress>,
        cx: &mut Context<Self>,
    ) {
        self.banner = Some(BannerData {
            kind,
            message,
            bytes_done: 0,
            bytes_total: 0,
            files_done: 0,
            files_total: 0,
            phase: None,
            indeterminate: false,
            on_cancel: None,
        });
        cx.notify();

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;

                let done = this
                    .update(cx, |app, cx| {
                        let done = progress.bytes_done();
                        let total = progress.bytes_total();
                        let files_done = progress.files_done() as u64;
                        let files_total = progress.files_total() as u64;

                        let is_finished = total > 0 && done >= total;

                        if let Some(banner) = &mut app.banner {
                            banner.bytes_done = done;
                            banner.bytes_total = total;
                            banner.files_done = files_done;
                            banner.files_total = files_total;

                            // Show progress in phase text
                            if total > 0 {
                                let pct = (done as f64 / total as f64 * 100.0).min(100.0);
                                banner.phase = Some(format!(
                                    "{:.0}% · {} / {}",
                                    pct,
                                    crate::ui::format_bytes(done),
                                    crate::ui::format_bytes(total),
                                ));
                            } else if files_done > 0 {
                                banner.phase = Some(format!(
                                    "{} files",
                                    crate::ui::format_number(files_done),
                                ));
                            }
                        }

                        cx.notify();
                        is_finished
                    })
                    .unwrap_or(true);

                if done {
                    break;
                }
            }

            this.update(cx, |app, cx| {
                app.banner = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Start polling a `CrawlProgress` (file/dir/byte counters, no total).
    pub fn start_crawl_progress_polling(
        &mut self,
        kind: BannerKind,
        message: String,
        progress: Arc<CrawlProgress>,
        on_cancel: Option<Arc<dyn Fn() + Send + Sync>>,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(message = %message, "start_crawl_progress_polling");
        self.banner = Some(BannerData {
            kind,
            message,
            bytes_done: 0,
            bytes_total: 0,
            files_done: 0,
            files_total: 0,
            phase: Some("Scanning...".to_string()),
            indeterminate: true,
            on_cancel,
        });
        cx.notify();

        cx.spawn(async move |this, cx| {
            let mut ticks: u64 = 0;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;

                let should_stop = this
                    .update(cx, |app, cx| {
                        // Banner was cleared — stop polling
                        let Some(banner) = &mut app.banner else {
                            tracing::debug!("poll: banner gone, stopping");
                            return true;
                        };

                        let files = progress.files() as u64;
                        let bytes = progress.bytes();
                        banner.files_done = files;
                        banner.bytes_done = bytes;

                        // Show real numbers in the phase text
                        if files > 0 {
                            let files_str = crate::ui::format_number(files);
                            let bytes_str = crate::ui::format_bytes(bytes);
                            banner.phase = Some(format!("{} files · {}", files_str, bytes_str));
                        }

                        // Log every ~3s (20 ticks × 150ms)
                        ticks += 1;
                        if ticks == 1 || ticks.is_multiple_of(20) {
                            tracing::info!(
                                files,
                                bytes = %crate::ui::format_bytes(bytes),
                                "indexing progress"
                            );
                        }

                        cx.notify();
                        false
                    })
                    .unwrap_or_else(|_| {
                        tracing::warn!("poll: entity update failed, stopping");
                        true
                    });

                if should_stop {
                    break;
                }
            }
        })
        .detach();
    }

    /// Start polling a `DedupProgress` (phased: scanning → hashing → done).
    pub fn start_dedup_progress_polling(
        &mut self,
        kind: BannerKind,
        message: String,
        progress: Arc<DedupProgress>,
        cx: &mut Context<Self>,
    ) {
        self.banner = Some(BannerData {
            kind,
            message,
            bytes_done: 0,
            bytes_total: 0,
            files_done: 0,
            files_total: 0,
            phase: Some("Scanning...".to_string()),
            indeterminate: true,
            on_cancel: None,
        });
        cx.notify();

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;

                let should_stop = this
                    .update(cx, |app, cx| {
                        // Banner was cleared by clear_banner() — stop polling
                        let Some(banner) = &mut app.banner else {
                            return true;
                        };

                        let phase = progress.phase.load(std::sync::atomic::Ordering::Acquire);
                        let files = progress
                            .files_found
                            .load(std::sync::atomic::Ordering::Relaxed)
                            as u64;

                        banner.files_done = files;

                        match phase {
                            0 => {
                                banner.indeterminate = true;
                                banner.phase = Some(format!(
                                    "Scanning... {} files",
                                    crate::ui::format_number(files),
                                ));
                            }
                            1 => {
                                let pct = progress.hash_percent();
                                let done = progress
                                    .bytes_hashed
                                    .load(std::sync::atomic::Ordering::Relaxed);
                                let total = progress
                                    .bytes_to_hash
                                    .load(std::sync::atomic::Ordering::Relaxed);
                                banner.indeterminate = false;
                                banner.bytes_done = done;
                                banner.bytes_total = total;
                                banner.phase = Some(format!(
                                    "Hashing {:.0}% · {} / {}",
                                    pct,
                                    crate::ui::format_bytes(done),
                                    crate::ui::format_bytes(total),
                                ));
                            }
                            _ => {
                                banner.indeterminate = false;
                                banner.phase = Some("Finishing...".to_string());
                            }
                        }

                        cx.notify();
                        false
                    })
                    .unwrap_or(true);

                if should_stop {
                    break;
                }
            }
        })
        .detach();
    }

    /// Clear the progress banner.
    pub fn clear_banner(&mut self, cx: &mut Context<Self>) {
        self.banner = None;
        cx.notify();
    }
}
