//! zero-watch - CLI for testing file and USB watchers
//!
//! Usage:
//!   zero-watch files /path/to/watch [/another/path ...]
//!   zero-watch usb       (macOS only)
//!   zero-watch all /path/to/watch [...]  (macOS only)

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing_subscriber::EnvFilter;
#[cfg(target_os = "macos")]
use zero_watcher::UsbWatcher;
use zero_watcher::{FileWatchConfig, FileWatcher};

#[derive(Parser)]
#[command(name = "zero-watch")]
#[command(about = "Watch for file system and USB mount/unmount events")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output events as JSON
    #[arg(long, global = true)]
    json: bool,

    /// Verbose output (includes latency measurements)
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Watch directories for file changes
    Files {
        /// Paths to watch
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Debounce delay in milliseconds
        #[arg(long, default_value = "100")]
        debounce_ms: u64,

        /// Don't watch subdirectories
        #[arg(long)]
        no_recursive: bool,
    },

    /// Watch for USB mount/unmount events (macOS only)
    #[cfg(target_os = "macos")]
    Usb {
        /// Only report events for volumes matching these names
        #[arg(long)]
        filter: Vec<String>,

        /// Only report events for devices with these serial numbers
        #[arg(long)]
        serial: Vec<String>,
    },

    /// Watch both files and USB events (macOS only)
    #[cfg(target_os = "macos")]
    All {
        /// Paths to watch for file changes
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Debounce delay in milliseconds
        #[arg(long, default_value = "100")]
        debounce_ms: u64,
    },

    /// Measure file event latency
    Latency {
        /// Path to watch
        path: PathBuf,

        /// Number of test iterations
        #[arg(long, default_value = "10")]
        iterations: usize,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.verbose {
        "zero_watcher=debug,info"
    } else {
        "zero_watcher=info,warn"
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
        )
        .with_target(true)
        .init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        platform = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        "zero-watcher starting"
    );

    match cli.command {
        Commands::Files {
            paths,
            debounce_ms,
            no_recursive,
        } => watch_files(paths, debounce_ms, !no_recursive, cli.json),

        #[cfg(target_os = "macos")]
        Commands::Usb { filter, serial } => watch_usb(filter, serial, cli.json),

        #[cfg(target_os = "macos")]
        Commands::All { paths, debounce_ms } => watch_all(paths, debounce_ms, cli.json),

        Commands::Latency { path, iterations } => measure_latency(path, iterations),
    }
}

