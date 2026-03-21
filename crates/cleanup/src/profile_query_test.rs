//! Cleanup query benchmark tests
//!
//! These tests measure the performance of cleanup queries against a real index.
//! Run with: cargo test --release bench_ -- --nocapture --ignored

use std::time::{Duration, Instant};

use crate::{ProfileCleanupQuery, execute_full_cleanup_scan, execute_group_cleanup};
use profiles::{CleanupGroup, load_cleanup};
use search::{IndexManager, SearchIndex};

/// Load the IndexManager for benchmarks
fn load_index_manager() -> Option<IndexManager> {
    match IndexManager::load() {
        Ok(manager) if manager.total_file_count() > 0 => {
            eprintln!(
                "Loaded index: {} files, {} dirs",
                manager.total_file_count(),
                manager.total_dir_count()
            );
            Some(manager)
        }
        Ok(_) => {
            eprintln!("No index found. Run `zero search --index ~` first.");
            None
        }
        Err(e) => {
            eprintln!("Failed to load index: {}", e);
            None
        }
    }
}

/// Load the SearchIndex directly for benchmarks (for search comparison tests)
fn load_search_index() -> Option<SearchIndex> {
    let manager = load_index_manager()?;
    manager.indexes().next().cloned()
}

/// Benchmark result for a single operation
#[derive(Debug)]
struct BenchResult {
    name: String,
    iterations: u32,
    total_duration: Duration,
    items_found: usize,
    bytes_found: u64,
}

impl BenchResult {
    fn avg_duration(&self) -> Duration {
        self.total_duration / self.iterations
    }

    fn print(&self) {
        let avg_ms = self.avg_duration().as_secs_f64() * 1000.0;
        let avg_us = self.avg_duration().as_micros();

        if avg_ms >= 1.0 {
            println!(
                "  {:40} {:>10.2} ms  ({} items, {} bytes)",
                self.name, avg_ms, self.items_found, self.bytes_found
            );
        } else {
            println!(
                "  {:40} {:>10} us  ({} items, {} bytes)",
                self.name, avg_us, self.items_found, self.bytes_found
            );
        }
    }
}

/// Run a benchmark with warmup
fn bench<F>(name: &str, iterations: u32, warmup: u32, mut f: F) -> BenchResult
where
    F: FnMut() -> (usize, u64),
{
    // Warmup runs
    for _ in 0..warmup {
        let _ = f();
    }

    // Timed runs
    let start = Instant::now();
    let mut last_result = (0, 0);
    for _ in 0..iterations {
        last_result = f();
    }
    let total_duration = start.elapsed();

    BenchResult {
        name: name.to_string(),
        iterations,
        total_duration,
        items_found: last_result.0,
        bytes_found: last_result.1,
    }
}

/// Simple timing helper that doesn't care about output stats
fn bench_simple<F, R>(name: &str, iterations: u32, warmup: u32, mut f: F) -> BenchResult
where
    F: FnMut() -> R,
{
    // Warmup runs
    for _ in 0..warmup {
        let _ = f();
    }

    // Timed runs
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = f();
    }
    let total_duration = start.elapsed();

    BenchResult {
        name: name.to_string(),
        iterations,
        total_duration,
        items_found: 0,
        bytes_found: 0,
    }
}

// =============================================================================
// Benchmark Tests (run with --ignored flag)
// =============================================================================

/// Benchmark full cleanup scan (all categories)
#[test]
#[ignore]
fn bench_full_cleanup_scan() {
    let manager = match load_index_manager() {
        Some(m) => m,
        None => {
            println!("Skipping benchmark - no index available");
            return;
        }
    };

    println!("\n{}", "=".repeat(60));
    println!("CLEANUP BENCHMARK: Full Scan (all categories)");
    println!("{}\n", "=".repeat(60));

    // Warmup + benchmark
    let result = bench(
        "execute_full_cleanup_scan",
        5,
        2,
        || match execute_full_cleanup_scan(&manager) {
            Ok(summary) => (summary.total_count, summary.total_bytes),
            Err(_) => (0, 0),
        },
    );

    result.print();
    println!();
}

