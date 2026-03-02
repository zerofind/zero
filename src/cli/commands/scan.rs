//! Scan command handler

use std::path::Path;
use std::time::{Duration, Instant};
use zero::cmd_success;
use zero::output::*;
use zero::scanner::{ScanOptions, scan, scan_collect};

pub fn cmd_scan(
    out: &Outputter,
    path: &Path,
    max_depth: Option<usize>,
    follow_symlinks: bool,
    skip_hidden: bool,
) -> anyhow::Result<()> {
    let start = Instant::now();

    out.header(&format!("Scanning {}", path.display()));

    let options = ScanOptions {
        max_depth: max_depth.unwrap_or(usize::MAX),
        follow_symlinks,
        skip_hidden,
        ..Default::default()
    };

    let entries = scan_collect(path, options)?;
    let duration = start.elapsed();
    let duration_ms = duration.as_millis() as u64;

    let total_bytes: u64 = entries.iter().map(|e| e.size).sum();
    let file_count = entries.len();

    // Count directories (unique parent paths)
    let dir_count = entries
        .iter()
        .filter_map(|e| e.path.parent())
        .collect::<std::collections::HashSet<_>>()
        .len();

    let secs = duration.as_secs_f64();
    let files_per_second = if secs > 0.0 {
        Some(file_count as f64 / secs)
    } else {
        None
    };
    let bytes_per_second = if secs > 0.0 {
        Some(total_bytes as f64 / secs)
    } else {
        None
    };

    let data = ScanData {
        path: path.to_path_buf(),
        total_files: file_count,
        total_dirs: dir_count,
        total_bytes,
        files_per_second,
        bytes_per_second,
    };

    cmd_success!(out, "scan", duration_ms, data, {
        out.success(&format!(
            "Found {} files in {} directories",
            format_count(file_count),
            format_count(dir_count)
        ));
        out.newline();
        out.kv("Total size", format_bytes(total_bytes));
        out.kv("Duration", format_duration(duration));
        if let Some(fps) = files_per_second {
            out.kv("Speed", format!("{:.0} files/sec", fps));
        }
        if let Some(bps) = bytes_per_second {
            out.kv("Throughput", format!("{}/s", format_bytes(bps as u64)));
        }

        // Show some sample entries
        if file_count > 0 {
            out.newline();
            out.info("Sample files:");
            for entry in entries.iter().take(10) {
                out.indented(&format!(
                    "{} ({})",
                    entry.path.display(),
                    format_bytes(entry.size)
                ));
            }
            if file_count > 10 {
                out.indented(&format!("... and {} more", file_count - 10));
            }
        }
    });

    Ok(())
}

pub fn cmd_benchmark(
    out: &Outputter,
    path: &Path,
    max_depth: Option<usize>,
    follow_symlinks: bool,
    iterations: usize,
    skip_hidden: bool,
) -> anyhow::Result<()> {
    let iterations = iterations.max(1);

    out.header(&format!(
        "Benchmarking scanner on {} ({} iterations)",
        path.display(),
        iterations
    ));

    // Warmup run
    let warmup_start = Instant::now();
    let options = ScanOptions {
        max_depth: max_depth.unwrap_or(usize::MAX),
        follow_symlinks,
        skip_hidden,
        ..Default::default()
    };
    let entries = scan_collect(path, options.clone())?;
    let file_count = entries.len();
    let total_size: u64 = entries.iter().map(|e| e.size).sum();
    let warmup_elapsed = warmup_start.elapsed();
    out.info(&format!(
        "Warmup: {} ({} files, {})",
        format_duration(warmup_elapsed),
        file_count,
        format_bytes(total_size)
    ));
    out.newline();

    // Benchmark runs
    let mut times: Vec<Duration> = Vec::with_capacity(iterations);

    for i in 0..iterations {
        let start = Instant::now();

        let count: usize = scan(path, options.clone())?.filter(|r| r.is_ok()).count();

        let elapsed = start.elapsed();
        times.push(elapsed);

        if iterations <= 10 || (i + 1) % (iterations / 10).max(1) == 0 {
            out.indented(&format!(
                "Run {:>3}: {} ({} files)",
                i + 1,
                format_duration(elapsed),
                count
            ));
        }
    }

    // Calculate statistics
    times.sort();
    let min = times.first().unwrap();
    let max = times.last().unwrap();
    let median = times[times.len() / 2];
    let mean: Duration = times.iter().sum::<Duration>() / times.len() as u32;

    let p95_idx = (times.len() as f64 * 0.95) as usize;
    let p95 = times[p95_idx.min(times.len() - 1)];

    out.newline();
    out.info(&format!("Results ({} iterations):", iterations));
    out.kv("Files", file_count);
    out.kv("Size", format_bytes(total_size));
    out.newline();
    out.kv("Min", format_duration(*min));
    out.kv("Max", format_duration(*max));
    out.kv("Mean", format_duration(mean));
    out.kv("Median", format_duration(median));
    out.kv("P95", format_duration(p95));
    out.newline();

    // Throughput
    let files_per_sec = file_count as f64 / median.as_secs_f64();
    out.kv("Throughput", format!("{:.0} files/sec", files_per_sec));

    Ok(())
}
