//! Automation command handlers

use anyhow::Result;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use zero::automation::{AutomationEvent, Executor, ExecutorConfig};
use zero::cache::Settings as AutomationSettings;
use zero::cache::{ControlDb, NewAutomation, PathMapping, Triggers};
use zero::output::*;
use zero::{cmd_error, cmd_success};
use zero_watcher::{FileWatchConfig, FileWatcher, UsbWatcher};

use crate::cli::AutomationCommands;

pub fn cmd_automation(out: &Outputter, auto_cmd: AutomationCommands) -> Result<()> {
    match auto_cmd {
        AutomationCommands::List => cmd_automation_list(out),
        AutomationCommands::Show { id } => cmd_automation_show(out, id),
        AutomationCommands::Create {
            name,
            template,
            sources,
            dests,
            device_serial,
            volume_name,
            dest_path,
            on_mount,
            on_change,
            verify,
            delete_orphans,
            debounce_ms,
        } => cmd_automation_create(
            out,
            name,
            template,
            sources,
            dests,
            device_serial,
            volume_name,
            dest_path,
            on_mount,
            on_change,
            verify,
            delete_orphans,
            debounce_ms,
        ),
        AutomationCommands::Delete { id } => cmd_automation_delete(out, id),
        AutomationCommands::Run { id } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(cmd_automation_run(out, id))
        }
        AutomationCommands::History { id, limit } => cmd_automation_history(out, id, limit),
        AutomationCommands::Daemon => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(cmd_automation_daemon(out))
        }
    }
}
fn cmd_automation_list(out: &Outputter) -> anyhow::Result<()> {
    let start = Instant::now();
    let db = ControlDb::open()?;
    let autos = db.list_automations()?;

    let duration_ms = start.elapsed().as_millis() as u64;

    if out.is_json() {
        let automations: Vec<AutomationInfo> = autos
            .iter()
            .map(|auto| AutomationInfo {
                id: auto.id,
                name: auto.name.clone(),
                enabled: auto.enabled,
                dest_device_serial: auto.dest_device_serial.clone(),
                dest_volume_name: auto.dest_volume_name.clone(),
                dest_path: auto.dest_path.clone(),
                triggers: AutomationTriggers {
                    on_mount: auto.triggers.on_mount,
                    on_change: auto.triggers.on_change,
                    on_schedule: auto.triggers.on_schedule.clone(),
                },
                paths_count: auto.paths.len(),
                created_at: auto.created_at,
                updated_at: auto.updated_at,
            })
            .collect();

        let total = automations.len();
        let data = AutomationListData { automations, total };
        let response = Response::success("automation list", duration_ms, data);
        out.result(&response);
        return Ok(());
    }

    // Human-readable output
    if autos.is_empty() {
        out.info("No automations configured");
        return Ok(());
    }

    out.header("Automations");
    out.newline();

    for auto in &autos {
        let status = if auto.enabled { "✓" } else { "○" };
        let dest = auto
            .dest_volume_name
            .as_deref()
            .or(auto.dest_path.as_deref())
            .unwrap_or("(no destination)");

        out.info(&format!(
            "{} [{}] {} → {}",
            status, auto.id, auto.name, dest
        ));

        let triggers: Vec<&str> = [
            if auto.triggers.on_mount {
                Some("on_mount")
            } else {
                None
            },
            if auto.triggers.on_change {
                Some("on_change")
            } else {
                None
            },
        ]
        .into_iter()
        .flatten()
        .collect();

        out.indented(&format!(
            "{} path(s), triggers: {}",
            auto.paths.len(),
            triggers.join(", ")
        ));
    }

    Ok(())
}

