use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};

use crate::theme::{
    self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, ICON_SM, ICON_XS, RADIUS, brand_color,
};
use crate::ui::format::format_bytes;

// -- Types -------------------------------------------------------------------

#[derive(Debug, Clone)]
struct DriveInfo {
    name: String,
    mount_point: PathBuf,
    size_bytes: u64,
    free_bytes: u64,
    file_system: String,
    is_external: bool,
}

/// Status of a completed (or in-progress) sync run — mapped from `zero::cache::runs::RunStatus`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum RunStatus {
    Success,
    Failed,
    Running,
    Warning,
}

/// A single sync history entry — built from `zero::cache::runs::Run`.
#[derive(Debug, Clone)]
struct RunInfo {
    automation_name: String,
    status: RunStatus,
    file_count: u64,
    bytes_transferred: u64,
    /// Unix timestamp (seconds since epoch).
    timestamp: u64,
}

impl DriveInfo {
    fn used_bytes(&self) -> u64 {
        self.size_bytes.saturating_sub(self.free_bytes)
    }

    fn usage_fraction(&self) -> f32 {
        if self.size_bytes == 0 {
            return 0.0;
        }
        (self.used_bytes() as f64 / self.size_bytes as f64) as f32
    }
}

/// An automation that targets a specific drive — built from `zero::cache::automations::Automation`.
#[derive(Debug, Clone)]
struct DriveAutomation {
    name: String,
    /// The destination mount point this automation targets.
    dest_mount: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DriveTab {
    Drives,
    History,
}

// -- Events ------------------------------------------------------------------

pub enum DrivesPopoverEvent {
    #[allow(dead_code)]
    Dismiss,
    OpenPath(PathBuf),
    EjectDrive(PathBuf),
    #[allow(dead_code)]
    RunAutomation(String),
}

impl EventEmitter<DrivesPopoverEvent> for DrivesPopover {}

// -- View --------------------------------------------------------------------

pub struct DrivesPopover {
    drives: Vec<DriveInfo>,
    history: Vec<RunInfo>,
    automations: Vec<DriveAutomation>,
    active_tab: DriveTab,
    loading: bool,
    focus_handle: FocusHandle,
}

impl DrivesPopover {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let drives = Self::discover_drives();
        let mut view = Self {
            drives,
            history: Vec::new(),
            automations: Vec::new(),
            active_tab: DriveTab::Drives,
            loading: true,
            focus_handle: cx.focus_handle(),
        };
        view.load_from_db(cx);
        view
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.drives = Self::discover_drives();
        self.load_from_db(cx);
        cx.notify();
    }

    /// Asynchronously load automations and run history from the control DB.
    fn load_from_db(&mut self, cx: &mut Context<Self>) {
        self.loading = true;

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { Self::fetch_db_data() })
                .await;