fn watch_files(paths: Vec<PathBuf>, debounce_ms: u64, recursive: bool, json: bool) -> Result<()> {
    let config = FileWatchConfig {
        debounce_ms,
        recursive,
        ..Default::default()
    };

    let mut watcher = FileWatcher::with_config(config)?;

    for path in &paths {
        watcher.watch(path)?;
        if !json {
            println!("Watching: {}", path.display());
        }
    }

    if !json {
        println!("\nPress Ctrl+C to stop\n");
    }

    loop {
        if let Some(event) = watcher.next_event_timeout(Duration::from_millis(100)) {
            if json {
                println!("{}", event.to_json()?);
            } else {
                for path in &event.paths {
                    println!(
                        "{:?}: {}",
                        event.kind,
                        path.strip_prefix(&event.watch_root)
                            .unwrap_or(path)
                            .display()
                    );
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn watch_usb(filter: Vec<String>, serial: Vec<String>, json: bool) -> Result<()> {
    let config = zero_watcher::UsbWatchConfig {
        volume_filter: filter,
        serial_filter: serial,
    };

    let mut watcher = UsbWatcher::with_config(config)?;

    if !json {
        println!("Watching for USB mount/unmount events");
        println!("\nCurrently mounted volumes:");
        for vol in UsbWatcher::current_volumes() {
            println!("  {}", vol.display());
        }
        println!("\nPress Ctrl+C to stop\n");
    }

    loop {
        if let Some(event) = watcher.next_event_timeout(Duration::from_millis(100)) {
            if json {
                println!("{}", event.to_json()?);
            } else {
                println!(
                    "{:?}: {}{}",
                    event.kind,
                    event.mount_point.display(),
                    event
                        .volume_name
                        .as_ref()
                        .map(|n| format!(" ({})", n))
                        .unwrap_or_default()
                );

                if let Some(ref serial) = event.device_serial {
                    println!("   Serial: {}", serial);
                }
                if let Some(ref fs) = event.file_system {
                    println!("   FS: {}", fs);
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn watch_all(paths: Vec<PathBuf>, debounce_ms: u64, json: bool) -> Result<()> {
    let file_config = FileWatchConfig {
        debounce_ms,
        recursive: true,
        ..Default::default()
    };

    let mut file_watcher = FileWatcher::with_config(file_config)?;
    let mut usb_watcher = UsbWatcher::new()?;

    for path in &paths {
        file_watcher.watch(path)?;
        if !json {
            println!("Watching files: {}", path.display());
        }
    }

    if !json {
        println!("Watching USB events");
        println!("\nPress Ctrl+C to stop\n");
    }

    loop {
        // Check file events
        if let Some(event) = file_watcher.try_next_event() {
            if json {
                let watch_event = zero_watcher::WatchEvent::File(event);
                println!("{}", watch_event.to_json()?);
            } else {
                for path in &event.paths {
                    println!(
                        "[FILE] {:?}: {}",
                        event.kind,
                        path.strip_prefix(&event.watch_root)
                            .unwrap_or(path)
                            .display()
                    );
                }
            }
        }

        // Check USB events
        if let Some(event) = usb_watcher.try_next_event() {
            if json {
                let watch_event = zero_watcher::WatchEvent::Usb(event);
                println!("{}", watch_event.to_json()?);
            } else {
                println!(
                    "[USB] {:?}: {}{}",
                    event.kind,
                    event.mount_point.display(),
                    event
                        .volume_name
                        .as_ref()
                        .map(|n| format!(" ({})", n))
                        .unwrap_or_default()
                );
            }
        }

        // Brief sleep to avoid busy-waiting
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn measure_latency(path: PathBuf, iterations: usize) -> Result<()> {
    use std::fs;

    println!("Measuring file watcher latency\n");
    println!("Watch path: {}", path.display());
    println!("Iterations: {}\n", iterations);

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
            if let Some(event) = watcher.next_event_timeout(Duration::from_millis(10)) {
                if event
                    .paths
                    .iter()
                    .any(|p| p.ends_with(".zero_latency_test"))
                {
                    let latency = start.elapsed();
                    latencies.push(latency);
                    println!(
                        "  Iteration {}: {:>6.2}ms ({:?})",
                        i + 1,
                        latency.as_secs_f64() * 1000.0,
                        event.kind
                    );
                    detected = true;
                    break;
                }
            }
        }

        if !detected {
            println!("  Iteration {}: TIMEOUT (>5s)", i + 1);
        }

        // Brief pause between iterations
        std::thread::sleep(Duration::from_millis(50));
    }

    // Clean up
    let _ = fs::remove_file(&test_file);

    // Print statistics
    if !latencies.is_empty() {
        println!("\nResults:");

        let min = latencies.iter().min().unwrap();
        let max = latencies.iter().max().unwrap();
        let sum: Duration = latencies.iter().sum();
        let avg = sum / latencies.len() as u32;

        let mut sorted = latencies.clone();
        sorted.sort();
        let median = sorted[sorted.len() / 2];
        let p95 = sorted[(sorted.len() as f64 * 0.95) as usize];

        println!("  Min:    {:>6.2}ms", min.as_secs_f64() * 1000.0);
        println!("  Max:    {:>6.2}ms", max.as_secs_f64() * 1000.0);
        println!("  Avg:    {:>6.2}ms", avg.as_secs_f64() * 1000.0);
        println!("  Median: {:>6.2}ms", median.as_secs_f64() * 1000.0);
        println!("  P95:    {:>6.2}ms", p95.as_secs_f64() * 1000.0);
        println!("\n  Success rate: {}/{}", latencies.len(), iterations);
    } else {
        println!("\nNo events detected!");
    }

    Ok(())
}
