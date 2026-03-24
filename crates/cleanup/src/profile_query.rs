//! Profile-based cleanup query execution
//!
//! This module provides cleanup queries based on profile configurations loaded from TOML files.
//! It replaces the hardcoded `CleanupCategory` approach with configurable profiles.
//!
//! ## Usage
//!
//! ```ignore
//! use cleanup::profile_query::{ProfileCleanupQuery, execute_cleanup_query};
//! use profiles::{load_cleanup, CleanupGroup};
//!
//! // Load the cleanup profile
//! let profile = load_cleanup()?;
//!
//! // Query a specific category
//! if let Some(category) = profile.get("node_modules") {
//!     let query = ProfileCleanupQuery::from_category(category);
//!     let result = query.execute(&index_manager);
//!     println!("Found {} items, {} bytes", result.count, result.total_bytes);
//! }
//!
//! // Query all categories in a group
//! let developer_results = execute_group_cleanup(&index_manager, CleanupGroup::Developer)?;
//! ```

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use profiles::{CleanupGroup, ResolvedCleanupCategory, load_cleanup};
use search::{FileNode, IndexManager};

/// A single item found by a profile-based cleanup query
#[derive(Debug, Clone)]
pub struct ProfileCleanupItem {
    /// The file node from the index
    pub node: FileNode,
    /// The category ID that matched
    pub category_id: Arc<str>,
    /// The category display name
    pub category_name: Arc<str>,
    /// The UI group this category belongs to
    pub group: CleanupGroup,
    /// Human-readable reason for flagging
    pub reason: Arc<str>,
}

impl ProfileCleanupItem {
    /// Create a new cleanup item with shared category strings
    fn with_shared(
        node: FileNode,
        category_id: Arc<str>,
        category_name: Arc<str>,
        group: CleanupGroup,
        reason: Arc<str>,
    ) -> Self {
        Self {
            node,
            category_id,
            category_name,
            group,
            reason,
        }
    }

    /// Get the file path
    pub fn path(&self) -> &str {
        &self.node.path
    }

    /// Get the file size in bytes
    pub fn size(&self) -> u64 {
        self.node.size
    }
}

/// Results from a profile-based cleanup query
#[derive(Debug, Clone)]
pub struct ProfileCleanupResult {
    /// The category ID queried
    pub category_id: Arc<str>,
    /// The category display name
    pub category_name: Arc<str>,
    /// The UI group
    pub group: CleanupGroup,
    /// Items found
    pub items: Vec<ProfileCleanupItem>,
    /// Total size in bytes
    pub total_bytes: u64,
    /// Total item count
    pub count: usize,
    /// Optional warning message
    pub warning: Option<String>,
}

impl ProfileCleanupResult {
    /// Create a new result
    pub fn new(
        cat_id: Arc<str>,
        cat_name: Arc<str>,
        group: CleanupGroup,
        warning: Option<String>,
        items: Vec<ProfileCleanupItem>,
    ) -> Self {
        let total_bytes = items.iter().map(ProfileCleanupItem::size).sum();
        let count = items.len();
        Self {
            category_id: cat_id,
            category_name: cat_name,
            group,
            items,
            total_bytes,
            count,
            warning,
        }
    }

    /// Create an empty result
    pub fn empty(category: &ResolvedCleanupCategory) -> Self {
        Self {
            category_id: Arc::from(category.id.as_str()),
            category_name: Arc::from(category.name.as_str()),
            group: category.group,
            items: Vec::new(),
            total_bytes: 0,
            count: 0,
            warning: category.warning.clone(),
        }
    }
}

/// Summary of cleanup results grouped by `CleanupGroup`
#[derive(Debug, Clone)]
pub struct ProfileCleanupSummary {
    /// Results grouped by `CleanupGroup`
    pub groups: Vec<GroupSummary>,
    /// Total bytes across all groups
    pub total_bytes: u64,
    /// Total items across all groups
    pub total_count: usize,
}

/// Summary for a single cleanup group
#[derive(Debug, Clone)]
pub struct GroupSummary {
    /// The group
    pub group: CleanupGroup,
    /// Results for categories in this group
    pub categories: Vec<ProfileCleanupResult>,
    /// Total bytes in this group
    pub total_bytes: u64,
    /// Total items in this group
    pub total_count: usize,
}

