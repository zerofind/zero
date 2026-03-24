//! Tests for profile loading
//!
//! Tests profile parsing, merging, and caching behavior.

use super::loader::*;
use super::schema::{AggregateMode, CleanupGroup, CleanupProfile, Os};

#[test]
fn test_parse_duration_days() {
    assert_eq!(parse_duration("30d"), Ok(30 * 24 * 60 * 60));
    assert_eq!(parse_duration("1d"), Ok(24 * 60 * 60));
    assert_eq!(parse_duration("365d"), Ok(365 * 24 * 60 * 60));
}

#[test]
fn test_parse_duration_hours() {
    assert_eq!(parse_duration("24h"), Ok(24 * 60 * 60));
    assert_eq!(parse_duration("1h"), Ok(60 * 60));
    assert_eq!(parse_duration("48h"), Ok(48 * 60 * 60));
}

#[test]
fn test_parse_duration_minutes() {
    assert_eq!(parse_duration("60m"), Ok(60 * 60));
    assert_eq!(parse_duration("1m"), Ok(60));
    assert_eq!(parse_duration("120m"), Ok(120 * 60));
}

#[test]
fn test_parse_duration_seconds() {
    assert_eq!(parse_duration("30s"), Ok(30));
    assert_eq!(parse_duration("1s"), Ok(1));
    assert_eq!(parse_duration("3600s"), Ok(3600));
}

#[test]
fn test_parse_duration_invalid() {
    assert!(parse_duration("invalid").is_err());
    assert!(parse_duration("").is_err());
    assert!(parse_duration("30").is_err());
    assert!(parse_duration("d").is_err());
    assert!(parse_duration("30x").is_err());
    assert!(parse_duration("-30d").is_err());
}

#[test]
fn test_parse_size_megabytes() {
    assert_eq!(parse_size("100MB"), Ok(100 * 1024 * 1024));
    assert_eq!(parse_size("1MB"), Ok(1024 * 1024));
    assert_eq!(parse_size("100M"), Ok(100 * 1024 * 1024));
}

#[test]
fn test_parse_size_gigabytes() {
    assert_eq!(parse_size("1GB"), Ok(1024 * 1024 * 1024));
    assert_eq!(parse_size("10GB"), Ok(10 * 1024 * 1024 * 1024));
    assert_eq!(parse_size("1G"), Ok(1024 * 1024 * 1024));
}

#[test]
fn test_parse_size_kilobytes() {
    assert_eq!(parse_size("500KB"), Ok(500 * 1024));
    assert_eq!(parse_size("1KB"), Ok(1024));
    assert_eq!(parse_size("500K"), Ok(500 * 1024));
}

#[test]
fn test_parse_size_bytes() {
    assert_eq!(parse_size("1024B"), Ok(1024));
    assert_eq!(parse_size("1B"), Ok(1));
}

#[test]
fn test_parse_size_case_insensitive() {
    assert_eq!(parse_size("100mb"), Ok(100 * 1024 * 1024));
    assert_eq!(parse_size("1gb"), Ok(1024 * 1024 * 1024));
    assert_eq!(parse_size("500kb"), Ok(500 * 1024));
}

#[test]
fn test_parse_size_invalid() {
    assert!(parse_size("invalid").is_err());
    assert!(parse_size("").is_err());
    assert!(parse_size("MB").is_err());
    assert!(parse_size("100XB").is_err());
}

#[test]
fn test_load_file_types_contains_expected_categories() {
    let profile = load_file_types().expect("should load file types");

    // Check main categories exist
    assert!(profile.types.contains_key("images"));
    assert!(profile.types.contains_key("videos"));
    assert!(profile.types.contains_key("audio"));
    assert!(profile.types.contains_key("documents"));
    assert!(profile.types.contains_key("code"));
    assert!(profile.types.contains_key("archives"));
    assert!(profile.types.contains_key("config"));
}

#[test]
fn test_load_file_types_images_extensions() {
    let profile = load_file_types().expect("should load file types");
    let images = &profile.types["images"].extensions;

    assert!(images.contains(&"png".to_string()));
    assert!(images.contains(&"jpg".to_string()));
    assert!(images.contains(&"jpeg".to_string()));
    assert!(images.contains(&"gif".to_string()));
    assert!(images.contains(&"webp".to_string()));
}

#[test]
fn test_load_file_types_code_extensions() {
    let profile = load_file_types().expect("should load file types");
    let code = &profile.types["code"].extensions;

    assert!(code.contains(&"rs".to_string()));
    assert!(code.contains(&"py".to_string()));
    assert!(code.contains(&"js".to_string()));
    assert!(code.contains(&"ts".to_string()));
    assert!(code.contains(&"go".to_string()));
}

