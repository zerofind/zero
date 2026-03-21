//! Service runner - main event loop for the service mode
//!
//! Handles:
//! - Reading JSON-RPC requests from stdin
//! - Processing requests via `ServiceHandler`
//! - Sending responses and events to stdout
//! - Managing file and USB watchers
//! - Graceful shutdown on SIGTERM/SIGINT

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use zero_watcher::{FileWatchConfig, FileWatcher};
#[cfg(target_os = "macos")]
use zero_watcher::{UsbEventKind, UsbWatcher};

use automation::{AutomationEvent, Executor, ExecutorConfig};
use cache::CacheDb;

use super::handler::ServiceHandler;
use super::logging::{ServiceLogger, setup_service_logging};
use super::protocol::{
    FileChangedParams, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, ServiceReadyParams,
    SyncCompletedParams, UsbMountedParams, UsbUnmountedParams,
};

/// Run the service in JSON-RPC mode
///
/// This is the main entry point for `zero service`.
/// It runs indefinitely until shutdown signal is received.
pub fn run_service(verbose: bool) -> anyhow::Result<()> {
    // Set up logging first
    let logger = setup_service_logging(verbose)?;
    logger.info("service", "Starting zero service");
    logger.info(
        "service",
        &format!("Version: {}", env!("CARGO_PKG_VERSION")),
    );

    // Set up shutdown signal handling
    let shutdown = Arc::new(AtomicBool::new(false));
    setup_signal_handlers(Arc::clone(&shutdown), logger.clone());

    // Run the async runtime
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async_service_loop(shutdown, logger))
}

/// Set up signal handlers for graceful shutdown
fn setup_signal_handlers(shutdown: Arc<AtomicBool>, logger: ServiceLogger) {
    // Handle SIGTERM and SIGINT for graceful shutdown
    let shutdown_clone = Arc::clone(&shutdown);
    let logger_clone = logger.clone();

    ctrlc::set_handler(move || {
        logger_clone.info("service", "Received shutdown signal");
        shutdown_clone.store(true, Ordering::SeqCst);
    })
    .expect("Failed to set signal handler");

    logger.debug("service", "Signal handlers installed");
}

/// Main async service loop
async fn async_service_loop(
    shutdown: Arc<AtomicBool>,
    logger: ServiceLogger,
) -> anyhow::Result<()> {
    // Initialize database
    logger.info("service", "Opening database");
    let db = CacheDb::open()?;

    // Create executor and recover interrupted runs
    logger.info("service", "Initializing automation executor");
    let executor = Executor::with_db(CacheDb::open()?, ExecutorConfig::default());

    let recovered = executor.recover_interrupted_runs().await?;
    if recovered > 0 {
        logger.info(
            "service",
            &format!("Recovered {recovered} interrupted runs"),
        );
    }

    let executor = Arc::new(executor);

    // Create service handler
    let handler =
        ServiceHandler::with_executor(Arc::new(db), Arc::clone(&executor), logger.clone());

    // Try to load search index
    let index_loaded = match handler.load_index().await {
        Ok(count) => {
            logger.info("service", &format!("Search index loaded: {count} files"));
            true
        }
        Err(e) => {
            logger.warn("service", &format!("Could not load search index: {e}"));
            false
        }
    };

    let file_count = handler.index_file_count().await;

    // Get automation count
    let automations_count = handler
        .db()
        .list_automations()
        .map_or(0, |a| a.len() as u64);

    // Start watchers
    let (event_tx, event_rx) = crossfire::mpsc::bounded_blocking_async::<ServiceEvent>(100);
    let watchers_active = start_watchers(Arc::clone(&executor), event_tx.clone(), logger.clone())
        .await
        .is_ok();

    // Send service ready notification
    let ready_params = ServiceReadyParams {
        version: env!("CARGO_PKG_VERSION").to_string(),
        index_loaded,
        file_count,
        watchers_active,
        automations_count,
        recovered_runs: recovered,
    };

    send_notification("service.ready", &ready_params);
    logger.info("service", "Service ready, entering main loop");

    // Set up stdin reader in separate thread
    let (stdin_tx, stdin_rx) = crossfire::mpsc::bounded_blocking_async::<String>(100);
    let shutdown_stdin = Arc::clone(&shutdown);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let reader = BufReader::new(stdin.lock());

        for line in reader.lines() {
            if shutdown_stdin.load(Ordering::SeqCst) {
                break;
            }

            match line {
                Ok(line) if !line.trim().is_empty() => {
                    if stdin_tx.send(line).is_err() {
                        break;
                    }
                }
                Ok(_) => continue, // Empty line
                Err(_) => break,   // EOF or error
            }
        }
    });

    // Update check state: random jitter so first check happens 0-60 min after start
    let update_jitter_secs = rand::random_range(0u64..3600);
    let mut last_update_check = Instant::now()
        .checked_sub(Duration::from_secs(
            86400u64.saturating_sub(update_jitter_secs),
        ))
        .unwrap_or_else(Instant::now);

    // Main event loop
    loop {
        if shutdown.load(Ordering::SeqCst) {
            logger.info("service", "Shutdown requested, exiting main loop");
            break;
        }

        tokio::select! {
            // Handle stdin requests
            Ok(line) = stdin_rx.recv() => {
                logger.trace("service", &format!("Received: {line}"));

                let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
                    Ok(request) => handler.handle_request(&request).await,
                    Err(e) => {
                        logger.warn("service", &format!("Parse error: {e}"));
                        JsonRpcResponse::error(
                            0,
                            super::protocol::JsonRpcError::parse_error(e.to_string()),
                        )
                    }
                };

                send_response(&response);
            }

            // Handle watcher events
            Ok(event) = event_rx.recv() => {
                handle_service_event(event, &logger);
            }

            // Periodic tasks (log rotation, health check, update check)
            () = tokio::time::sleep(Duration::from_mins(1)) => {
                // Rotate logs if needed (10MB max)
                if let Err(e) = logger.rotate_if_needed(10 * 1024 * 1024) {
                    logger.warn("service", &format!("Log rotation failed: {e}"));
                }

                // Check for updates every 24h
                if last_update_check.elapsed() > Duration::from_hours(24) {
                    last_update_check = Instant::now();
                    let update_logger = logger.clone();
                    tokio::task::spawn_blocking(move || {
                        match updater::check_latest() {
                            Ok(updater::UpdateStatus::Available { version }) => {
                                update_logger.info(
                                    "updater",
                                    &format!("Update available: v{version}"),
                                );
                                updater::record_check();
                                send_notification(
                                    "update.available",
                                    &serde_json::json!({ "version": version }),
                                );
                            }
                            Ok(updater::UpdateStatus::UpToDate) => {
                                update_logger.debug("updater", "Up to date");
                                updater::record_check();
                            }
                            Err(e) => {
                                update_logger.warn(
                                    "updater",
                                    &format!("Update check failed: {e}"),
                                );
                            }
                        }
                    });
                }
            }
        }
    }

    // Graceful shutdown
    logger.info("service", "Shutting down gracefully");
    send_notification("service.shutdown", &serde_json::json!({"reason": "signal"}));

    Ok(())
}