fn cmd_automation_show(out: &Outputter, id: i64) -> anyhow::Result<()> {
    let start = Instant::now();
    let db = ControlDb::open()?;
    let auto = match db.get_automation(id)? {
        Some(a) => a,
        None => {
            let msg = format!("Automation not found: {}", id);
            cmd_error!(
                out,
                "automation show",
                start.elapsed().as_millis() as u64,
                "NOT_FOUND",
                msg
            );
            if out.is_json() {
                return Ok(());
            }
            return Err(anyhow::anyhow!("Automation not found: {}", id));
        }
    };

    // Get last run info
    let last_run = db.get_latest_run(id)?;

    let duration_ms = start.elapsed().as_millis() as u64;

    if out.is_json() {
        let last_run_info = last_run.map(|run| {
            let (files_total, bytes_transferred, errors) = run
                .result
                .as_ref()
                .map(|r| {
                    let total =
                        r.summary.files_added + r.summary.files_modified + r.summary.files_deleted;
                    (
                        Some(total),
                        Some(r.summary.bytes_transferred),
                        r.errors.clone(),
                    )
                })
                .unwrap_or((None, None, vec![]));

            AutomationRunInfo {
                id: run.id,
                status: run.status.as_str().to_string(),
                trigger: run.trigger.map(|t| t.as_str().to_string()),
                started_at: run.started_at,
                completed_at: run.completed_at,
                files_total,
                bytes_transferred,
                errors,
            }
        });

        let data = AutomationShowData {
            id: auto.id,
            name: auto.name.clone(),
            enabled: auto.enabled,
            dest_device_serial: auto.dest_device_serial.clone(),
            dest_volume_name: auto.dest_volume_name.clone(),
            dest_path: auto.dest_path.clone(),
            triggers: AutomationTriggers {
                on_mount: auto.triggers.on_mount,
                on_change: auto.triggers.on_change,
                on_schedule: auto.triggers.on_schedule.clone(),
            },
            paths: auto
                .paths
                .iter()
                .map(|p| AutomationPath {
                    source: p.source.clone(),
                    dest: p.dest.clone(),
                    exclude: p.exclude.clone(),
                })
                .collect(),
            settings: AutomationSettingsData {
                verify: auto.settings.verify,
                delete_orphans: auto.settings.delete_orphans,
                notify: auto.settings.notify,
                debounce_ms: auto.settings.debounce_ms,
            },
            created_at: auto.created_at,
            updated_at: auto.updated_at,
            last_run: last_run_info,
        };

        let response = Response::success("automation show", duration_ms, data);
        out.result(&response);
        return Ok(());
    }

    // Human-readable output
    out.header(&format!("Automation: {}", auto.name));
    out.newline();

    out.kv("ID", auto.id.to_string());
    out.kv("Enabled", if auto.enabled { "yes" } else { "no" });

    if let Some(ref serial) = auto.dest_device_serial {
        out.kv("Device Serial", serial.clone());
    }
    if let Some(ref name) = auto.dest_volume_name {
        out.kv("Volume Name", name.clone());
    }
    if let Some(ref path) = auto.dest_path {
        out.kv("Dest Path", path.clone());
    }

    out.newline();
    out.info("Triggers:");
    if auto.triggers.on_mount {
        out.indented("• On USB mount");
    }
    if auto.triggers.on_change {
        out.indented("• On file change");
    }
    if let Some(ref schedule) = auto.triggers.on_schedule {
        out.indented(&format!("• On schedule: {}", schedule));
    }

    out.newline();
    out.info("Paths:");
    for path in &auto.paths {
        let dest = if path.dest.is_empty() {
            "(root)".to_string()
        } else {
            path.dest.clone()
        };
        out.indented(&format!("{} → {}", path.source, dest));
        if !path.exclude.is_empty() {
            out.indented(&format!("  exclude: {}", path.exclude.join(", ")));
        }
    }

    out.newline();
    out.info("Settings:");
    out.indented(&format!("• Verify: {}", auto.settings.verify));
    out.indented(&format!(
        "• Delete orphans: {}",
        auto.settings.delete_orphans
    ));
    out.indented(&format!("• Notify: {}", auto.settings.notify));

    // Show last run if available
    if let Some(run) = last_run {
        out.newline();
        out.info("Last Run:");
        out.indented(&format!("• Status: {:?}", run.status));
        if let Some(result) = &run.result {
            out.indented(&format!(
                "• Files: {} added, {} modified, {} deleted",
                result.summary.files_added,
                result.summary.files_modified,
                result.summary.files_deleted
            ));
            out.indented(&format!(
                "• Transferred: {}",
                format_bytes(result.summary.bytes_transferred)
            ));
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_automation_create(
    out: &Outputter,
    name: String,
    template: Option<String>,
    sources: Vec<PathBuf>,
    dests: Vec<String>,
    device_serial: Option<String>,
    volume_name: Option<String>,
    dest_path: Option<PathBuf>,
    on_mount: bool,
    on_change: bool,
    verify: bool,
    delete_orphans: bool,
    debounce_ms: u64,
) -> anyhow::Result<()> {
    use zero::templates::get_template;

    // Resolve sources from template or direct input
    let (resolved_sources, template_excludes): (Vec<PathBuf>, Vec<String>) =
        if let Some(template_id) = &template {
            let tmpl = get_template(template_id)
                .ok_or_else(|| anyhow::anyhow!("Template not found: {}", template_id))?;

            let resolved = tmpl.resolve();

            // Check if template is valid
            let missing = resolved.missing_required_sources();
            if !missing.is_empty() {
                out.error(&format!(
                    "Template '{}' has missing required sources:",
                    template_id
                ));
                for src in &missing {
                    out.indented(&format!("✗ ~/{}", src.source.path));
                }
                anyhow::bail!("Cannot create automation with invalid template");
            }

            // Get existing source paths
            let existing: Vec<PathBuf> = resolved
                .existing_sources()
                .iter()
                .map(|s| s.path.clone())
                .collect();

            out.info(&format!(
                "Using template '{}' with {} source(s)",
                tmpl.name,
                existing.len()
            ));
            if !resolved.detected_cloud_folders.is_empty() {
                out.info(&format!(
                    "Auto-excluding {} cloud folder(s)",
                    resolved.detected_cloud_folders.len()
                ));
            }

            (existing, resolved.excludes)
        } else if sources.is_empty() {
            anyhow::bail!("Either --template or --source must be specified");
        } else {
            (sources, Vec::new())
        };

    // Use resolved_sources instead of sources from here on
    let sources = resolved_sources;
    let _ = template_excludes; // TODO: Store excludes in automation settings
    let start = Instant::now();
    let db = ControlDb::open()?;

    // Build path mappings
    let paths: Vec<PathMapping> = sources
        .iter()
        .enumerate()
        .map(|(i, source)| PathMapping {
            source: source.to_string_lossy().to_string(),
            dest: dests.get(i).cloned().unwrap_or_default(),
            exclude: vec![],
        })
        .collect();

    let auto = db.create_automation(NewAutomation {
        name: name.clone(),
        dest_device_serial: device_serial,
        dest_volume_name: volume_name,
        dest_path: dest_path.map(|p| p.to_string_lossy().to_string()),
        triggers: Triggers {
            on_mount,
            on_change,
            on_schedule: None,
        },
        paths,
        settings: AutomationSettings {
            verify,
            delete_orphans,
            notify: true,
            debounce_ms,
        },
    })?;

    let duration_ms = start.elapsed().as_millis() as u64;

    let data = AutomationCreateData {
        id: auto.id,
        name: name.clone(),
    };
    cmd_success!(out, "automation create", duration_ms, data, {
        out.success(&format!(
            "Created automation '{}' with ID {}",
            name, auto.id
        ));
    });
    Ok(())
}

fn cmd_automation_delete(out: &Outputter, id: i64) -> anyhow::Result<()> {
    let start = Instant::now();
    let db = ControlDb::open()?;

    // Check it exists
    let auto = match db.get_automation(id)? {
        Some(a) => a,
        None => {
            let msg = format!("Automation not found: {}", id);
            cmd_error!(
                out,
                "automation delete",
                start.elapsed().as_millis() as u64,
                "NOT_FOUND",
                msg
            );
            if out.is_json() {
                return Ok(());
            }
            return Err(anyhow::anyhow!("Automation not found: {}", id));
        }
    };

    let name = auto.name.clone();
    db.delete_automation(id)?;

    let duration_ms = start.elapsed().as_millis() as u64;

    let data = AutomationDeleteData {
        id,
        name: name.clone(),
        deleted: true,
    };
    cmd_success!(out, "automation delete", duration_ms, data, {
        out.success(&format!("Deleted automation '{}' (ID {})", name, id));
    });
    Ok(())
}

async fn cmd_automation_run(out: &Outputter, id: i64) -> anyhow::Result<()> {
    let start = Instant::now();
    let db = ControlDb::open()?;

    // Check it exists
    let auto = match db.get_automation(id)? {
        Some(a) => a,
        None => {
            let msg = format!("Automation not found: {}", id);
            cmd_error!(
                out,
                "automation run",
                start.elapsed().as_millis() as u64,
                "NOT_FOUND",
                msg
            );
            if out.is_json() {
                return Ok(());
            }
            return Err(anyhow::anyhow!("Automation not found: {}", id));
        }
    };

    if !out.is_json() {
        out.info(&format!("Running automation '{}'...", auto.name));
    }

    let executor = Executor::with_db(db, ExecutorConfig::default());

    let run_id = executor
        .handle_event(AutomationEvent::Manual { automation_id: id })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run automation: {}", e))?;

    let duration_ms = start.elapsed().as_millis() as u64;

    if run_id.is_empty() {
        cmd_error!(
            out,
            "automation run",
            duration_ms,
            "DEST_UNAVAILABLE",
            "Automation did not start (destination may not be available)"
        );
        return Ok(());
    }

    // Get the run result
    let run = executor
        .db()
        .get_run(run_id[0])?
        .ok_or_else(|| anyhow::anyhow!("Run not found"))?;

    if out.is_json() {
        let result = run.result.as_ref().map(|r| AutomationRunResult {
            files_added: r.summary.files_added,
            files_modified: r.summary.files_modified,
            files_deleted: r.summary.files_deleted,
            files_unchanged: r.summary.files_unchanged,
            bytes_transferred: r.summary.bytes_transferred,
            errors: r.errors.clone(),
        });

        let data = AutomationRunData {
            automation_id: id,
            automation_name: auto.name.clone(),
            run_id: run.id,
            status: run.status.as_str().to_string(),
            result,
        };
        let response = Response::success("automation run", duration_ms, data);
        out.result(&response);
        return Ok(());
    }

    // Human-readable output
    match run.status {
        zero::cache::RunStatus::Success => {
            out.success("Automation completed successfully");
            if let Some(result) = &run.result {
                out.kv(
                    "Files synced",
                    format!(
                        "{} added, {} modified, {} deleted",
                        result.summary.files_added,
                        result.summary.files_modified,
                        result.summary.files_deleted
                    ),
                );
                out.kv(
                    "Bytes transferred",
                    format_bytes(result.summary.bytes_transferred),
                );
            }
        }
        zero::cache::RunStatus::Failed => {
            out.error("Automation failed");
            if let Some(result) = &run.result {
                for err in &result.errors {
                    out.indented(&format!("• {}", err));
                }
            }
        }
        zero::cache::RunStatus::Partial => {
            out.info("Automation partially completed (can be resumed)");
        }
        _ => {
            out.info(&format!("Automation status: {:?}", run.status));
        }
    }

    Ok(())
}

fn cmd_automation_history(out: &Outputter, id: i64, limit: i64) -> anyhow::Result<()> {
    let start = Instant::now();
    let db = ControlDb::open()?;

    // Check automation exists
    let auto = match db.get_automation(id)? {
        Some(a) => a,
        None => {
            let msg = format!("Automation not found: {}", id);
            cmd_error!(
                out,
                "automation history",
                start.elapsed().as_millis() as u64,
                "NOT_FOUND",
                msg
            );
            if out.is_json() {
                return Ok(());
            }
            return Err(anyhow::anyhow!("Automation not found: {}", id));
        }
    };

    let history = db.list_runs_for_automation(id, limit as usize)?;

    let duration_ms = start.elapsed().as_millis() as u64;

    if out.is_json() {
        let runs: Vec<AutomationRunInfo> = history
            .iter()
            .map(|run| {
                let (files_total, bytes_transferred, errors) = run
                    .result
                    .as_ref()
                    .map(|r| {
                        let total = r.summary.files_added
                            + r.summary.files_modified
                            + r.summary.files_deleted;
                        (
                            Some(total),
                            Some(r.summary.bytes_transferred),
                            r.errors.clone(),
                        )
                    })
                    .unwrap_or((None, None, vec![]));

                AutomationRunInfo {
                    id: run.id,
                    status: run.status.as_str().to_string(),
                    trigger: run.trigger.map(|t| t.as_str().to_string()),
                    started_at: run.started_at,
                    completed_at: run.completed_at,
                    files_total,
                    bytes_transferred,
                    errors,
                }
            })
            .collect();

        let total = runs.len();
        let data = AutomationHistoryData {
            automation_id: id,
            automation_name: auto.name.clone(),
            runs,
            total,
        };
        let response = Response::success("automation history", duration_ms, data);
        out.result(&response);
        return Ok(());
    }

    // Human-readable output
    if history.is_empty() {
        out.info(&format!("No run history for '{}'", auto.name));
        return Ok(());
    }

    out.header(&format!("Run History: {}", auto.name));
    out.newline();

    for run in &history {
        let status_icon = match run.status {
            zero::cache::RunStatus::Success => "✓",
            zero::cache::RunStatus::Failed => "✗",
            zero::cache::RunStatus::Partial => "◐",
            zero::cache::RunStatus::Running => "●",
            zero::cache::RunStatus::Cancelled => "○",
        };

        let trigger = run
            .trigger
            .map(|t| format!("{:?}", t))
            .unwrap_or_else(|| "unknown".to_string());

        out.info(&format!(
            "{} Run #{} ({}) - {:?}",
            status_icon, run.id, trigger, run.status
        ));

        if let Some(result) = &run.result {
            let total_files = result.summary.files_added
                + result.summary.files_modified
                + result.summary.files_deleted;
            out.indented(&format!(
                "{} files, {}",
                total_files,
                format_bytes(result.summary.bytes_transferred)
            ));
        }
    }

    Ok(())
}

async fn cmd_automation_daemon(out: &Outputter) -> anyhow::Result<()> {
    out.header("Starting automation daemon");
    out.info("Watching for USB mounts and file changes...");
    out.info("Press Ctrl+C to stop");
    out.newline();

    let db = ControlDb::open()?;

    // Mark any interrupted runs
    let executor = Executor::with_db(db, ExecutorConfig::default());
    let interrupted = executor.recover_interrupted_runs().await?;
    if interrupted > 0 {
        out.info(&format!("Recovered {} interrupted runs", interrupted));
    }

    // Start USB watcher
    let mut usb_watcher = UsbWatcher::new()?;

    // Collect paths to watch from automations with on_change trigger
    let autos = executor.db().list_enabled_automations()?;
    let mut file_watcher = FileWatcher::with_config(FileWatchConfig {
        debounce_ms: 500,
        recursive: true,
        ..Default::default()
    })?;

    let mut watch_paths = Vec::new();
    for auto in &autos {
        if auto.triggers.on_change {
            for path_mapping in &auto.paths {
                let path = PathBuf::from(&path_mapping.source);
                if path.exists() && !watch_paths.contains(&path) {
                    file_watcher.watch(&path)?;
                    watch_paths.push(path.clone());
                    out.indented(&format!("Watching: {}", path.display()));
                }
            }
        }
    }

    out.newline();

    // Event loop
    loop {
        // Check USB events
        if let Some(event) = usb_watcher.try_next_event() {
            match event.kind {
                zero_watcher::UsbEventKind::Mounted => {
                    out.info(&format!(
                        "{} ({}): connected",
                        event.mount_point.display(),
                        event.volume_name.as_deref().unwrap_or("unknown")
                    ));

                    if let Some(serial) = &event.device_serial {
                        let run_ids = executor
                            .handle_event(AutomationEvent::UsbMounted {
                                serial: serial.clone(),
                                mount_point: event.mount_point.clone(),
                                volume_name: event.volume_name.clone(),
                            })
                            .await;

                        match run_ids {
                            Ok(ids) if !ids.is_empty() => {
                                out.success(&format!("Started {} automation(s)", ids.len()));
                            }
                            Ok(_) => {}
                            Err(e) => {
                                out.error(&format!("Error: {}", e));
                            }
                        }
                    }
                }
                zero_watcher::UsbEventKind::Unmounted => {
                    out.info(&format!(
                        "{} ({}): disconnected",
                        event.mount_point.display(),
                        event.volume_name.as_deref().unwrap_or("unknown")
                    ));

                    if let Some(serial) = &event.device_serial {
                        let _ = executor
                            .handle_event(AutomationEvent::UsbUnmounted {
                                serial: serial.clone(),
                                mount_point: event.mount_point.clone(),
                            })
                            .await;
                    }
                }
                _ => {}
            }
        }

        // Check file events
        if let Some(event) = file_watcher.try_next_event() {
            let run_ids = executor
                .handle_event(AutomationEvent::FilesChanged {
                    watch_root: event.watch_root.clone(),
                    changed_paths: event.paths.clone(),
                })
                .await;

            match run_ids {
                Ok(ids) if !ids.is_empty() => {
                    out.info(&format!(
                        "File change in {} triggered {} automation(s)",
                        event.watch_root.display(),
                        ids.len()
                    ));
                }
                Ok(_) => {}
                Err(e) => {
                    out.error(&format!("Error handling file change: {}", e));
                }
            }
        }

        // Brief sleep to avoid busy-waiting
        std::thread::sleep(Duration::from_millis(50));
    }
}
