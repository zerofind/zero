//! Watch command handlers (dev/testing tools)

use anyhow::Result;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use zero::output::Outputter;
use zero_watcher::{FileWatchConfig, FileWatcher, UsbWatcher};

use crate::cli::WatchCommands;

pub fn cmd_watch(out: &Outputter, watch_cmd: WatchCommands) -> Result<()> {
    match watch_cmd {
        WatchCommands::Files {
            paths,
            debounce_ms,
            no_recursive,
        } => {
            cmd_watch_files(out, paths, debounce_ms, !no_recursive)?;
        }
        WatchCommands::Usb { filter } => {
            cmd_watch_usb(out, filter)?;
        }
        WatchCommands::Latency { path, iterations } => {
            cmd_watch_latency(out, path, iterations)?;
        }
    }
    Ok(())
}
fn cmd_watch_files(
    out: &Outputter,
    paths: Vec<PathBuf>,
    debounce_ms: u64,
    recursive: bool,
) -> anyhow::Result<()> {
    let config = FileWatchConfig {
        debounce_ms,
        recursive,
        ..Default::default()
    };

    let mut watcher = FileWatcher::with_config(config)?;

    for path in &paths {
        watcher.watch(path)?;
        out.info(&format!("Watching: {}", path.display()));
    }

    out.newline();
    out.info("Press Ctrl+C to stop");
    out.newline();

    loop {
        if let Some(event) = watcher.next_event_timeout(Duration::from_millis(100)) {
            for path in &event.paths {
                out.info(&format!(
                    "{}: {:?}",
                    path.strip_prefix(&event.watch_root)
                        .unwrap_or(path)
                        .display(),
                    event.kind
                ));
            }
        }
    }
}

fn cmd_watch_usb(out: &Outputter, filter: Vec<String>) -> anyhow::Result<()> {
    let config = zero_watcher::UsbWatchConfig {
        volume_filter: filter,
        serial_filter: vec![],
    };

    let mut watcher = UsbWatcher::with_config(config)?;

    out.info("Watching for USB mount/unmount events");
    out.newline();
    out.info("Currently mounted volumes:");
    for vol in UsbWatcher::current_volumes() {
        out.indented(&format!("{}", vol.display()));
    }
    out.newline();
    out.info("Press Ctrl+C to stop");
    out.newline();

    loop {
        if let Some(event) = watcher.next_event_timeout(Duration::from_millis(100)) {
            let status = match event.kind {
                zero_watcher::UsbEventKind::Mounted => "connected",
                zero_watcher::UsbEventKind::Unmounted => "disconnected",
                zero_watcher::UsbEventKind::Unmounting => "disconnecting",
            };

            let name = event.volume_name.as_deref().unwrap_or("unknown");

            out.info(&format!(
                "{} ({}): {}",
                event.mount_point.display(),
                name,
                status
            ));
        }
    }
}

fn cmd_watch_latency(out: &Outputter, path: PathBuf, iterations: usize) -> anyhow::Result<()> {
    use std::fs;

    out.header("Measuring file watcher latency");
    out.newline();
    out.info(&format!("Watch path: {}", path.display()));
    out.info(&format!("Iterations: {}", iterations));
    out.newline();

    // Create a temp file path
    let test_file = path.join(".zero_latency_test");

    // Ensure the path exists
    if !path.exists() {
        fs::create_dir_all(&path)?;
    }

    let config = FileWatchConfig {
        debounce_ms: 0, // No debouncing for latency test
        recursive: false,
        ignore_patterns: vec![], // Don't ignore our test file
    };

    let mut watcher = FileWatcher::with_config(config)?;
    watcher.watch(&path)?;

    // Wait for watcher to be ready
    std::thread::sleep(Duration::from_millis(100));

    let mut latencies = Vec::with_capacity(iterations);

    for i in 0..iterations {
        // Clean up any previous test file
        let _ = fs::remove_file(&test_file);
        std::thread::sleep(Duration::from_millis(50));

        // Drain any pending events
        while watcher.try_next_event().is_some() {}

        // Time the write -> event detection
        let start = Instant::now();
        fs::write(&test_file, format!("test iteration {}", i))?;

        // Wait for the event
        let mut detected = false;
        while start.elapsed() < Duration::from_secs(5) {
            if let Some(event) = watcher.next_event_timeout(Duration::from_millis(10))
                && event
                    .paths
                    .iter()
                    .any(|p| p.ends_with(".zero_latency_test"))
            {
                let latency = start.elapsed();
                latencies.push(latency);
                out.info(&format!(
                    "  Iteration {}: {:>6.2}ms ({:?})",
                    i + 1,
                    latency.as_secs_f64() * 1000.0,
                    event.kind
                ));
                detected = true;
                break;
            }
        }

        if !detected {
            out.info(&format!("  Iteration {}: TIMEOUT (>5s)", i + 1));
        }

        // Brief pause between iterations
        std::thread::sleep(Duration::from_millis(50));
    }

    // Clean up
    let _ = fs::remove_file(&test_file);

    // Print statistics
    if !latencies.is_empty() {
        out.newline();
        out.header("Results");

        let min = latencies.iter().min().unwrap();
        let max = latencies.iter().max().unwrap();
        let sum: Duration = latencies.iter().sum();
        let avg = sum / latencies.len() as u32;

        let mut sorted = latencies.clone();
        sorted.sort();
        let median = sorted[sorted.len() / 2];
        let p95_idx = ((sorted.len() as f64 * 0.95) as usize).min(sorted.len() - 1);
        let p95 = sorted[p95_idx];

        out.info(&format!("  Min:    {:>6.2}ms", min.as_secs_f64() * 1000.0));
        out.info(&format!("  Max:    {:>6.2}ms", max.as_secs_f64() * 1000.0));
        out.info(&format!("  Avg:    {:>6.2}ms", avg.as_secs_f64() * 1000.0));
        out.info(&format!(
            "  Median: {:>6.2}ms",
            median.as_secs_f64() * 1000.0
        ));
        out.info(&format!("  P95:    {:>6.2}ms", p95.as_secs_f64() * 1000.0));
        out.newline();
        out.info(&format!(
            "  Success rate: {}/{}",
            latencies.len(),
            iterations
        ));
    } else {
        out.newline();
        out.error("No events detected!");
    }

    Ok(())
}