/// Benchmark individual category queries
#[test]
#[ignore]
fn bench_category_queries() {
    let manager = match load_index_manager() {
        Some(m) => m,
        None => {
            println!("Skipping benchmark - no index available");
            return;
        }
    };

    let profile = load_cleanup().expect("should load profile");

    println!("\n{}", "=".repeat(60));
    println!("CLEANUP BENCHMARK: Individual Categories");
    println!("{}\n", "=".repeat(60));

    // Test specific categories that exercise different query patterns
    let categories_to_test = [
        // Extension patterns (**/*.ext)
        "log_files",
        "disk_images",
        // Folder patterns (**/folder)
        "node_modules",
        "rust_target",
        "python_cache",
        // Fixed path patterns (~/path)
        #[cfg(target_os = "macos")]
        "system_caches",
        #[cfg(target_os = "macos")]
        "xcode_derived",
        // Filename patterns (**/.file)
        "ds_store",
    ];

    let mut results = Vec::new();

    for cat_id in categories_to_test {
        if let Some(category) = profile.get(cat_id) {
            let pattern_type = if category.patterns.iter().any(|p| p.starts_with("**/*.")) {
                "extension"
            } else if category.patterns.iter().any(|p| p.starts_with("**/")) {
                "folder"
            } else if category.patterns.iter().any(|p| p.starts_with("~/")) {
                "fixed_path"
            } else {
                "other"
            };

            let result = bench(&format!("{} ({})", cat_id, pattern_type), 10, 2, || {
                let r = ProfileCleanupQuery::from_category(category).execute(&manager);
                (r.count, r.total_bytes)
            });
            results.push(result);
        }
    }

    println!(
        "  {:40} {:>10}     {:>10}",
        "Category", "Avg Time", "Results"
    );
    println!(
        "  {:40} {:>10}     {:>10}",
        "-".repeat(40),
        "-".repeat(10),
        "-".repeat(10)
    );

    for result in &results {
        result.print();
    }

    // Summary statistics
    let total_time: Duration = results.iter().map(|r| r.avg_duration()).sum();
    let total_items: usize = results.iter().map(|r| r.items_found).sum();

    println!();
    println!(
        "  {:40} {:>10.2} ms  ({} items total)",
        "TOTAL",
        total_time.as_secs_f64() * 1000.0,
        total_items
    );
    println!();
}

/// Benchmark group queries
#[test]
#[ignore]
fn bench_group_queries() {
    let manager = match load_index_manager() {
        Some(m) => m,
        None => {
            println!("Skipping benchmark - no index available");
            return;
        }
    };

    println!("\n{}", "=".repeat(60));
    println!("CLEANUP BENCHMARK: Groups");
    println!("{}\n", "=".repeat(60));

    let groups = [
        CleanupGroup::Developer,
        CleanupGroup::System,
        CleanupGroup::Media,
        CleanupGroup::Documents,
    ];

    let mut results = Vec::new();

    for group in groups {
        let result = bench(
            &format!("{:?}", group),
            5,
            2,
            || match execute_group_cleanup(&manager, group) {
                Ok(summary) => (summary.total_count, summary.total_bytes),
                Err(_) => (0, 0),
            },
        );
        results.push(result);
    }

    println!("  {:40} {:>10}     {:>10}", "Group", "Avg Time", "Results");
    println!(
        "  {:40} {:>10}     {:>10}",
        "-".repeat(40),
        "-".repeat(10),
        "-".repeat(10)
    );

    for result in &results {
        result.print();
    }
    println!();
}

/// Benchmark pattern matching overhead (no index query)
#[test]
#[ignore]
fn bench_pattern_overhead() {
    println!("\n{}", "=".repeat(60));
    println!("CLEANUP BENCHMARK: Pattern Matching Overhead");
    println!("{}\n", "=".repeat(60));

    let profile = load_cleanup().expect("should load profile");

    // Count patterns
    let total_patterns: usize = profile.all_categories().map(|c| c.patterns.len()).sum();
    let total_categories = profile.all_categories().count();

    println!(
        "  Profile: {} categories, {} patterns\n",
        total_categories, total_patterns
    );

    // Benchmark query construction
    let category = profile
        .get("node_modules")
        .expect("should have node_modules");

    let start = Instant::now();
    let iterations = 10000u32;
    for _ in 0..iterations {
        let _ = ProfileCleanupQuery::from_category(category)
            .with_limit(100)
            .with_min_size(1024);
    }
    let construction_time = start.elapsed();
    let construction_ns = construction_time.as_nanos() / iterations as u128;
    println!("  Query construction: {} ns/query", construction_ns);

    // Benchmark pattern type detection
    let start = Instant::now();
    let iterations = 100_000u32;
    for _ in 0..iterations {
        for cat in profile.all_categories() {
            for pattern in &cat.patterns {
                let _ = pattern.starts_with("~/");
                let _ = pattern.starts_with("**/");
                let _ = pattern.starts_with("**/*.");
            }
        }
    }
    let pattern_time = start.elapsed();
    let per_pattern_ns = pattern_time.as_nanos() / (iterations as u128 * total_patterns as u128);

    println!("  Pattern type detection: {} ns/pattern", per_pattern_ns);
    println!();
}

