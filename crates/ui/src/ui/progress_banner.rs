use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable as _, h_flex, v_flex};

use super::format::{format_bytes, format_number};
use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, ICON_XS, PROGRESS_BAR_HEIGHT, RADIUS};

/// What kind of background operation is in progress.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum BannerKind {
    Sync,
    Index,
    Copy,
    Search,
    Dedup,
    Cleanup,
}

/// Data to drive the progress banner.
#[derive(Clone)]
pub struct BannerData {
    pub kind: BannerKind,
    pub message: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    /// Optional phase text (e.g. "Scanning", "Hashing 45%")
    pub phase: Option<String>,
    /// True when there's no known total (crawl/scan) — show pulsing bar
    pub indeterminate: bool,
    /// Optional cancel callback — when present, a cancel button is shown
    pub on_cancel: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl BannerData {
    fn progress_fraction(&self) -> f32 {
        if self.bytes_total == 0 {
            return 0.0;
        }
        (self.bytes_done as f32 / self.bytes_total as f32).min(1.0)
    }

    #[allow(dead_code)]
    fn icon(&self) -> IconName {
        match self.kind {
            BannerKind::Sync => IconName::Redo,
            BannerKind::Index => IconName::Search,
            BannerKind::Copy => IconName::Copy,
            BannerKind::Search => IconName::Search,
            BannerKind::Dedup => IconName::File,
            BannerKind::Cleanup => IconName::Delete,
        }
    }
}

/// Sticky banner showing progress of a background operation.
#[derive(IntoElement)]
pub struct ProgressBanner {
    data: BannerData,
}

impl ProgressBanner {
    pub fn new(data: BannerData) -> Self {
        Self { data }
    }
}

impl RenderOnce for ProgressBanner {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let fraction = self.data.progress_fraction();
        let indeterminate = self.data.indeterminate;

        // Build detail text: phase > byte progress > file count > empty
        let detail = if let Some(ref phase) = self.data.phase {
            phase.clone()
        } else if self.data.bytes_total > 0 {
            format!(
                "{} / {}",
                format_bytes(self.data.bytes_done),
                format_bytes(self.data.bytes_total),
            )
        } else if self.data.files_done > 0 {
            format!("{} files scanned", format_number(self.data.files_done))
        } else {
            String::new()
        };

        // Progress bar: only show for determinate progress (known total)
        let progress_bar = if !indeterminate && fraction > 0.0 {
            Some(
                div()
                    .w_full()
                    .h(PROGRESS_BAR_HEIGHT)
                    .bg(theme::surface_active(cx))
                    .child(
                        div()
                            .h_full()
                            .bg(theme::brand_color(cx))
                            .w(relative(fraction)),
                    ),
            )
        } else {
            None
        };

        let on_cancel = self.data.on_cancel.clone();

        v_flex()
            .w_full()
            .bg(theme::banner_bg(cx))
            .child(
                h_flex()
                    .w_full()
                    .px_3()
                    .py(px(6.0))
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .text_size(FONT_SIZE_BODY)
                            .text_color(cx.theme().foreground)
                            .child(SharedString::from(self.data.message)),
                    )
                    .when(!detail.is_empty(), |el| {
                        el.child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(cx.theme().muted_foreground)
                                .child(SharedString::from(detail)),
                        )
                    })
                    .when_some(on_cancel, |el, cancel_fn| {
                        el.child(
                            div()
                                .id("cancel-indexing")
                                .cursor_pointer()
                                .p_1()
                                .rounded(RADIUS)
                                .hover(|s| s.bg(theme::surface_active(cx)))
                                .on_click(move |_, _, _| {
                                    (cancel_fn)();
                                })
                                .child(
                                    Icon::new(IconName::Close)
                                        .with_size(ICON_XS)
                                        .text_color(cx.theme().muted_foreground),
                                ),
                        )
                    }),
            )
            .when_some(progress_bar, |el, bar| el.child(bar))
    }
}

// format_number and format_bytes are in ui/format.rs, re-exported from ui/mod.rs