/// Events from watchers that need to be sent as notifications
#[derive(Debug)]
#[allow(dead_code)]
enum ServiceEvent {
    UsbMounted(UsbMountedParams),
    UsbUnmounted(UsbUnmountedParams),
    FileChanged(FileChangedParams),
    SyncCompleted(SyncCompletedParams),
}

/// Start file and USB watchers
async fn start_watchers(
    executor: Arc<Executor>,
    event_tx: crossfire::MTx<crossfire::mpsc::Array<ServiceEvent>>,
    logger: ServiceLogger,
) -> anyhow::Result<()> {
    // Start USB watcher (macOS only - uses DiskArbitration framework)
    #[cfg(target_os = "macos")]
    {
        let usb_tx = event_tx.clone();
        let usb_logger = logger.clone();
        let usb_executor = Arc::clone(&executor);

        std::thread::spawn(move || {
            let mut watcher = match UsbWatcher::new() {
                Ok(w) => w,
                Err(e) => {
                    usb_logger.error("usb_watcher", &format!("Failed to start: {e}"));
                    return;
                }
            };

            usb_logger.info("usb_watcher", "Started");

            loop {
                if let Some(event) = watcher.next_event_timeout(Duration::from_millis(500)) {
                    let timestamp_ms = event.timestamp_ms;

                    match event.kind {
                        UsbEventKind::Mounted => {
                            usb_logger.info(
                                "usb_watcher",
                                &format!("USB mounted: {}", event.mount_point.display()),
                            );

                            let params = UsbMountedParams {
                                mount_point: event.mount_point.to_string_lossy().to_string(),
                                volume_name: event.volume_name.clone(),
                                device_serial: event.device_serial.clone(),
                                volume_uuid: event.volume_uuid.clone(),
                                capacity_bytes: event.capacity_bytes,
                                timestamp_ms,
                            };

                            let _ = usb_tx.send(ServiceEvent::UsbMounted(params));

                            // Trigger automations
                            if let Some(serial) = &event.device_serial {
                                let automation_event = AutomationEvent::UsbMounted {
                                    serial: serial.clone(),
                                    mount_point: event.mount_point.clone(),
                                    volume_name: event.volume_name.clone(),
                                };

                                let executor = Arc::clone(&usb_executor);
                                tokio::runtime::Handle::current().spawn(async move {
                                    let _ = executor.handle_event(automation_event).await;
                                });
                            }
                        }

                        UsbEventKind::Unmounted => {
                            usb_logger.info(
                                "usb_watcher",
                                &format!("USB unmounted: {}", event.mount_point.display()),
                            );

                            let params = UsbUnmountedParams {
                                mount_point: event.mount_point.to_string_lossy().to_string(),
                                volume_name: event.volume_name.clone(),
                                device_serial: event.device_serial.clone(),
                                timestamp_ms,
                            };

                            let _ = usb_tx.send(ServiceEvent::UsbUnmounted(params));
                        }

                        UsbEventKind::Unmounting => {
                            usb_logger.debug(
                                "usb_watcher",
                                &format!("USB unmounting: {}", event.mount_point.display()),
                            );
                        }
                    }
                }
            }
        });
    }

    #[cfg(not(target_os = "macos"))]
    logger.info("service", "USB watcher not available on this platform");

    // Start file watcher for automations with on_change trigger
    let file_tx = event_tx;
    let file_logger = logger.clone();
    let file_executor = Arc::clone(&executor);

    tokio::spawn(async move {
        // Get paths to watch from enabled automations
        let autos = match executor.db().list_enabled_automations() {
            Ok(a) => a,
            Err(e) => {
                file_logger.error("file_watcher", &format!("Failed to get automations: {e}"));
                return;
            }
        };

        let mut watch_paths: Vec<PathBuf> = Vec::new();
        for auto in &autos {
            if auto.triggers.on_change {
                for path_mapping in &auto.paths {
                    let path = PathBuf::from(&path_mapping.source);
                    if path.exists() && !watch_paths.contains(&path) {
                        watch_paths.push(path);
                    }
                }
            }
        }

        if watch_paths.is_empty() {
            file_logger.info(
                "file_watcher",
                "No paths to watch (no on_change automations)",
            );
            return;
        }

        let mut watcher = match FileWatcher::with_config(FileWatchConfig {
            debounce_ms: 500,
            recursive: true,
            ..Default::default()
        }) {
            Ok(w) => w,
            Err(e) => {
                file_logger.error("file_watcher", &format!("Failed to start: {e}"));
                return;
            }
        };

        for path in &watch_paths {
            if let Err(e) = watcher.watch(path) {
                file_logger.warn(
                    "file_watcher",
                    &format!("Failed to watch {}: {}", path.display(), e),
                );
            } else {
                file_logger.info("file_watcher", &format!("Watching: {}", path.display()));
            }
        }

        file_logger.info(
            "file_watcher",
            &format!("Started, watching {} paths", watch_paths.len()),
        );

        loop {
            if let Some(event) = watcher.next_event_timeout(Duration::from_millis(500)) {
                file_logger.debug(
                    "file_watcher",
                    &format!(
                        "File event: {:?} in {}",
                        event.kind,
                        event.watch_root.display()
                    ),
                );

                let params = FileChangedParams {
                    kind: format!("{:?}", event.kind),
                    paths: event
                        .paths
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect(),
                    watch_root: event.watch_root.to_string_lossy().to_string(),
                    timestamp_ms: event.timestamp_ms,
                };

                let _ = file_tx.send(ServiceEvent::FileChanged(params));

                // Trigger automations
                let automation_event = AutomationEvent::FilesChanged {
                    watch_root: event.watch_root,
                    changed_paths: event.paths,
                };

                let _ = file_executor.handle_event(automation_event).await;
            }
        }
    });

    logger.info("service", "Watchers started");
    Ok(())
}

