use gpui::*;
use gpui_component::input::InputState;

// -- Events ------------------------------------------------------------------

pub enum ModalEvent {
    Saved,
    Dismissed,
}

impl EventEmitter<ModalEvent> for AutomationModal {}

/// Data for pre-populating the modal when editing an existing automation.
pub struct EditData {
    pub id: i64,
    pub name: String,
    pub sources: Vec<String>,
    pub dest: String,
    pub on_mount: bool,
    pub on_change: bool,
    pub verify: bool,
    pub delete_orphans: bool,
}

// -- View --------------------------------------------------------------------

pub struct AutomationModal {
    pub(super) editing_id: Option<i64>,
    pub(super) name_input: Entity<InputState>,
    pub(super) source_input: Entity<InputState>,
    pub(super) dest_input: Entity<InputState>,
    pub(super) on_mount: bool,
    pub(super) on_change: bool,
    pub(super) verify: bool,
    pub(super) delete_orphans: bool,
    pub(super) sources: Vec<String>,
    pub(super) saving: bool,
    pub(super) error: Option<String>,
    pub(super) focus_handle: FocusHandle,
}

impl AutomationModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            editing_id: None,
            name_input: cx.new(|cx| InputState::new(window, cx).placeholder("Automation name")),
            source_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("~/Documents, ~/Photos")),
            dest_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("/Volumes/Backup or ~/Backup")),
            on_mount: true,
            on_change: false,
            verify: true,
            delete_orphans: false,
            sources: Vec::new(),
            saving: false,
            error: None,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn edit(data: EditData, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            editing_id: Some(data.id),
            name_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Automation name")
                    .default_value(&data.name)
            }),
            source_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("~/Documents, ~/Photos")),
            dest_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("/Volumes/Backup or ~/Backup")
                    .default_value(&data.dest)
            }),
            on_mount: data.on_mount,
            on_change: data.on_change,
            verify: data.verify,
            delete_orphans: data.delete_orphans,
            sources: data.sources,
            saving: false,
            error: None,
            focus_handle: cx.focus_handle(),
        }
    }

    pub(super) fn dismiss(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("automations: modal dismiss");
        cx.emit(ModalEvent::Dismissed);
    }

    pub(super) fn add_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!("automations: add source");
        let val = self.source_input.read(cx).value().to_string();
        let val = val.trim().to_string();
        if val.is_empty() {
            return;
        }
        // Expand ~ to home directory
        let expanded = if val.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                val.replacen('~', &home.to_string_lossy(), 1)
            } else {
                val
            }
        } else {
            val
        };
        if !self.sources.contains(&expanded) {
            self.sources.push(expanded);
        }
        self.source_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        cx.notify();
    }

    pub(super) fn remove_source(&mut self, idx: usize, cx: &mut Context<Self>) {
        tracing::debug!(idx, "automations: remove source");
        if idx < self.sources.len() {
            self.sources.remove(idx);
            cx.notify();
        }
    }

    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        tracing::debug!("automations: modal save");
        let name = self
            .name_input
            .read(cx)
            .value()
            .to_string()
            .trim()
            .to_string();
        if name.is_empty() {
            self.error = Some("Name is required.".to_string());
            cx.notify();
            return;
        }
        if self.sources.is_empty() {
            self.error = Some("Add at least one source folder.".to_string());
            cx.notify();
            return;
        }
        let dest_raw = self
            .dest_input
            .read(cx)
            .value()
            .to_string()
            .trim()
            .to_string();
        if dest_raw.is_empty() {
            self.error = Some("Destination is required.".to_string());
            cx.notify();
            return;
        }

        let dest = if dest_raw.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                dest_raw.replacen('~', &home.to_string_lossy(), 1)
            } else {
                dest_raw.clone()
            }
        } else {
            dest_raw.clone()
        };

        self.saving = true;
        self.error = None;
        cx.notify();

        let new_auto = self.build_new_automation(name, dest);
        let editing_id = self.editing_id;

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { Self::save_to_db(editing_id, new_auto).await })
                .await;

            this.update(cx, |view, cx| {
                view.saving = false;
                match result {
                    Ok(()) => cx.emit(ModalEvent::Saved),
                    Err(e) => {
                        view.error = Some(e);
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn build_new_automation(
        &self,
        name: String,
        dest: String,
    ) -> zero::cache::automations::NewAutomation {
        let triggers = zero::cache::automations::Triggers {
            on_mount: self.on_mount,
            on_change: self.on_change,
            on_schedule: None,
        };

        let paths: Vec<zero::cache::automations::PathMapping> = self
            .sources
            .iter()
            .map(|s| zero::cache::automations::PathMapping {
                source: s.clone(),
                dest: String::new(),
                exclude: Vec::new(),
            })
            .collect();

        let settings = zero::cache::automations::Settings {
            verify: self.verify,
            delete_orphans: self.delete_orphans,
            notify: true,
            debounce_ms: 5000,
        };

        // If path starts with /Volumes/, extract volume name
        let (dest_path, dest_volume_name) = if dest.starts_with("/Volumes/") {
            let vol_name = dest
                .strip_prefix("/Volumes/")
                .and_then(|s| s.split('/').next())
                .unwrap_or(&dest)
                .to_string();
            (Some(dest.clone()), Some(vol_name))
        } else {
            (Some(dest), None)
        };

        zero::cache::automations::NewAutomation {
            name,
            dest_device_serial: None,
            dest_volume_name,
            dest_path,
            triggers,
            paths,
            settings,
        }
    }

    async fn save_to_db(
        editing_id: Option<i64>,
        new: zero::cache::automations::NewAutomation,
    ) -> Result<(), String> {
        let db = zero::cache::CacheDb::open().map_err(|e| format!("Database error: {e}"))?;

        if let Some(id) = editing_id {
            db.update_automation(id, new)
                .map_err(|e| format!("Failed to update: {e}"))?;
        } else {
            db.create_automation(new)
                .map_err(|e| format!("Failed to create: {e}"))?;
        }

        Ok(())
    }
}