impl GroupSummary {
    /// Create a new group summary
    pub fn new(group: CleanupGroup, categories: Vec<ProfileCleanupResult>) -> Self {
        let total_bytes = categories.iter().map(|r| r.total_bytes).sum();
        let total_count = categories.iter().map(|r| r.count).sum();
        Self {
            group,
            categories,
            total_bytes,
            total_count,
        }
    }
}

/// Query builder for profile-based cleanup operations
#[derive(Debug, Clone)]
pub struct ProfileCleanupQuery<'a> {
    /// The category configuration
    category: &'a ResolvedCleanupCategory,
    /// Maximum results to return
    limit: usize,
    /// Override minimum size (bytes)
    min_size_override: Option<u64>,
    /// Override minimum age (seconds)
    min_age_override: Option<u64>,
}

#[allow(clippy::too_many_arguments)]
impl<'a> ProfileCleanupQuery<'a> {
    /// Create a query from a resolved category
    pub fn from_category(category: &'a ResolvedCleanupCategory) -> Self {
        Self {
            category,
            limit: usize::MAX,
            min_size_override: None,
            min_age_override: None,
        }
    }

    /// Set the maximum number of results
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Override the minimum file size
    pub fn with_min_size(mut self, bytes: u64) -> Self {
        self.min_size_override = Some(bytes);
        self
    }

    /// Override the minimum age in days
    pub fn with_min_age_days(mut self, days: u32) -> Self {
        self.min_age_override = Some(u64::from(days) * 24 * 60 * 60);
        self
    }

    /// Execute the query against an `IndexManager`
    pub fn execute(&self, manager: &IndexManager) -> ProfileCleanupResult {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        // Determine age cutoff
        let min_age_secs = self.min_age_override.or(self.category.min_age_secs);
        let cutoff_time = min_age_secs.map(|age| now.saturating_sub(age));

        // Determine size minimum
        let min_size = self.min_size_override.or(self.category.min_size_bytes);

        // Pre-create shared arcs for category strings (one atomic inc per item instead of full string clone)
        let cat_id: Arc<str> = Arc::from(self.category.id.as_str());
        let cat_name: Arc<str> = Arc::from(self.category.name.as_str());

        let mut items: Vec<ProfileCleanupItem> = Vec::new();

        // Execute different strategies based on pattern types
        if !self.category.patterns.is_empty() {
            self.execute_pattern_query(
                manager,
                &mut items,
                cutoff_time,
                min_size,
                &cat_id,
                &cat_name,
            );
        }

        // Filter out stale entries (files deleted since last index)
        items.retain(|item| std::path::Path::new(item.path()).exists());

        // Sort by size descending
        items.sort_by_key(|a| std::cmp::Reverse(a.size()));

        // Apply limit
        items.truncate(self.limit);

        ProfileCleanupResult::new(
            cat_id,
            cat_name,
            self.category.group,
            self.category.warning.clone(),
            items,
        )
    }

    /// Execute pattern-based queries
    fn execute_pattern_query(
        &self,
        manager: &IndexManager,
        items: &mut Vec<ProfileCleanupItem>,
        cutoff_time: Option<u64>,
        min_size: Option<u64>,
        cat_id: &Arc<str>,
        cat_name: &Arc<str>,
    ) {
        for pattern in &self.category.patterns {
            // Determine pattern type and execute appropriate query
            if let Some(path_pattern) = pattern.strip_prefix("~/") {
                // Fixed path relative to home
                self.query_fixed_path(
                    manager,
                    items,
                    path_pattern,
                    cutoff_time,
                    min_size,
                    cat_id,
                    cat_name,
                );
            } else if pattern.starts_with("**/") {
                // Recursive glob pattern
                let search_term = pattern.trim_start_matches("**/");
                self.query_recursive_pattern(
                    manager,
                    items,
                    search_term,
                    cutoff_time,
                    min_size,
                    cat_id,
                    cat_name,
                );
            } else {
                // Treat as search term
                self.query_search_term(
                    manager,
                    items,
                    pattern,
                    cutoff_time,
                    min_size,
                    cat_id,
                    cat_name,
                );
            }
        }
    }