#[test]
fn test_load_cleanup_not_empty() {
    let profile = load_cleanup().expect("should load cleanup profile");
    assert!(!profile.categories.is_empty());
}

#[test]
fn test_load_cleanup_node_modules_exists() {
    let profile = load_cleanup().expect("should load cleanup profile");

    let node_modules = profile.get("node_modules");
    assert!(node_modules.is_some());

    let cat = node_modules.unwrap();
    assert_eq!(cat.name, "node_modules");
    assert_eq!(cat.group, CleanupGroup::Developer);
    assert!(!cat.patterns.is_empty());
}

#[test]
fn test_load_cleanup_rust_target_exists() {
    let profile = load_cleanup().expect("should load cleanup profile");

    let rust_target = profile.get("rust_target");
    assert!(rust_target.is_some());

    let cat = rust_target.unwrap();
    assert_eq!(cat.group, CleanupGroup::Developer);
}

#[test]
#[cfg(target_os = "macos")]
fn test_load_cleanup_macos_xcode_exists() {
    let profile = load_cleanup().expect("should load cleanup profile");

    assert!(profile.get("xcode_derived_data").is_some());
    assert!(profile.get("xcode_archives").is_some());
    assert!(profile.get("xcode_simulators").is_some());
}

#[test]
#[cfg(target_os = "macos")]
fn test_load_cleanup_macos_ds_store_exists() {
    let profile = load_cleanup().expect("should load cleanup profile");

    let ds_store = profile.get("ds_store");
    assert!(ds_store.is_some());

    let cat = ds_store.unwrap();
    assert_eq!(cat.group, CleanupGroup::System);
    assert!(cat.patterns.iter().any(|p| p.contains(".DS_Store")));
}

#[test]
#[cfg(target_os = "macos")]
fn test_load_cleanup_macos_homebrew_exists() {
    let profile = load_cleanup().expect("should load cleanup profile");

    let homebrew = profile.get("homebrew_cache");
    assert!(homebrew.is_some());

    let cat = homebrew.unwrap();
    assert_eq!(cat.group, CleanupGroup::System);
}

#[test]
#[cfg(target_os = "macos")]
fn test_load_cleanup_macos_ios_backups_exists() {
    let profile = load_cleanup().expect("should load cleanup profile");

    let ios_backups = profile.get("ios_backups");
    assert!(ios_backups.is_some());

    let cat = ios_backups.unwrap();
    assert_eq!(cat.group, CleanupGroup::Ios);
    assert!(cat.warning.is_some());
}

#[test]
fn test_categories_by_group_developer() {
    let profile = load_cleanup().expect("should load cleanup profile");
    let developer_cats = profile.categories_by_group(CleanupGroup::Developer);

    assert!(!developer_cats.is_empty());

    for cat in developer_cats {
        assert_eq!(cat.group, CleanupGroup::Developer);
    }
}

#[test]
fn test_categories_by_group_system() {
    let profile = load_cleanup().expect("should load cleanup profile");
    let system_cats = profile.categories_by_group(CleanupGroup::System);

    // System categories exist on all platforms (at least from shared.toml)
    // But most are in macos.toml, so this might be empty on other OSes
    for cat in system_cats {
        assert_eq!(cat.group, CleanupGroup::System);
    }
}

#[test]
fn test_active_groups_not_empty() {
    let profile = load_cleanup().expect("should load cleanup profile");
    let groups = profile.active_groups();

    assert!(!groups.is_empty());
    // Developer should always be present (from shared.toml)
    assert!(groups.contains(&CleanupGroup::Developer));
}

#[test]
fn test_cleanup_category_min_age_parsed() {
    let profile = load_cleanup().expect("should load cleanup profile");

    // Find a category with min_age (old_downloads on macOS, log_files in shared)
    let categories_with_age: Vec<_> = profile
        .all_categories()
        .filter(|c| c.min_age_secs.is_some())
        .collect();

    // At least one category should have min_age
    assert!(
        !categories_with_age.is_empty(),
        "expected at least one category with min_age"
    );

    // Verify the age is properly parsed (should be > 0)
    for cat in categories_with_age {
        assert!(cat.min_age_secs.unwrap() > 0, "min_age should be positive");
    }
}

#[test]
fn test_cleanup_group_display_names() {
    assert_eq!(CleanupGroup::Developer.display_name(), "Developer");
    assert_eq!(CleanupGroup::System.display_name(), "System");
    assert_eq!(CleanupGroup::Documents.display_name(), "Documents");
    assert_eq!(CleanupGroup::Media.display_name(), "Media");
    assert_eq!(CleanupGroup::Mail.display_name(), "Mail");
    assert_eq!(CleanupGroup::Messages.display_name(), "Messages");
    assert_eq!(CleanupGroup::Ios.display_name(), "iOS Files");
    assert_eq!(CleanupGroup::Trash.display_name(), "Trash");
}

