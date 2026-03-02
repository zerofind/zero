use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use gpui::*;
use gpui_component::input::InputState;

use zero::disk::erase::state::EraseState as PersistentEraseState;

// -- Events ------------------------------------------------------------------

pub enum SecureEraseEvent {
    /// Erase operation started (for progress banner if needed).
    #[allow(dead_code)]
    Started,
    /// Erase operation finished.
    #[allow(dead_code)]
    Finished,
}

impl EventEmitter<SecureEraseEvent> for SecureEraseView {}

// -- Shared progress ---------------------------------------------------------

/// Atomic progress data shared between the background erase thread and UI.
pub(super) struct SharedEraseProgress {
    pub percent: AtomicU8,
    pub pass: AtomicUsize,
    pub total_passes: AtomicUsize,
    pub bad_blocks: AtomicU32,
    pub speed_bps: AtomicU64,
    pub eta_seconds: AtomicU64,
    pub verifying: AtomicBool,
}

impl SharedEraseProgress {
    fn new() -> Self {
        Self {
            percent: AtomicU8::new(0),
            pass: AtomicUsize::new(0),
            total_passes: AtomicUsize::new(0),
            bad_blocks: AtomicU32::new(0),
            speed_bps: AtomicU64::new(0),
            eta_seconds: AtomicU64::new(0),
            verifying: AtomicBool::new(false),
        }
    }

    fn reset(&self) {
        self.percent.store(0, Ordering::Relaxed);
        self.pass.store(0, Ordering::Relaxed);
        self.total_passes.store(0, Ordering::Relaxed);
        self.bad_blocks.store(0, Ordering::Relaxed);
        self.speed_bps.store(0, Ordering::Relaxed);
        self.eta_seconds.store(0, Ordering::Relaxed);
        self.verifying.store(false, Ordering::Relaxed);
    }
}

// -- Resume info -------------------------------------------------------------

/// Summary of a resumable erase found on disk.
#[derive(Debug, Clone)]
pub(super) struct ResumeInfo {
    pub percent_str: String,
    #[allow(dead_code)]
    pub level: String,
    pub state: PersistentEraseState,
}

// -- State machine -----------------------------------------------------------

#[derive(Debug, Clone)]
pub(super) enum ErasePhase {
    /// Select a volume and erase level.
    Idle,
    /// Show confirmation before erasing.
    Confirming {
        target_name: String,
        target_size: u64,
        level_name: String,
        pass_count: usize,
    },
    /// Erase in progress.
    Erasing,
    /// Erase completed successfully.
    Complete {
        duration_secs: u64,
        bytes_written: u64,
    },
    /// An error occurred.
    Error(String),
    /// Erase was cancelled — progress saved, user can resume later.
    Cancelled,
}

// -- View --------------------------------------------------------------------

pub struct SecureEraseView {
    pub(super) phase: ErasePhase,
    pub(super) selected_volume: Option<VolumeEntry>,
    pub(super) selected_level: usize, // 0=Quick, 1=Normal, 2=Strong
    pub(super) volumes: Vec<VolumeEntry>,
    pub(super) progress: Arc<SharedEraseProgress>,
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) resume_info: Option<ResumeInfo>,
    pub(super) confirm_name_input: Entity<InputState>,
    pub(super) confirm_name_value: String,
    pub(super) focus_handle: FocusHandle,
}

#[derive(Debug, Clone)]
pub(super) struct VolumeEntry {
    pub(super) name: String,
    pub(super) mount_point: PathBuf,
    pub(super) size_bytes: u64,
    pub(super) file_system: String,
    pub(super) is_external: bool,
}