    /// Query files under a fixed path
    fn query_fixed_path(
        &self,
        manager: &IndexManager,
        items: &mut Vec<ProfileCleanupItem>,
        path_pattern: &str,
        cutoff_time: Option<u64>,
        min_size: Option<u64>,
        cat_id: &Arc<str>,
        cat_name: &Arc<str>,
    ) {
        // Expand ~ to home directory
        let home = dirs::home_dir();
        let full_path = match home {
            Some(h) => h.join(path_pattern).to_string_lossy().to_string(),
            None => return,
        };

        // Extract the last component for searching, then filter by full path
        let search_term = path_pattern.rsplit('/').next().unwrap_or(path_pattern);

        // Search for files matching the last path component
        let results = manager.search(search_term, self.limit.saturating_mul(10));

        for result in results {
            // Filter to only files under the target path
            if result.node.path.starts_with(&full_path)
                && Self::matches_filters(&result.node, cutoff_time, min_size)
                && !self.is_excluded(&result.node.path)
            {
                let reason: Arc<str> = Arc::from(self.generate_reason(&result.node));
                items.push(ProfileCleanupItem::with_shared(
                    result.node,
                    Arc::clone(cat_id),
                    Arc::clone(cat_name),
                    self.category.group,
                    reason,
                ));

                if items.len() >= self.limit {
                    return;
                }
            }
        }
    }

    /// Query files matching a recursive pattern (e.g., **/`node_modules`)
    fn query_recursive_pattern(
        &self,
        manager: &IndexManager,
        items: &mut Vec<ProfileCleanupItem>,
        search_term: &str,
        cutoff_time: Option<u64>,
        min_size: Option<u64>,
        cat_id: &Arc<str>,
        cat_name: &Arc<str>,
    ) {
        // Handle different pattern types
        if search_term.starts_with('.') {
            // Filename pattern like .DS_Store
            self.query_filename_pattern(
                manager,
                items,
                search_term,
                cutoff_time,
                min_size,
                cat_id,
                cat_name,
            );
        } else if search_term.starts_with("*.") {
            // Extension pattern like *.dmg
            let ext = search_term.trim_start_matches("*.");
            self.query_extension_pattern(
                manager,
                items,
                ext,
                cutoff_time,
                min_size,
                cat_id,
                cat_name,
            );
        } else if search_term.ends_with("/*") {
            // Directory contents pattern
            let dir_name = search_term.trim_end_matches("/*");
            self.query_directory_contents(
                manager,
                items,
                dir_name,
                cutoff_time,
                min_size,
                cat_id,
                cat_name,
            );
        } else {
            // Folder name pattern like node_modules
            self.query_folder_pattern(
                manager,
                items,
                search_term,
                cutoff_time,
                min_size,
                cat_id,
                cat_name,
            );
        }
    }

    /// Query by filename pattern (e.g., .`DS_Store`)
    fn query_filename_pattern(
        &self,
        manager: &IndexManager,
        items: &mut Vec<ProfileCleanupItem>,
        filename: &str,
        cutoff_time: Option<u64>,
        min_size: Option<u64>,
        cat_id: &Arc<str>,
        cat_name: &Arc<str>,
    ) {
        // Search for the exact filename
        let results = manager.search(filename, self.limit);

        for result in results {
            if result.node.name() == filename
                && Self::matches_filters(&result.node, cutoff_time, min_size)
                && !self.is_excluded(&result.node.path)
            {
                let reason: Arc<str> = Arc::from(self.generate_reason(&result.node));
                items.push(ProfileCleanupItem::with_shared(
                    result.node,
                    Arc::clone(cat_id),
                    Arc::clone(cat_name),
                    self.category.group,
                    reason,
                ));

                if items.len() >= self.limit {
                    return;
                }
            }
        }
    }