#[test]
fn test_cleanup_group_icons() {
    // Just verify icons are non-empty strings
    for group in CleanupGroup::all() {
        assert!(!group.icon().is_empty());
    }
}

#[test]
fn test_cleanup_group_all_returns_all_variants() {
    let all = CleanupGroup::all();

    assert!(all.contains(&CleanupGroup::Developer));
    assert!(all.contains(&CleanupGroup::System));
    assert!(all.contains(&CleanupGroup::Documents));
    assert!(all.contains(&CleanupGroup::Media));
    assert!(all.contains(&CleanupGroup::Mail));
    assert!(all.contains(&CleanupGroup::Messages));
    assert!(all.contains(&CleanupGroup::Ios));
    assert!(all.contains(&CleanupGroup::Trash));
}

#[test]
fn test_user_profiles_dir() {
    // This should return Some on all platforms with a home directory
    let dir = user_profiles_dir();

    if let Some(path) = dir {
        assert!(path.ends_with(".zero/profiles"));
    }
}

#[test]
fn test_os_as_str() {
    assert_eq!(Os::MacOs.as_str(), "macos");
    assert_eq!(Os::Linux.as_str(), "linux");
    assert_eq!(Os::Windows.as_str(), "windows");
}

// =============================================================================
// Performance Benchmarks
// =============================================================================

#[test]
fn bench_profile_load_time() {
    use std::time::Instant;

    // Warm up the cache
    let _ = load_file_types();
    let _ = load_cleanup();

    // Measure file types loading (should be instant from cache)
    let start = Instant::now();
    for _ in 0..1000 {
        let _ = load_file_types();
    }
    let file_types_time = start.elapsed();

    // Measure cleanup profile loading (should be instant from cache)
    let start = Instant::now();
    for _ in 0..1000 {
        let _ = load_cleanup();
    }
    let cleanup_time = start.elapsed();

    println!("\n=== Profile Loading Performance ===");
    println!("1000x load_file_types(): {file_types_time:?}");
    println!("1000x load_cleanup(): {cleanup_time:?}");
    println!(
        "Average per call: {:?} (file_types), {:?} (cleanup)",
        file_types_time / 1000,
        cleanup_time / 1000
    );

    // Should be sub-microsecond since it's cached
    assert!(
        file_types_time.as_micros() < 1000,
        "file_types loading too slow: {file_types_time:?}"
    );
    assert!(
        cleanup_time.as_micros() < 1000,
        "cleanup loading too slow: {cleanup_time:?}"
    );
}

#[test]
fn bench_category_iteration() {
    use std::time::Instant;

    let profile = load_cleanup().expect("should load cleanup profile");

    // Measure category iteration
    let start = Instant::now();
    let mut total_patterns = 0;
    for _ in 0..10000 {
        for cat in profile.all_categories() {
            total_patterns += cat.patterns.len();
        }
    }
    let iter_time = start.elapsed();

    let category_count = profile.categories.len();
    println!("\n=== Category Iteration Performance ===");
    println!("Categories: {category_count}");
    println!("Total patterns (per iteration): {}", total_patterns / 10000);
    println!("10000x full iteration: {iter_time:?}");
    println!("Average per iteration: {:?}", iter_time / 10000);

    // Should be very fast - microseconds
    assert!(
        iter_time.as_millis() < 100,
        "category iteration too slow: {iter_time:?}"
    );
}

#[test]
fn bench_group_lookup() {
    use std::time::Instant;

    let profile = load_cleanup().expect("should load cleanup profile");

    // Measure group-based lookup
    let start = Instant::now();
    for _ in 0..10000 {
        let _ = profile.categories_by_group(CleanupGroup::Developer);
        let _ = profile.categories_by_group(CleanupGroup::System);
        let _ = profile.categories_by_group(CleanupGroup::Documents);
    }
    let lookup_time = start.elapsed();

    println!("\n=== Group Lookup Performance ===");
    println!("30000 group lookups (3 groups x 10000): {lookup_time:?}");
    println!("Average per lookup: {:?}", lookup_time / 30000);

    // Should be sub-millisecond total
    assert!(
        lookup_time.as_millis() < 500,
        "group lookup too slow: {lookup_time:?}"
    );
}