            this.update(cx, |view, cx| {
                view.loading = false;
                match result {
                    Ok((automations, history)) => {
                        view.automations = automations;
                        view.history = history;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "drives popover DB load failed");
                        // Keep existing data on error
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Fetch automations + run history from the DB on a background thread.
    fn fetch_db_data() -> Result<(Vec<DriveAutomation>, Vec<RunInfo>), String> {
        let db =
            zero::cache::CacheDb::open().map_err(|e| format!("Failed to open database: {e}"))?;

        // Load automations and build name lookup + DriveAutomation list
        let automations = db
            .list_automations()
            .map_err(|e| format!("Failed to list automations: {e}"))?;

        let mut drive_autos = Vec::new();
        let mut name_by_id: Vec<(i64, String)> = Vec::new();

        for auto in &automations {
            name_by_id.push((auto.id, auto.name.clone()));

            // Build dest mount path from dest_path or dest_volume_name
            let dest = auto.dest_path.as_deref().map(PathBuf::from).or_else(|| {
                auto.dest_volume_name
                    .as_deref()
                    .map(|v| PathBuf::from(format!("/Volumes/{v}")))
            });

            if let Some(dest_mount) = dest {
                drive_autos.push(DriveAutomation {
                    name: auto.name.clone(),
                    dest_mount,
                });
            }
        }

        // Load run history: last 50 runs across all automations
        let mut history = Vec::new();
        for (auto_id, auto_name) in &name_by_id {
            let runs = db
                .list_runs_for_automation(*auto_id, 20)
                .unwrap_or_default();

            for run in runs {
                let status = match run.status {
                    zero::cache::runs::RunStatus::Success => RunStatus::Success,
                    zero::cache::runs::RunStatus::Failed => RunStatus::Failed,
                    zero::cache::runs::RunStatus::Running => RunStatus::Running,
                    zero::cache::runs::RunStatus::Partial
                    | zero::cache::runs::RunStatus::Cancelled => RunStatus::Warning,
                };

                // Extract file count and bytes from result summary if available
                let (file_count, bytes_transferred) = run
                    .result
                    .as_ref()
                    .map(|r| {
                        let files = r.summary.files_added
                            + r.summary.files_modified
                            + r.summary.files_deleted;
                        (files, r.summary.bytes_transferred)
                    })
                    .unwrap_or((0, 0));

                history.push(RunInfo {
                    automation_name: auto_name.clone(),
                    status,
                    file_count,
                    bytes_transferred,
                    timestamp: run.started_at as u64,
                });
            }
        }

        // Sort by timestamp descending (newest first)
        history.sort_by_key(|r| std::cmp::Reverse(r.timestamp));
        history.truncate(50);

        Ok((drive_autos, history))
    }

    /// Get automations whose destination is on the given mount point.
    fn automations_for_drive(&self, mount: &std::path::Path) -> Vec<&DriveAutomation> {
        let mount_str = format!("{}/", mount.to_string_lossy().trim_end_matches('/'));
        self.automations
            .iter()
            .filter(|a| {
                let dest = format!("{}/", a.dest_mount.to_string_lossy().trim_end_matches('/'));
                dest.starts_with(&mount_str) || mount_str.starts_with(&dest)
            })
            .collect()
    }

    fn discover_drives() -> Vec<DriveInfo> {
        let mut drives = Vec::new();

        // Add the home directory as "This Mac"
        if let Some(home) = dirs::home_dir()
            && let Ok(vol) = zero::disk::VolumeInfo::for_path(&home)
        {
            drives.push(DriveInfo {
                name: "This Mac".to_string(),
                mount_point: home,
                size_bytes: vol.size_bytes,
                free_bytes: vol.free_bytes,
                file_system: vol.file_system,
                is_external: false,
            });
        }

        // Discover external volumes
        if let Ok(all) = zero::disk::VolumeInfo::all() {
            for vol in all {
                if !vol.is_external() {
                    continue;
                }
                drives.push(DriveInfo {
                    name: vol.name.clone(),
                    mount_point: PathBuf::from(&vol.mount_point),
                    size_bytes: vol.size_bytes,
                    free_bytes: vol.free_bytes,
                    file_system: vol.file_system,
                    is_external: true,
                });
            }
        }

        drives
    }
}

// -- Rendering ---------------------------------------------------------------

impl Render for DrivesPopover {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        let internal: Vec<_> = self.drives.iter().filter(|d| !d.is_external).collect();
        let external: Vec<_> = self.drives.iter().filter(|d| d.is_external).collect();

        v_flex()
            .id("drives-popover")
            .track_focus(&self.focus_handle)
            .w(px(320.0))
            .max_h(px(480.0))
            .overflow_y_scroll()
            .bg(theme::surface_hover(cx))
            .border_1()
            .border_color(cx.theme().border)
            .rounded(RADIUS)
            .shadow_lg()
            .p_3()
            .gap_3()
            // Tab bar
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("tab-drives")
                            .label("Drives")
                            .compact()
                            .small()
                            .when(self.active_tab == DriveTab::Drives, |b| b.primary())
                            .when(self.active_tab != DriveTab::Drives, |b| b.ghost())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.active_tab = DriveTab::Drives;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("tab-history")
                            .label("History")
                            .compact()
                            .small()
                            .when(self.active_tab == DriveTab::History, |b| b.primary())
                            .when(self.active_tab != DriveTab::History, |b| b.ghost())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.active_tab = DriveTab::History;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("refresh-drives")
                            .ghost()
                            .compact()
                            .small()
                            .icon(IconName::Replace)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh(cx);
                            })),
                    ),
            )
            // Content
            .when(self.active_tab == DriveTab::Drives, |el| {
                el.when(!internal.is_empty(), |el| {
                    el.child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("LOCAL"),
                    )
                })
                .children(
                    internal
                        .iter()
                        .enumerate()
                        .map(|(i, d)| Self::render_drive(i, d, false, muted, cx)),
                )
                .when(!external.is_empty(), |el| {
                    el.child(
                        div()
                            .pt_2()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("EXTERNAL"),
                    )
                })
                .children(external.iter().enumerate().map(|(i, d)| {
                    let drive_autos = self.automations_for_drive(&d.mount_point);
                    Self::render_drive_with_automations(i, d, true, &drive_autos, muted, cx)
                }))
                .when(external.is_empty(), |el| {
                    el.child(
                        div()
                            .pt_2()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .child("No external drives connected"),
                    )
                })
            })
            .when(self.active_tab == DriveTab::History, |el| {
                if self.loading {
                    el.child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .child("Loading..."),
                    )
                } else if self.history.is_empty() {
                    el.child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .child("No sync history yet."),
                    )
                } else {
                    let grouped = Self::group_runs_by_date(&self.history);
                    let mut container = el;
                    for (label, runs) in grouped {
                        container = container
                            .child(
                                div()
                                    .pt_2()
                                    .text_size(FONT_SIZE_CAPTION)
                                    .text_color(muted)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(SharedString::from(label)),
                            )
                            .children(
                                runs.iter()
                                    .enumerate()
                                    .map(|(i, r)| Self::render_history_row(i, r, muted, cx)),
                            );
                    }
                    container
                }
            })
    }
}