impl SecureEraseView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let volumes = Self::discover_volumes();

        let confirm_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Type volume name to confirm"));

        cx.subscribe(
            &confirm_input,
            |this, _entity, ev: &gpui_component::input::InputEvent, cx| {
                if matches!(ev, gpui_component::input::InputEvent::Change) {
                    this.confirm_name_value = _entity.read(cx).value().to_string();
                    cx.notify();
                }
            },
        )
        .detach();

        Self {
            phase: ErasePhase::Idle,
            selected_volume: None,
            selected_level: 1, // Normal default
            volumes,
            progress: Arc::new(SharedEraseProgress::new()),
            cancelled: Arc::new(AtomicBool::new(false)),
            resume_info: None,
            confirm_name_input: confirm_input,
            confirm_name_value: String::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    fn discover_volumes() -> Vec<VolumeEntry> {
        let mut volumes = Vec::new();

        if let Ok(entries) = std::fs::read_dir("/Volumes") {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                // Skip the system volume
                if name == "Macintosh HD" || name == "Macintosh HD - Data" {
                    continue;
                }
                // Only show volumes that look external
                volumes.push(VolumeEntry {
                    name,
                    mount_point: path,
                    size_bytes: 0,
                    file_system: String::new(),
                    is_external: true,
                });
            }
        }

        // Try to enrich with VolumeInfo if available
        if let Ok(all) = zero::disk::VolumeInfo::all() {
            for vol in &all {
                if let Some(entry) = volumes
                    .iter_mut()
                    .find(|v| v.mount_point.to_string_lossy() == vol.mount_point)
                {
                    entry.size_bytes = vol.size_bytes;
                    entry.file_system = vol.file_system.clone();
                    entry.is_external = vol.is_external();
                }
            }
            // Remove non-external volumes
            volumes.retain(|v| v.is_external);
        }

        volumes
    }

    pub(super) fn level_name(idx: usize) -> &'static str {
        match idx {
            0 => "Quick",
            1 => "Normal",
            2 => "Strong",
            _ => "Normal",
        }
    }

    pub(super) fn level_desc(idx: usize) -> &'static str {
        match idx {
            0 => "Single zero pass (fastest)",
            1 => "Double random pass (recommended)",
            2 => "VSITR 7-pass (most secure)",
            _ => "",
        }
    }

    pub(super) fn level_passes(idx: usize) -> usize {
        match idx {
            0 => 1,
            1 => 2,
            2 => 7,
            _ => 2,
        }
    }

    pub(super) fn select_volume(&mut self, idx: usize, cx: &mut Context<Self>) {
        if let Some(vol) = self.volumes.get(idx).cloned() {
            // Check for resumable state for this volume
            self.resume_info = zero::cache::ControlDb::open()
                .ok()
                .and_then(|db| {
                    PersistentEraseState::find_by_volume_name_or_mount_point(
                        &db,
                        &vol.mount_point.to_string_lossy(),
                    )
                    .ok()
                    .flatten()
                })
                .map(|state| {
                    let percent_str = state.summary();
                    let level = state.settings().level.clone();
                    ResumeInfo {
                        percent_str,
                        level,
                        state,
                    }
                });
            self.selected_volume = Some(vol);
            cx.notify();
        }
    }

    pub(super) fn request_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(vol) = &self.selected_volume else {
            return;
        };
        self.confirm_name_value.clear();
        self.confirm_name_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.phase = ErasePhase::Confirming {
            target_name: vol.name.clone(),
            target_size: vol.size_bytes,
            level_name: Self::level_name(self.selected_level).to_string(),
            pass_count: Self::level_passes(self.selected_level),
        };
        cx.notify();
    }

    pub(super) fn cancel_confirm(&mut self, cx: &mut Context<Self>) {
        self.phase = ErasePhase::Idle;
        cx.notify();
    }

    pub(super) fn start_erase(&mut self, cx: &mut Context<Self>) {
        self.start_erase_inner(None, cx);
    }

    pub(super) fn resume_erase(&mut self, cx: &mut Context<Self>) {
        let resume_state = self.resume_info.as_ref().map(|r| r.state.clone());
        if resume_state.is_none() {
            return;
        }
        self.start_erase_inner(resume_state, cx);
    }

    fn start_erase_inner(
        &mut self,
        resume_state: Option<PersistentEraseState>,
        cx: &mut Context<Self>,
    ) {
        let Some(vol) = &self.selected_volume else {
            return;
        };

        let mount_point = vol.mount_point.clone();
        let level_idx = self.selected_level;
        let progress = self.progress.clone();
        let cancelled = self.cancelled.clone();

        self.phase = ErasePhase::Erasing;
        progress.reset();
        cancelled.store(false, Ordering::Relaxed);

        cx.emit(SecureEraseEvent::Started);

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    Self::run_erase(mount_point, level_idx, progress, cancelled, resume_state)
                })
                .await;

            this.update(cx, |view, cx| {
                match result {
                    Ok((duration_secs, bytes_written)) => {
                        view.phase = ErasePhase::Complete {
                            duration_secs,
                            bytes_written,
                        };
                        view.resume_info = None;
                    }
                    Err(e) if e.contains("cancelled") || e.contains("Cancelled") => {
                        view.phase = ErasePhase::Cancelled;
                    }
                    Err(e) => {
                        view.phase = ErasePhase::Error(e);
                    }
                }
                cx.emit(SecureEraseEvent::Finished);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn run_erase(
        mount_point: PathBuf,
        level_idx: usize,
        progress: Arc<SharedEraseProgress>,
        cancelled: Arc<AtomicBool>,
        resume_state: Option<PersistentEraseState>,
    ) -> Result<(u64, u64), String> {
        let volume = zero::disk::VolumeInfo::for_path(&mount_point)
            .map_err(|e| format!("Failed to get volume info: {e}"))?;

        let _target =
            zero::disk::erase::prepare_erase(&volume).map_err(|e| format!("Cannot erase: {e}"))?;

        let level = match level_idx {
            0 => zero::disk::erase::EraseLevel::Quick,
            2 => zero::disk::erase::EraseLevel::Strong,
            _ => zero::disk::erase::EraseLevel::Normal,
        };

        let options = zero::disk::erase::EraseOptions::new(level);
        let scheme = level.scheme();

        let result = zero::disk::erase::wipe::run_wipe_cancellable(
            &_target,
            &scheme,
            &options,
            resume_state,
            |p| {
                progress.percent.store(p.percent(), Ordering::Relaxed);
                progress.pass.store(p.pass, Ordering::Relaxed);
                progress
                    .total_passes
                    .store(p.total_passes, Ordering::Relaxed);
                progress.bad_blocks.store(p.bad_blocks, Ordering::Relaxed);
                progress.speed_bps.store(p.speed_bps, Ordering::Relaxed);
                progress.eta_seconds.store(p.eta_seconds, Ordering::Relaxed);
                progress.verifying.store(p.verifying, Ordering::Relaxed);
            },
            move || cancelled.load(Ordering::Relaxed),
        )
        .map_err(|e| format!("{e}"))?;

        Ok((result.duration.as_secs(), result.bytes_written))
    }

    pub(super) fn cancel_erase(&mut self, cx: &mut Context<Self>) {
        self.cancelled.store(true, Ordering::Relaxed);
        cx.notify();
    }

    pub(super) fn reset(&mut self, cx: &mut Context<Self>) {
        self.phase = ErasePhase::Idle;
        self.selected_volume = None;
        self.resume_info = None;
        self.volumes = Self::discover_volumes();
        cx.notify();
    }
}