    /// Query by extension pattern (e.g., *.dmg)
    ///
    /// Uses O(1) bitmap lookup via `TypeIndex` extension bitmaps.
    /// This is ~1000x faster than text search for extension queries.
    fn query_extension_pattern(
        &self,
        manager: &IndexManager,
        items: &mut Vec<ProfileCleanupItem>,
        ext: &str,
        cutoff_time: Option<u64>,
        min_size: Option<u64>,
        cat_id: &Arc<str>,
        cat_name: &Arc<str>,
    ) {
        // Use O(1) extension bitmap lookup instead of text search
        // This is the key optimization: bitmap lookup vs O(n) text scan
        let results = manager.search_by_extension(ext, self.limit.saturating_mul(10));

        for result in results {
            // No need to verify extension - bitmap already guarantees it matches
            if Self::matches_filters(&result.node, cutoff_time, min_size)
                && !self.is_excluded(&result.node.path)
            {
                let reason: Arc<str> = Arc::from(self.generate_reason(&result.node));
                items.push(ProfileCleanupItem::with_shared(
                    result.node,
                    Arc::clone(cat_id),
                    Arc::clone(cat_name),
                    self.category.group,
                    reason,
                ));

                if items.len() >= self.limit {
                    return;
                }
            }
        }
    }

    /// Query directory contents
    fn query_directory_contents(
        &self,
        manager: &IndexManager,
        items: &mut Vec<ProfileCleanupItem>,
        dir_name: &str,
        cutoff_time: Option<u64>,
        min_size: Option<u64>,
        cat_id: &Arc<str>,
        cat_name: &Arc<str>,
    ) {
        // Search for the directory name
        let results = manager.search(dir_name, self.limit.saturating_mul(10));

        for result in results {
            // Check if path contains the directory
            if result.node.path.contains(&format!("/{dir_name}/"))
                && Self::matches_filters(&result.node, cutoff_time, min_size)
                && !self.is_excluded(&result.node.path)
            {
                let reason: Arc<str> = Arc::from(self.generate_reason(&result.node));
                items.push(ProfileCleanupItem::with_shared(
                    result.node,
                    Arc::clone(cat_id),
                    Arc::clone(cat_name),
                    self.category.group,
                    reason,
                ));

                if items.len() >= self.limit {
                    return;
                }
            }
        }
    }

    /// Query by folder pattern (e.g., `node_modules`, target)
    ///
    /// Uses O(1) bitmap lookup via `TypeIndex` path component bitmaps.
    /// Falls back to text search if the component isn't in the selective index.
    fn query_folder_pattern(
        &self,
        manager: &IndexManager,
        items: &mut Vec<ProfileCleanupItem>,
        folder_name: &str,
        cutoff_time: Option<u64>,
        min_size: Option<u64>,
        cat_id: &Arc<str>,
        cat_name: &Arc<str>,
    ) {
        // Try O(1) path component bitmap lookup first
        let results = manager.search_by_path_component(folder_name, self.limit.saturating_mul(10));

        if results.is_empty() {
            // Fallback: component may not be in the selective index.
            // Use text search instead.
            self.query_search_term(
                manager,
                items,
                folder_name,
                cutoff_time,
                min_size,
                cat_id,
                cat_name,
            );
            return;
        }

        for result in results {
            if Self::matches_filters(&result.node, cutoff_time, min_size)
                && !self.is_excluded(&result.node.path)
            {
                let reason: Arc<str> = Arc::from(self.generate_reason(&result.node));
                items.push(ProfileCleanupItem::with_shared(
                    result.node,
                    Arc::clone(cat_id),
                    Arc::clone(cat_name),
                    self.category.group,
                    reason,
                ));

                if items.len() >= self.limit {
                    return;
                }
            }
        }
    }

    /// Query using general search term
    fn query_search_term(
        &self,
        manager: &IndexManager,
        items: &mut Vec<ProfileCleanupItem>,
        term: &str,
        cutoff_time: Option<u64>,
        min_size: Option<u64>,
        cat_id: &Arc<str>,
        cat_name: &Arc<str>,
    ) {
        let results = manager.search(term, self.limit.saturating_mul(10));

        for result in results {
            if Self::matches_filters(&result.node, cutoff_time, min_size)
                && !self.is_excluded(&result.node.path)
            {
                let reason: Arc<str> = Arc::from(self.generate_reason(&result.node));
                items.push(ProfileCleanupItem::with_shared(
                    result.node,
                    Arc::clone(cat_id),
                    Arc::clone(cat_name),
                    self.category.group,
                    reason,
                ));

                if items.len() >= self.limit {
                    return;
                }
            }
        }
    }