impl DrivesPopover {
    fn render_drive(
        idx: usize,
        drive: &DriveInfo,
        is_external: bool,
        muted: Hsla,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon = if is_external {
            IconName::ExternalLink
        } else {
            IconName::Folder
        };

        let mount = drive.mount_point.clone();
        let eject_mount = drive.mount_point.clone();

        let capacity_label = if drive.size_bytes > 0 {
            format!(
                "{} free of {}",
                format_bytes(drive.free_bytes),
                format_bytes(drive.size_bytes)
            )
        } else {
            String::new()
        };

        let usage = drive.usage_fraction();
        let has_capacity = drive.size_bytes > 0;

        let prefix = if is_external { "ext" } else { "int" };

        div()
            .id(SharedString::from(format!("drive-{prefix}-{idx}")))
            .px_2()
            .py_2()
            .rounded(RADIUS)
            .cursor_pointer()
            .hover(|s| s.bg(theme::selection_active_bg(cx)))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.emit(DrivesPopoverEvent::OpenPath(mount.clone()));
                this.active_tab = DriveTab::Drives;
            }))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                gpui_component::Icon::new(icon)
                                    .size(ICON_SM)
                                    .text_color(muted),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(FONT_SIZE_BODY)
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(SharedString::from(drive.name.clone())),
                            )
                            .when(!drive.file_system.is_empty(), |el| {
                                el.child(
                                    div()
                                        .text_size(FONT_SIZE_CAPTION)
                                        .text_color(muted)
                                        .child(SharedString::from(drive.file_system.clone())),
                                )
                            })
                            .when(is_external, |el| {
                                el.child(
                                    Button::new(SharedString::from(format!(
                                        "eject-{prefix}-{idx}"
                                    )))
                                    .ghost()
                                    .compact()
                                    .small()
                                    .icon(IconName::Minus)
                                    .tooltip("Eject")
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        cx.emit(DrivesPopoverEvent::EjectDrive(
                                            eject_mount.clone(),
                                        ));
                                    })),
                                )
                            }),
                    )
                    // Capacity bar
                    .when(has_capacity, |el| {
                        el.child(
                            div()
                                .w_full()
                                .h(px(4.0))
                                .rounded(px(2.0))
                                .bg(cx.theme().secondary)
                                .child(
                                    div()
                                        .h_full()
                                        .rounded(px(2.0))
                                        .bg(if usage > 0.9 {
                                            cx.theme().danger
                                        } else {
                                            brand_color(cx)
                                        })
                                        .w(relative(usage)),
                                ),
                        )
                        .child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .child(SharedString::from(capacity_label)),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_drive_with_automations(
        idx: usize,
        drive: &DriveInfo,
        is_external: bool,
        automations: &[&DriveAutomation],
        muted: Hsla,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if automations.is_empty() {
            return Self::render_drive(idx, drive, is_external, muted, cx);
        }

        let drive_el = Self::render_drive(idx, drive, is_external, muted, cx);
        let auto_rows: Vec<_> = automations
            .iter()
            .enumerate()
            .map(|(ai, auto)| {
                let name = auto.name.clone();
                let run_name = auto.name.clone();
                h_flex()
                    .id(SharedString::from(format!("auto-{idx}-{ai}")))
                    .px_4()
                    .py_1()
                    .gap_2()
                    .items_center()
                    .child(Icon::new(IconName::Folder).size(ICON_XS).text_color(muted))
                    .child(
                        div()
                            .flex_1()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(SharedString::from(name)),
                    )
                    .child(
                        Button::new(SharedString::from(format!("run-auto-{idx}-{ai}")))
                            .icon(IconName::ChevronRight)
                            .ghost()
                            .compact()
                            .xsmall()
                            .tooltip("Run now")
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(DrivesPopoverEvent::RunAutomation(run_name.clone()));
                            })),
                    )
                    .into_any_element()
            })
            .collect();

        v_flex()
            .child(drive_el)
            .children(auto_rows)
            .into_any_element()
    }

    /// Group history runs by date label (Today, Yesterday, or formatted date).
    fn group_runs_by_date(runs: &[RunInfo]) -> Vec<(String, Vec<&RunInfo>)> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let today_start = now - (now % 86400);
        let yesterday_start = today_start - 86400;

        let mut groups: Vec<(String, Vec<&RunInfo>)> = Vec::new();

        for run in runs {
            let label = if run.timestamp >= today_start {
                "TODAY".to_string()
            } else if run.timestamp >= yesterday_start {
                "YESTERDAY".to_string()
            } else {
                let days_ago = (today_start - run.timestamp) / 86400;
                format!("{days_ago} DAYS AGO")
            };

            if let Some(group) = groups.iter_mut().find(|(l, _)| *l == label) {
                group.1.push(run);
            } else {
                groups.push((label, vec![run]));
            }
        }

        groups
    }

    fn render_history_row(
        idx: usize,
        run: &RunInfo,
        muted: Hsla,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (status_color, status_icon) = match run.status {
            RunStatus::Success => (hsla(0.35, 0.8, 0.45, 1.0), IconName::Check),
            RunStatus::Failed => (cx.theme().danger, IconName::Close),
            RunStatus::Running => (brand_color(cx), IconName::Replace),
            RunStatus::Warning => (hsla(0.1, 0.9, 0.55, 1.0), IconName::Minus),
        };

        let detail = format!(
            "{} files · {}",
            run.file_count,
            format_bytes(run.bytes_transferred)
        );
        let relative = format_relative_time(run.timestamp);

        h_flex()
            .id(SharedString::from(format!("history-{idx}")))
            .px_2()
            .py_1p5()
            .gap_2()
            .items_center()
            .rounded(RADIUS)
            .hover(|s| s.bg(theme::surface_hover(cx)))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(18.0))
                    .h(px(18.0))
                    .rounded_full()
                    .bg(status_color)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(status_icon)
                            .size(ICON_XS)
                            .text_color(gpui::white()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(FONT_SIZE_BODY)
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(SharedString::from(run.automation_name.clone())),
                    )
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .child(SharedString::from(detail)),
                    ),
            )
            .child(
                div()
                    .text_size(FONT_SIZE_CAPTION)
                    .text_color(muted)
                    .child(SharedString::from(relative)),
            )
            .into_any_element()
    }
}

/// Format a unix timestamp into a human-readable relative time string.
fn format_relative_time(epoch_secs: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if epoch_secs > now {
        return "just now".to_string();
    }

    let diff = now - epoch_secs;

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        let mins = diff / 60;
        format!("{mins}m ago")
    } else if diff < 86400 {
        let hours = diff / 3600;
        format!("{hours}h ago")
    } else {
        let days = diff / 86400;
        format!("{days}d ago")
    }
}