/// Detailed breakdown of query execution time
#[test]
#[ignore]
fn bench_query_breakdown() {
    let manager = match load_index_manager() {
        Some(m) => m,
        None => {
            println!("Skipping benchmark - no index available");
            return;
        }
    };

    println!("\n{}", "=".repeat(60));
    println!("CLEANUP BENCHMARK: Query Time Breakdown");
    println!("{}\n", "=".repeat(60));

    let profile = load_cleanup().expect("should load profile");

    // Test different pattern types
    let test_cases: &[(&str, &str)] = &[
        ("log_files", "Extension: **/*.log"),
        ("node_modules", "Folder: **/node_modules"),
        ("ds_store", "Filename: **/.DS_Store"),
        #[cfg(target_os = "macos")]
        ("system_caches", "Fixed path: ~/Library/Caches"),
    ];

    println!(
        "  {:20} {:30} {:>12} {:>12}",
        "Category", "Pattern Type", "Avg Time", "Items"
    );
    println!(
        "  {:20} {:30} {:>12} {:>12}",
        "-".repeat(20),
        "-".repeat(30),
        "-".repeat(12),
        "-".repeat(12)
    );

    for (cat_id, description) in test_cases {
        if let Some(category) = profile.get(cat_id) {
            let result = bench(cat_id, 20, 5, || {
                let r = ProfileCleanupQuery::from_category(category).execute(&manager);
                (r.count, r.total_bytes)
            });

            let avg_ms = result.avg_duration().as_secs_f64() * 1000.0;
            println!(
                "  {:20} {:30} {:>10.2} ms {:>12}",
                cat_id, description, avg_ms, result.items_found
            );
        }
    }
    println!();

    // Now show what percentage is search vs post-filter
    println!("  NOTE: Current implementation uses manager.search() which is O(n) text search.");
    println!("        Optimization will use bitmap lookups for O(1) extension/folder queries.");
    println!();
}

/// Compare search performance for different query types
#[test]
#[ignore]
fn bench_search_comparison() {
    let index = match load_search_index() {
        Some(i) => i,
        None => {
            println!("Skipping benchmark - no index available");
            return;
        }
    };

    println!("\n{}", "=".repeat(60));
    println!("CLEANUP BENCHMARK: Search Method Comparison");
    println!("{}\n", "=".repeat(60));

    let iterations = 20u32;
    let warmup = 5u32;

    // 1. Text search (current approach)
    let text_result = bench_simple("Text search: '.log'", iterations, warmup, || {
        index.search(".log", 1000)
    });

    // 2. Type-filtered search
    let type_result = bench_simple("Type search: 'code'", iterations, warmup, || {
        index.search_by_type("code", 1000)
    });

    // 3. Combined search
    let combined_result = bench_simple("Combined: 'test' + type=code", iterations, warmup, || {
        index.search_with_type("test", "code", 100)
    });

    println!("  {:40} {:>12}", "Method", "Avg Time");
    println!("  {:40} {:>12}", "-".repeat(40), "-".repeat(12));

    for result in [&text_result, &type_result, &combined_result] {
        let avg_ms = result.avg_duration().as_secs_f64() * 1000.0;
        println!("  {:40} {:>10.2} ms", result.name, avg_ms);
    }

    println!();
    println!("  Type search uses bitmap lookups (fast).");
    println!("  Text search scans name index (slower for common terms).");
    println!("  Cleanup optimization: use type index for extensions.");
    println!();
}

// =============================================================================
// Quick validation tests (not benchmarks)
// =============================================================================

#[test]
fn test_cleanup_profile_loads() {
    let profile = load_cleanup();
    assert!(profile.is_ok(), "Cleanup profile should load");

    let profile = profile.unwrap();
    let count = profile.all_categories().count();
    assert!(count > 0, "Should have categories");
    println!("Loaded {} cleanup categories", count);
}

#[test]
fn test_cleanup_groups_defined() {
    let profile = load_cleanup().expect("should load profile");

    for group in [
        CleanupGroup::Developer,
        CleanupGroup::System,
        CleanupGroup::Media,
        CleanupGroup::Documents,
    ] {
        let categories = profile.categories_by_group(group);
        println!("{:?}: {} categories", group, categories.len());
    }
}