    /// Check if a node matches size and age filters
    fn matches_filters(node: &FileNode, cutoff_time: Option<u64>, min_size: Option<u64>) -> bool {
        // Size filter
        if let Some(min) = min_size
            && node.size < min
        {
            return false;
        }

        // Age filter (mtime must be older than cutoff)
        if let Some(cutoff) = cutoff_time
            && node.mtime > cutoff
        {
            return false;
        }

        true
    }

    /// Check if a path is in the exclude list
    fn is_excluded(&self, path: &str) -> bool {
        for exclude in &self.category.exclude {
            // Expand ~ in exclude pattern
            let expanded = if exclude.starts_with("~/") {
                dirs::home_dir().map_or_else(
                    || exclude.clone(),
                    |h| h.join(&exclude[2..]).to_string_lossy().to_string(),
                )
            } else {
                exclude.clone()
            };

            if path.starts_with(&expanded) || path.contains(&expanded) {
                return true;
            }
        }
        false
    }

    /// Generate a human-readable reason for this item
    fn generate_reason(&self, node: &FileNode) -> String {
        if let Some(desc) = &self.category.description {
            return desc.clone();
        }

        // Generate based on category characteristics
        if self.category.min_size_bytes.is_some() {
            return format!(
                "Large file ({:.1} MB)",
                node.size as f64 / (1024.0 * 1024.0)
            );
        }

        if self.category.min_age_secs.is_some() {
            let age_days = Self::get_age_days(node.mtime);
            return format!("{} ({} days old)", self.category.name, age_days);
        }

        format!("Matched {}", self.category.name)
    }

    /// Calculate age in days from mtime
    fn get_age_days(mtime: u64) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        now.saturating_sub(mtime) / (24 * 60 * 60)
    }
}

// =============================================================================
// Convenience Functions
// =============================================================================

/// Execute cleanup queries for all categories in a group
pub fn execute_group_cleanup(
    manager: &IndexManager,
    group: CleanupGroup,
) -> Result<GroupSummary, profiles::ProfileError> {
    let profile = load_cleanup()?;
    let categories = profile.categories_by_group(group);

    let results: Vec<ProfileCleanupResult> = categories
        .into_iter()
        .map(|cat| ProfileCleanupQuery::from_category(cat).execute(manager))
        .collect();

    Ok(GroupSummary::new(group, results))
}

/// Execute cleanup queries for all categories and return a summary
pub fn execute_full_cleanup_scan(
    manager: &IndexManager,
) -> Result<ProfileCleanupSummary, profiles::ProfileError> {
    let profile = load_cleanup()?;

    let mut groups: Vec<GroupSummary> = Vec::new();

    for group in profile.active_groups() {
        let categories = profile.categories_by_group(group);
        let results: Vec<ProfileCleanupResult> = categories
            .into_iter()
            .map(|cat| ProfileCleanupQuery::from_category(cat).execute(manager))
            .collect();

        if results.iter().any(|r| r.count > 0) {
            groups.push(GroupSummary::new(group, results));
        }
    }

    let total_bytes = groups.iter().map(|g| g.total_bytes).sum();
    let total_count = groups.iter().map(|g| g.total_count).sum();

    Ok(ProfileCleanupSummary {
        groups,
        total_bytes,
        total_count,
    })
}

