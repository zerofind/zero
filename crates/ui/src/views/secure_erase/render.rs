use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::Input,
    v_flex,
};
use std::sync::atomic::Ordering;

use crate::theme::{
    self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, FONT_SIZE_DISPLAY, RADIUS_LG, RADIUS_SM, brand_color,
};
use crate::ui::format::format_bytes;

use super::view::{ErasePhase, SecureEraseView};

impl Render for SecureEraseView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        let content: AnyElement = match &self.phase {
            ErasePhase::Idle => self.render_idle(muted, cx).into_any_element(),
            ErasePhase::Confirming { .. } => self.render_confirming(muted, cx).into_any_element(),
            ErasePhase::Erasing => self.render_erasing(muted, cx).into_any_element(),
            ErasePhase::Complete { .. } => self.render_complete(muted, cx).into_any_element(),
            ErasePhase::Error(_) => self.render_error(muted, cx).into_any_element(),
            ErasePhase::Cancelled => self.render_cancelled(muted, cx).into_any_element(),
        };

        v_flex()
            .id("secure-erase-view")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(theme::content_bg(cx))
            .overflow_y_scroll()
            .p_6()
            .child(
                v_flex()
                    .max_w(px(560.0))
                    .mx_auto()
                    .gap_4()
                    .child(
                        div()
                            .text_size(FONT_SIZE_DISPLAY)
                            .font_weight(FontWeight::BOLD)
                            .child("Secure Erase"),
                    )
                    .child(div().text_size(FONT_SIZE_BODY).text_color(muted).child(
                        "Securely wipe external drives so data cannot be recovered. \
                                 Only external drives are shown.",
                    ))
                    .child(content),
            )
    }
}