/// Handle a service event by sending notification
fn handle_service_event(event: ServiceEvent, logger: &ServiceLogger) {
    match event {
        ServiceEvent::UsbMounted(params) => {
            logger.debug("service", "Sending usb_mounted event");
            send_notification("event.usb_mounted", &params);
        }
        ServiceEvent::UsbUnmounted(params) => {
            logger.debug("service", "Sending usb_unmounted event");
            send_notification("event.usb_unmounted", &params);
        }
        ServiceEvent::FileChanged(params) => {
            logger.debug("service", "Sending file_changed event");
            send_notification("event.file_changed", &params);
        }
        ServiceEvent::SyncCompleted(params) => {
            logger.debug("service", "Sending sync_completed event");
            send_notification("event.sync_completed", &params);
        }
    }
}

/// Send a JSON-RPC response to stdout
fn send_response(response: &JsonRpcResponse) {
    let line = response.to_line();
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(line.as_bytes());
    let _ = stdout.flush();
}

/// Send a JSON-RPC notification to stdout
fn send_notification<T: serde::Serialize>(method: &str, params: &T) {
    let notification = JsonRpcNotification::new(method, params);
    let line = notification.to_line();
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(line.as_bytes());
    let _ = stdout.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_notification_format() {
        // Just verify the notification can be created
        let params = ServiceReadyParams {
            version: "0.1.0".to_string(),
            index_loaded: true,
            file_count: Some(1000),
            watchers_active: true,
            automations_count: 5,
            recovered_runs: 0,
        };

        let notification = JsonRpcNotification::new("service.ready", &params);
        let line = notification.to_line();

        assert!(line.contains("service.ready"));
        assert!(line.ends_with('\n'));
    }
}