/// Execute cleanup query for a single category by ID
pub fn execute_category_cleanup(
    manager: &IndexManager,
    category_id: &str,
) -> Result<Option<ProfileCleanupResult>, profiles::ProfileError> {
    let profile = load_cleanup()?;

    match profile.get(category_id) {
        Some(category) => {
            let result = ProfileCleanupQuery::from_category(category).execute(manager);
            Ok(Some(result))
        }
        None => Ok(None),
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_cleanup_result_empty() {
        let profile = load_cleanup().expect("should load profile");
        let category = profile
            .get("node_modules")
            .expect("should have node_modules");

        let result = ProfileCleanupResult::empty(category);
        assert_eq!(result.count, 0);
        assert_eq!(result.total_bytes, 0);
        assert_eq!(&*result.category_id, "node_modules");
    }

    #[test]
    fn test_profile_cleanup_query_builder() {
        let profile = load_cleanup().expect("should load profile");
        let category = profile
            .get("node_modules")
            .expect("should have node_modules");

        let query = ProfileCleanupQuery::from_category(category)
            .with_limit(100)
            .with_min_size(1024);

        assert_eq!(query.limit, 100);
        assert_eq!(query.min_size_override, Some(1024));
    }

    #[test]
    fn test_is_excluded() {
        let profile = load_cleanup().expect("should load profile");

        // Find a category with excludes (system_caches on macOS has them)
        #[cfg(target_os = "macos")]
        {
            if let Some(category) = profile.get("system_caches") {
                let query = ProfileCleanupQuery::from_category(category);

                // The exclude list should filter out Safari caches etc.
                if !category.exclude.is_empty() {
                    let home = dirs::home_dir().unwrap();
                    let safari_path = home.join("Library/Caches/com.apple.Safari");
                    assert!(query.is_excluded(&safari_path.to_string_lossy()));
                }
            }
        }

        // Suppress unused variable warning on non-macOS
        let _ = profile;
    }

    #[test]
    fn test_group_summary_totals() {
        let result1 = ProfileCleanupResult {
            category_id: Arc::from("test1"),
            category_name: Arc::from("Test 1"),
            group: CleanupGroup::Developer,
            items: vec![],
            total_bytes: 1000,
            count: 5,
            warning: None,
        };

        let result2 = ProfileCleanupResult {
            category_id: Arc::from("test2"),
            category_name: Arc::from("Test 2"),
            group: CleanupGroup::Developer,
            items: vec![],
            total_bytes: 2000,
            count: 10,
            warning: None,
        };

        let summary = GroupSummary::new(CleanupGroup::Developer, vec![result1, result2]);

        assert_eq!(summary.total_bytes, 3000);
        assert_eq!(summary.total_count, 15);
        assert_eq!(summary.categories.len(), 2);
    }

    #[test]
    fn test_matches_filters_size() {
        let small_node = FileNode {
            path: "/test/small.txt".into(),
            size: 100,
            mtime: 0,
            node_type: search::NodeType::File,
        };

        let large_node = FileNode {
            path: "/test/large.txt".into(),
            size: 1_000_000,
            mtime: 0,
            node_type: search::NodeType::File,
        };

        // No size filter - both should match
        assert!(ProfileCleanupQuery::matches_filters(
            &small_node,
            None,
            None
        ));
        assert!(ProfileCleanupQuery::matches_filters(
            &large_node,
            None,
            None
        ));

        // With size filter - only large should match
        assert!(!ProfileCleanupQuery::matches_filters(
            &small_node,
            None,
            Some(1000)
        ));
        assert!(ProfileCleanupQuery::matches_filters(
            &large_node,
            None,
            Some(1000)
        ));
    }

    // =============================================================================
    // Performance Benchmarks
    // =============================================================================

    #[test]
    fn bench_cleanup_query_construction() {
        use std::time::Instant;

        let profile = load_cleanup().expect("should load profile");
        let category = profile
            .get("node_modules")
            .expect("should have node_modules");

        let start = Instant::now();
        for _ in 0..10000 {
            let _ = ProfileCleanupQuery::from_category(category)
                .with_limit(100)
                .with_min_size(1024);
        }
        let construction_time = start.elapsed();

        println!("\n=== Cleanup Query Construction Performance ===");
        println!("10000 query constructions: {construction_time:?}");
        println!("Average per construction: {:?}", construction_time / 10000);

        // Should be very fast - sub-millisecond
        assert!(
            construction_time.as_millis() < 10,
            "query construction too slow: {construction_time:?}"
        );
    }

    #[test]
    fn bench_pattern_parsing() {
        use std::time::Instant;

        let profile = load_cleanup().expect("should load profile");

        // Count total patterns
        let total_patterns: usize = profile.all_categories().map(|c| c.patterns.len()).sum();

        // Measure pattern access
        let start = Instant::now();
        for _ in 0..10000 {
            for cat in profile.all_categories() {
                for pattern in &cat.patterns {
                    // Simulate pattern type detection (what execute_pattern_query does)
                    let _ = pattern.starts_with("~/");
                    let _ = pattern.starts_with("**/");
                }
            }
        }
        let parse_time = start.elapsed();

        println!("\n=== Pattern Parsing Performance ===");
        println!("Total patterns: {total_patterns}");
        println!(
            "10000 iterations ({} pattern checks): {:?}",
            total_patterns * 10000,
            parse_time
        );
        println!(
            "Average per pattern: {:?}",
            parse_time / (total_patterns * 10000) as u32
        );

        // Should be extremely fast
        assert!(
            parse_time.as_millis() < 50,
            "pattern parsing too slow: {parse_time:?}"
        );
    }

    #[test]
    fn bench_filter_matching() {
        use std::time::Instant;

        // Create test nodes
        let small_node = FileNode {
            path: "/test/small.txt".into(),
            size: 100,
            mtime: 0,
            node_type: search::NodeType::File,
        };

        let large_node = FileNode {
            path: "/test/large.txt".into(),
            size: 1_000_000,
            mtime: 0,
            node_type: search::NodeType::File,
        };

        // Measure filter matching
        let start = Instant::now();
        for _ in 0..100000 {
            let _ = ProfileCleanupQuery::matches_filters(&small_node, None, None);
            let _ = ProfileCleanupQuery::matches_filters(&large_node, Some(1000), Some(500));
        }
        let filter_time = start.elapsed();

        println!("\n=== Filter Matching Performance ===");
        println!("200000 filter checks: {filter_time:?}");
        println!("Average per check: {:?}", filter_time / 200000);

        // Should be nanoseconds per check
        assert!(
            filter_time.as_millis() < 50,
            "filter matching too slow: {filter_time:?}"
        );
    }

    #[test]
    fn bench_exclude_checking() {
        use std::time::Instant;

        let profile = load_cleanup().expect("should load profile");

        // Find a category with excludes
        #[cfg(target_os = "macos")]
        let category = profile.get("system_caches");
        #[cfg(not(target_os = "macos"))]
        let category = profile.get("node_modules");

        if let Some(cat) = category {
            let query = ProfileCleanupQuery::from_category(cat);

            let test_paths = vec![
                "/Users/test/Library/Caches/com.apple.Safari/something.db",
                "/Users/test/Library/Caches/com.example.app/cache.dat",
                "/Users/test/Documents/file.txt",
                "/Users/test/node_modules/package/index.js",
            ];

            let start = Instant::now();
            for _ in 0..10000 {
                for path in &test_paths {
                    let _ = query.is_excluded(path);
                }
            }
            let exclude_time = start.elapsed();

            println!("\n=== Exclude Checking Performance ===");
            println!("Excludes in category: {}", cat.exclude.len());
            println!("40000 exclude checks: {exclude_time:?}");
            println!("Average per check: {:?}", exclude_time / 40000);

            // Should be fast even with exclude checking
            assert!(
                exclude_time.as_millis() < 100,
                "exclude checking too slow: {exclude_time:?}"
            );
        }
    }

    #[test]
    fn test_matches_filters_age() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let old_node = FileNode {
            path: "/test/old.txt".into(),
            size: 100,
            mtime: now - (60 * 24 * 60 * 60), // 60 days ago
            node_type: search::NodeType::File,
        };

        let new_node = FileNode {
            path: "/test/new.txt".into(),
            size: 100,
            mtime: now - (5 * 24 * 60 * 60), // 5 days ago
            node_type: search::NodeType::File,
        };

        // Cutoff at 30 days ago
        let cutoff = now - (30 * 24 * 60 * 60);

        // Old file should match (mtime < cutoff)
        assert!(ProfileCleanupQuery::matches_filters(
            &old_node,
            Some(cutoff),
            None
        ));

        // New file should not match (mtime > cutoff)
        assert!(!ProfileCleanupQuery::matches_filters(
            &new_node,
            Some(cutoff),
            None
        ));
    }
}