impl SecureEraseView {
    pub(super) fn render_idle(&mut self, muted: Hsla, cx: &mut Context<Self>) -> impl IntoElement {
        let has_selection = self.selected_volume.is_some();
        let has_resume = self.resume_info.is_some();
        let resume_label = self
            .resume_info
            .as_ref()
            .map(|r| format!("Resume previous erase ({})", r.percent_str));

        v_flex()
            .gap_4()
            // Volume list
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_size(FONT_SIZE_BODY)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Select Volume"),
                    )
                    .when(self.volumes.is_empty(), |el| {
                        el.child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .child("No external volumes found. Connect an external drive."),
                        )
                    })
                    .children(self.volumes.iter().enumerate().map(|(i, vol)| {
                        let selected = self
                            .selected_volume
                            .as_ref()
                            .is_some_and(|s| s.mount_point == vol.mount_point);

                        let size_str = if vol.size_bytes > 0 {
                            format!(" \u{2022} {}", format_bytes(vol.size_bytes))
                        } else {
                            String::new()
                        };

                        let fs_str = if vol.file_system.is_empty() {
                            String::new()
                        } else {
                            format!(" \u{2022} {}", vol.file_system)
                        };

                        div()
                            .id(SharedString::from(format!("vol-{i}")))
                            .px_3()
                            .py_2()
                            .rounded(RADIUS_SM)
                            .cursor_pointer()
                            .border_1()
                            .when(selected, |el| {
                                el.border_color(brand_color(cx))
                                    .bg(theme::selection_active_bg(cx))
                            })
                            .when(!selected, |el| {
                                el.border_color(cx.theme().border)
                                    .hover(|s| s.bg(theme::surface_hover(cx)))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select_volume(i, cx);
                            }))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(FONT_SIZE_BODY)
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(SharedString::from(vol.name.clone())),
                                    )
                                    .child(
                                        div().text_size(FONT_SIZE_CAPTION).text_color(muted).child(
                                            SharedString::from(format!("{size_str}{fs_str}")),
                                        ),
                                    ),
                            )
                    })),
            )
            // Resume option
            .when(has_resume, |el| {
                let label = resume_label.unwrap_or_default();
                el.child(
                    div()
                        .px_3()
                        .py_2()
                        .rounded(RADIUS_SM)
                        .border_1()
                        .border_color(cx.theme().warning)
                        .bg(cx.theme().warning.opacity(0.08))
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(FONT_SIZE_CAPTION)
                                        .text_color(cx.theme().warning)
                                        .child(SharedString::from(label)),
                                )
                                .child(
                                    Button::new("resume-erase")
                                        .label("Resume")
                                        .compact()
                                        .xsmall()
                                        .primary()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.resume_erase(cx);
                                        })),
                                ),
                        ),
                )
            })
            // Level selector
            .when(has_selection, |el| {
                el.child(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_size(FONT_SIZE_BODY)
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Security Level"),
                        )
                        .child(h_flex().gap_2().children((0..3).map(|i| {
                            let selected = self.selected_level == i;
                            Button::new(SharedString::from(format!("level-{i}")))
                                .label(Self::level_name(i))
                                .compact()
                                .small()
                                .when(selected, gpui_component::button::ButtonVariants::primary)
                                .when(!selected, gpui_component::button::ButtonVariants::ghost)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.selected_level = i;
                                    cx.notify();
                                }))
                        })))
                        .child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .child(Self::level_desc(self.selected_level)),
                        ),
                )
            })
            // Erase button
            .when(has_selection, |el| {
                el.child(
                    h_flex().justify_end().child(
                        Button::new("start-erase")
                            .label("Erase Volume")
                            .compact()
                            .small()
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_confirm(window, cx);
                            })),
                    ),
                )
            })
    }

    pub(super) fn render_confirming(
        &mut self,
        muted: Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (target_name, target_size, level_name, pass_count) = match &self.phase {
            ErasePhase::Confirming {
                target_name,
                target_size,
                level_name,
                pass_count,
            } => (
                target_name.clone(),
                *target_size,
                level_name.clone(),
                *pass_count,
            ),
            _ => return v_flex().into_any_element(),
        };

        let name_matches = self
            .confirm_name_value
            .trim()
            .eq_ignore_ascii_case(target_name.trim());

        v_flex()
            .gap_4()
            .rounded(RADIUS_LG)
            .p_4()
            .border_1()
            .border_color(cx.theme().danger)
            .child(
                div()
                    .text_size(FONT_SIZE_BODY)
                    .font_weight(FontWeight::BOLD)
                    .child("Confirm Secure Erase"),
            )
            .child(
                div()
                    .text_size(FONT_SIZE_BODY)
                    .text_color(muted)
                    .child(SharedString::from(format!(
                        "You are about to securely erase \"{target_name}\" ({}).\n\
                         This will run {pass_count} pass(es) using the {level_name} method.\n\
                         ALL DATA WILL BE PERMANENTLY DESTROYED.",
                        format_bytes(target_size)
                    ))),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_size(FONT_SIZE_CAPTION).text_color(muted).child(
                        SharedString::from(format!("Type \"{target_name}\" to confirm:")),
                    ))
                    .child(Input::new(&self.confirm_name_input)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("cancel-erase")
                            .label("Cancel")
                            .compact()
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_confirm(cx);
                            })),
                    )
                    .child(
                        Button::new("confirm-erase")
                            .label("Erase Permanently")
                            .compact()
                            .small()
                            .primary()
                            .disabled(!name_matches)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.start_erase(cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_erasing(
        &mut self,
        muted: Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let pct = self.progress.percent.load(Ordering::Relaxed);
        let pass = self.progress.pass.load(Ordering::Relaxed);
        let total_passes = self.progress.total_passes.load(Ordering::Relaxed);
        let bad_blocks = self.progress.bad_blocks.load(Ordering::Relaxed);
        let speed_bps = self.progress.speed_bps.load(Ordering::Relaxed);
        let eta_seconds = self.progress.eta_seconds.load(Ordering::Relaxed);
        let verifying = self.progress.verifying.load(Ordering::Relaxed);

        let title = if verifying {
            format!("Verifying pass {pass}/{total_passes}... {pct}%")
        } else if total_passes > 0 {
            format!("Erasing pass {pass}/{total_passes}... {pct}%")
        } else {
            format!("Erasing... {pct}%")
        };

        // Format speed
        let speed_str = if speed_bps > 0 {
            format_speed(speed_bps)
        } else {
            String::new()
        };

        // Format ETA
        let eta_str = if eta_seconds > 0 {
            format_eta(eta_seconds)
        } else {
            String::new()
        };

        // Status line: "125.4 MB/s — 3m 12s remaining"
        let status_parts: Vec<String> = [speed_str, eta_str]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        let status_line = status_parts.join(" \u{2014} ");

        v_flex()
            .gap_4()
            .child(
                div()
                    .text_size(FONT_SIZE_BODY)
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(title)),
            )
            .child(
                // Progress bar
                div()
                    .w_full()
                    .h(px(8.0))
                    .rounded(px(4.0))
                    .bg(cx.theme().progress_bar)
                    .child(
                        div()
                            .h_full()
                            .rounded(px(4.0))
                            .bg(brand_color(cx))
                            .w(relative(f32::from(pct) / 100.0)),
                    ),
            )
            // Speed & ETA
            .when(!status_line.is_empty(), |el| {
                el.child(
                    div()
                        .text_size(FONT_SIZE_CAPTION)
                        .text_color(muted)
                        .child(SharedString::from(status_line)),
                )
            })
            // Bad blocks warning
            .when(bad_blocks > 0, |el| {
                el.child(
                    div()
                        .text_size(FONT_SIZE_CAPTION)
                        .text_color(cx.theme().warning)
                        .child(SharedString::from(format!(
                            "{bad_blocks} bad block(s) encountered"
                        ))),
                )
            })
            .child(
                div()
                    .text_size(FONT_SIZE_CAPTION)
                    .text_color(muted)
                    .child("Do not disconnect the drive during this operation."),
            )
            .child(
                h_flex().justify_end().child(
                    Button::new("cancel-erasing")
                        .label("Cancel")
                        .compact()
                        .small()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cancel_erase(cx);
                        })),
                ),
            )
    }

    pub(super) fn render_complete(
        &mut self,
        muted: Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (duration_secs, bytes_written) = match &self.phase {
            ErasePhase::Complete {
                duration_secs,
                bytes_written,
            } => (*duration_secs, *bytes_written),
            _ => (0, 0),
        };

        v_flex()
            .gap_4()
            .child(
                div()
                    .text_size(FONT_SIZE_BODY)
                    .font_weight(FontWeight::BOLD)
                    .child("Erase Complete"),
            )
            .child(
                div()
                    .text_size(FONT_SIZE_BODY)
                    .text_color(muted)
                    .child(SharedString::from(format!(
                        "Successfully erased {} in {}m {}s.",
                        format_bytes(bytes_written),
                        duration_secs / 60,
                        duration_secs % 60,
                    ))),
            )
            .child(
                h_flex().justify_end().child(
                    Button::new("done-erase")
                        .label("Done")
                        .compact()
                        .small()
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.reset(cx);
                        })),
                ),
            )
    }

    pub(super) fn render_error(&mut self, muted: Hsla, cx: &mut Context<Self>) -> impl IntoElement {
        let msg = match &self.phase {
            ErasePhase::Error(e) => e.clone(),
            _ => String::new(),
        };

        v_flex()
            .gap_4()
            .child(
                div()
                    .text_size(FONT_SIZE_BODY)
                    .font_weight(FontWeight::BOLD)
                    .child("Erase Failed"),
            )
            .child(
                div()
                    .text_size(FONT_SIZE_BODY)
                    .text_color(muted)
                    .child(SharedString::from(msg)),
            )
            .child(
                h_flex().justify_end().child(
                    Button::new("retry-erase")
                        .label("Try Again")
                        .compact()
                        .small()
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.reset(cx);
                        })),
                ),
            )
    }

    #[allow(clippy::unused_self)]
    pub(super) fn render_cancelled(
        &mut self,
        muted: Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .gap_4()
            .child(
                div()
                    .text_size(FONT_SIZE_BODY)
                    .font_weight(FontWeight::BOLD)
                    .child("Erase Cancelled"),
            )
            .child(div().text_size(FONT_SIZE_BODY).text_color(muted).child(
                "Progress saved \u{2014} you can resume later when the drive is reconnected.",
            ))
            .child(
                h_flex().justify_end().child(
                    Button::new("back-cancelled")
                        .label("Done")
                        .compact()
                        .small()
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.reset(cx);
                        })),
                ),
            )
    }
}

fn format_speed(bps: u64) -> String {
    const MB: u64 = 1_000_000;
    const GB: u64 = 1_000_000_000;

    if bps >= GB {
        format!("{:.1} GB/s", bps as f64 / GB as f64)
    } else if bps >= MB {
        format!("{:.1} MB/s", bps as f64 / MB as f64)
    } else {
        format!("{} KB/s", bps / 1_000)
    }
}

fn format_eta(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s remaining")
    } else if secs < 3600 {
        format!("{}m {}s remaining", secs / 60, secs % 60)
    } else {
        format!("{}h {}m remaining", secs / 3600, (secs % 3600) / 60)
    }
}