#[test]
fn bench_category_by_id_lookup() {
    use std::time::Instant;

    let profile = load_cleanup().expect("should load cleanup profile");

    // Get a list of category IDs to look up
    let ids: Vec<_> = profile.categories.keys().cloned().collect();

    // Measure ID-based lookup
    let start = Instant::now();
    for _ in 0..10000 {
        for id in &ids {
            let _ = profile.get(id);
        }
    }
    let lookup_time = start.elapsed();

    let total_lookups = 10000 * ids.len();
    println!("\n=== Category ID Lookup Performance ===");
    println!("Category IDs: {}", ids.len());
    println!("{total_lookups} lookups: {lookup_time:?}");
    println!(
        "Average per lookup: {:?}",
        lookup_time / total_lookups as u32
    );

    // HashMap lookup should be O(1), very fast
    assert!(
        lookup_time.as_millis() < 100,
        "ID lookup too slow: {lookup_time:?}"
    );
}

#[test]
fn test_user_profiles_dir_format() {
    // user_profiles_dir should return a path ending in zero/profiles
    if let Some(dir) = user_profiles_dir() {
        let path_str = dir.to_string_lossy();
        assert!(path_str.contains("zero"));
        assert!(path_str.ends_with("profiles"));
    }
}

#[test]
fn test_ensure_user_profiles_dir() {
    // This test just verifies the function doesn't panic
    // We don't actually create the directory in tests to avoid side effects
    let _ = ensure_user_profiles_dir();
}

#[test]
fn test_sibling_field_parses() {
    let toml_str = r#"
[rust_target]
name = "Rust Build Cache"
patterns = ["**/target"]
sibling = "Cargo.toml"
aggregate = "directory"
group = "developer"
"#;
    let profile: CleanupProfile = toml::from_str(toml_str).expect("should parse");
    let cat = &profile.categories["rust_target"];
    assert_eq!(cat.sibling.as_deref(), Some("Cargo.toml"));
    assert_eq!(cat.aggregate, Some(AggregateMode::Directory));
}

#[test]
fn test_aggregate_field_parses() {
    let toml_str = r#"
[node_modules]
name = "node_modules"
patterns = ["**/node_modules"]
aggregate = "directory"
group = "developer"
"#;
    let profile: CleanupProfile = toml::from_str(toml_str).expect("should parse");
    let cat = &profile.categories["node_modules"];
    assert_eq!(cat.aggregate, Some(AggregateMode::Directory));
}

#[test]
fn test_aggregate_file_mode() {
    let toml_str = r#"
[log_files]
name = "Log Files"
patterns = ["**/*.log"]
aggregate = "file"
group = "system"
"#;
    let profile: CleanupProfile = toml::from_str(toml_str).expect("should parse");
    let cat = &profile.categories["log_files"];
    assert_eq!(cat.aggregate, Some(AggregateMode::File));
}

#[test]
fn test_backward_compat_no_new_fields() {
    let toml_str = r#"
[old_category]
name = "Old Category"
patterns = ["**/*.tmp"]
group = "system"
"#;
    let profile: CleanupProfile =
        toml::from_str(toml_str).expect("should parse without new fields");
    let cat = &profile.categories["old_category"];
    assert_eq!(cat.sibling, None);
    assert_eq!(cat.aggregate, None);
}

#[test]
fn test_resolved_category_has_sibling_and_aggregate() {
    let profile = load_cleanup().expect("should load cleanup profile");

    let rust_target = profile
        .get("rust_target")
        .expect("rust_target should exist");
    assert_eq!(rust_target.sibling.as_deref(), Some("Cargo.toml"));
    assert_eq!(rust_target.aggregate, AggregateMode::Directory);

    let node_modules = profile
        .get("node_modules")
        .expect("node_modules should exist");
    assert_eq!(node_modules.sibling.as_deref(), Some("package.json"));
    assert_eq!(node_modules.aggregate, AggregateMode::Directory);
}

#[test]
fn test_new_developer_categories_exist() {
    let profile = load_cleanup().expect("should load cleanup profile");

    let expected = [
        "nextjs_cache",
        "python_venv",
        "tox_cache",
        "flutter_build",
        "zig_cache",
        "cmake_build",
        "elixir_build",
        "ruby_vendor",
        "cocoapods_proj",
        "parcel_cache",
        "turbo_cache",
    ];

    for id in &expected {
        let cat = profile.get(id);
        assert!(cat.is_some(), "category '{id}' should exist");
        let cat = cat.unwrap();
        assert_eq!(cat.group, CleanupGroup::Developer);
        assert_eq!(cat.aggregate, AggregateMode::Directory);
    }
}

#[test]
fn test_has_user_overrides() {
    // This just checks the function works - result depends on system state
    let _ = has_user_overrides();
}

#[test]
fn test_profile_loads_without_user_overrides() {
    // Even without user overrides, profiles should load successfully
    let file_types = load_file_types();
    assert!(file_types.is_ok());

    let cleanup = load_cleanup();
    assert!(cleanup.is_ok());
}
