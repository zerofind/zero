use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{h_flex, v_flex, ActiveTheme, Icon, IconName, Sizable as _};

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, PROGRESS_BAR_HEIGHT, PROGRESS_BAR_RADIUS, RADIUS};
use super::format::{format_bytes, format_number};

/// What kind of background operation is in progress.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum BannerKind {
    Sync,
    Index,
    Copy,
    Search,
    Dedup,
}

/// Data to drive the progress banner.
#[derive(Debug, Clone)]
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
}

impl BannerData {
    fn progress_fraction(&self) -> f32 {
        if self.bytes_total == 0 {
            return 0.0;
        }
        (self.bytes_done as f32 / self.bytes_total as f32).min(1.0)
    }

    fn icon(&self) -> IconName {
        match self.kind {
            BannerKind::Sync => IconName::Redo,
            BannerKind::Index => IconName::Search,
            BannerKind::Copy => IconName::Copy,
            BannerKind::Search => IconName::Search,
            BannerKind::Dedup => IconName::File,
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

        // Progress bar: either determinate (known total) or indeterminate (pulsing)
        let progress_bar = if indeterminate {
            // Pulsing indeterminate bar — fixed 30% width with animated opacity
            div()
                .w_full()
                .h(PROGRESS_BAR_HEIGHT)
                .rounded(PROGRESS_BAR_RADIUS)
                .bg(theme::surface_active(cx))
                .overflow_hidden()
                .child(
                    div()
                        .id("indeterminate-pulse")
                        .h_full()
                        .rounded(PROGRESS_BAR_RADIUS)
                        .bg(theme::brand_color())
                        .w(relative(0.3))
                        .with_animation(
                            "indeterminate-slide",
                            Animation::new(Duration::from_millis(1500))
                                .repeat()
                                .with_easing(bounce(ease_in_out)),
                            |el, delta| {
                                // Slide from left edge to right edge
                                el.ml(relative(delta * 0.7))
                            },
                        ),
                )
        } else {
            div()
                .w_full()
                .h(PROGRESS_BAR_HEIGHT)
                .rounded(PROGRESS_BAR_RADIUS)
                .bg(theme::surface_active(cx))
                .child(
                    div()
                        .h_full()
                        .rounded(PROGRESS_BAR_RADIUS)
                        .bg(theme::brand_color())
                        .w(relative(fraction)),
                )
        };

        h_flex()
            .w_full()
            .px_3()
            .py_2()
            .gap_3()
            .items_center()
            .bg(theme::surface_hover(cx))
            .rounded(RADIUS)
            .child(
                Icon::new(self.data.icon())
                    .xsmall()
                    .text_color(theme::brand_color()),
            )
            .child(
                v_flex()
                    .flex_1()
                    .gap_1()
                    .child(
                        div()
                            .text_size(FONT_SIZE_BODY)
                            .text_color(cx.theme().foreground)
                            .child(SharedString::from(self.data.message)),
                    )
                    .child(progress_bar),
            )
            .when(!detail.is_empty(), |el| {
                el.child(
                    div()
                        .text_size(FONT_SIZE_CAPTION)
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(detail)),
                )
            })
    }
}

// format_number and format_bytes are in ui/format.rs, re-exported from ui/mod.rs
