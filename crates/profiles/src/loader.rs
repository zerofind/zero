//! Profile loader
//!
//! Loads profile configurations from embedded defaults and user overrides.
//! Embedded profiles are compiled into the binary for zero runtime I/O on defaults.
//! User overrides are loaded from ~/.zero/profiles/ at runtime.
//!
//! ## Loading Order
//!
//! 1. Embedded defaults (compiled into binary)
//! 2. User overrides from `~/.zero/profiles/` (merged on top)
//!
//! ## File Naming Convention
//!
//! Files use `{domain}_{scope}.toml` naming, where scope is `base` (cross-platform)
//! or an OS name (`macos`, `linux`, `windows`):
//!
//! ```text
//! profiles/
//!   file_types_base.toml      # Universal file type definitions
//!   cleanup_base.toml         # Cross-platform cleanup categories
//!   cleanup_macos.toml        # macOS-specific cleanup categories
//!   cleanup_linux.toml        # Linux-specific cleanup categories
//!   cleanup_windows.toml      # Windows-specific cleanup categories
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use super::error::{ProfileError, ProfileResult};
use super::schema::{
    AggregateMode, CleanupCategory, CleanupGroup, CleanupProfile, FileTypesProfile, Os,
};

// =============================================================================
// Embedded Profiles (compiled into binary)
// =============================================================================

/// Embedded `file_types_base.toml`
const EMBEDDED_FILE_TYPES: &str = include_str!("../data/file_types_base.toml");

/// Embedded `cleanup_base.toml`
const EMBEDDED_CLEANUP_BASE: &str = include_str!("../data/cleanup_base.toml");

/// Embedded `cleanup_macos.toml`
#[cfg(target_os = "macos")]
const EMBEDDED_CLEANUP_OS: &str = include_str!("../data/cleanup_macos.toml");

/// Embedded cleanup_linux.toml
#[cfg(target_os = "linux")]
const EMBEDDED_CLEANUP_OS: &str = include_str!("../data/cleanup_linux.toml");

/// Embedded cleanup_windows.toml
#[cfg(target_os = "windows")]
const EMBEDDED_CLEANUP_OS: &str = include_str!("../data/cleanup_windows.toml");

// =============================================================================
// Cached Profiles (loaded once, reused)
// =============================================================================

/// Cached file types profile (Result because parsing can fail)
static FILE_TYPES_CACHE: OnceLock<Result<FileTypesProfile, String>> = OnceLock::new();

/// Cached merged cleanup profile (Result because parsing can fail)
static CLEANUP_CACHE: OnceLock<Result<MergedCleanupProfile, String>> = OnceLock::new();

// =============================================================================
// Merged Profile Types
// =============================================================================

/// A merged cleanup profile with all categories resolved
#[derive(Debug, Clone)]
pub struct MergedCleanupProfile {
    /// All cleanup categories, keyed by ID
    pub categories: HashMap<String, ResolvedCleanupCategory>,
}

/// A resolved cleanup category with parsed values
#[derive(Debug, Clone)]
pub struct ResolvedCleanupCategory {
    /// Category ID (from TOML key)
    pub id: String,

    /// Display name shown in UI
    pub name: String,

    /// UI grouping
    pub group: CleanupGroup,

    /// Resolved glob patterns for current OS
    pub patterns: Vec<String>,

    /// Optional help text
    pub description: Option<String>,

    /// Optional warning before deletion
    pub warning: Option<String>,

    /// Minimum file age in seconds (parsed from "30d" etc.)
    pub min_age_secs: Option<u64>,

    /// Minimum file size in bytes (parsed from "100MB" etc.)
    pub min_size_bytes: Option<u64>,

    /// Paths to exclude
    pub exclude: Vec<String>,

    /// Sibling file required in parent dir for pattern to match
    pub sibling: Option<String>,

    /// How to aggregate results (file-level or directory-level)
    pub aggregate: AggregateMode,
}

impl MergedCleanupProfile {
    /// Get all categories
    pub fn all_categories(&self) -> impl Iterator<Item = &ResolvedCleanupCategory> {
        self.categories.values()
    }

    /// Get categories by group
    pub fn categories_by_group(&self, group: CleanupGroup) -> Vec<&ResolvedCleanupCategory> {
        self.categories
            .values()
            .filter(|c| c.group == group)
            .collect()
    }

    /// Get a category by ID
    pub fn get(&self, id: &str) -> Option<&ResolvedCleanupCategory> {
        self.categories.get(id)
    }

    /// Get all groups that have at least one category
    pub fn active_groups(&self) -> Vec<CleanupGroup> {
        let mut groups: Vec<CleanupGroup> = self
            .categories
            .values()
            .map(|c| c.group)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Sort by display order
        let order = CleanupGroup::all();
        groups.sort_by_key(|g| order.iter().position(|o| o == g).unwrap_or(usize::MAX));
        groups
    }
}

// =============================================================================
// Loading Functions
// =============================================================================

/// Load the file types profile (cached after first load)
///
/// Loads embedded defaults first, then merges any user overrides from
/// `~/.zero/profiles/file_types_base.toml`.
pub fn load_file_types() -> ProfileResult<&'static FileTypesProfile> {
    let result = FILE_TYPES_CACHE.get_or_init(|| {
        // Parse embedded profile
        let mut profile = match toml::from_str::<FileTypesProfile>(EMBEDDED_FILE_TYPES) {
            Ok(p) => p,
            Err(e) => return Err(format!("file_types_base.toml (embedded): {e}")),
        };

        // Load and merge user overrides
        if let Some(user_dir) = user_profiles_dir() {
            let user_file = user_dir.join("file_types_base.toml");
            if user_file.exists()
                && let Ok(content) = fs::read_to_string(&user_file)
                && let Ok(user_profile) = toml::from_str::<FileTypesProfile>(&content)
            {
                // Merge user types into profile (user wins on conflicts)
                for (key, value) in user_profile.types {
                    profile.types.insert(key, value);
                }
            }
        }

        Ok(profile)
    });

    match result {
        Ok(profile) => Ok(profile),
        Err(msg) => Err(ProfileError::NotFound(msg.clone())),
    }
}

/// Load the cleanup profile for the current OS (cached after first load)
pub fn load_cleanup() -> ProfileResult<&'static MergedCleanupProfile> {
    let result = CLEANUP_CACHE.get_or_init(|| match load_cleanup_for_os(Os::current()) {
        Ok(profile) => Ok(profile),
        Err(e) => Err(e.to_string()),
    });

    match result {
        Ok(profile) => Ok(profile),
        Err(msg) => Err(ProfileError::NotFound(msg.clone())),
    }
}

/// Load cleanup profile for a specific OS
///
/// Loading order:
/// 1. Embedded `cleanup_base.toml`
/// 2. Embedded `cleanup_{os}.toml`
/// 3. User `~/.zero/profiles/cleanup_base.toml` (if exists)
/// 4. User `~/.zero/profiles/cleanup_{os}.toml` (if exists)
/// 5. User packs from `~/.zero/profiles/cleanup_packs/*.toml` (if any)
pub fn load_cleanup_for_os(os: Os) -> ProfileResult<MergedCleanupProfile> {
    // Parse embedded base profile
    let base: CleanupProfile = toml::from_str(EMBEDDED_CLEANUP_BASE)
        .map_err(|e| ProfileError::parse("cleanup_base.toml (embedded)", e))?;

    // Parse embedded OS-specific profile
    #[allow(clippy::const_is_empty)]
    let os_specific: CleanupProfile = if EMBEDDED_CLEANUP_OS.is_empty() {
        CleanupProfile {
            categories: HashMap::new(),
        }
    } else {
        toml::from_str(EMBEDDED_CLEANUP_OS).map_err(|e| {
            ProfileError::parse(format!("cleanup_{}.toml (embedded)", os.as_str()), e)
        })?
    };

    // Merge embedded profiles (OS-specific wins on conflicts)
    let mut merged = merge_cleanup_profiles(base, os_specific, os)?;

    // Load and merge user overrides
    if let Some(user_dir) = user_profiles_dir() {
        // Load user cleanup_base.toml
        let user_base_file = user_dir.join("cleanup_base.toml");
        if user_base_file.exists()
            && let Ok(content) = fs::read_to_string(&user_base_file)
            && let Ok(user_base) = toml::from_str::<CleanupProfile>(&content)
        {
            merge_user_cleanup_profile(&mut merged, user_base, os)?;
        }

        // Load user cleanup_{os}.toml
        let user_os_file = user_dir.join(format!("cleanup_{}.toml", os.as_str()));
        if user_os_file.exists()
            && let Ok(content) = fs::read_to_string(&user_os_file)
            && let Ok(user_os) = toml::from_str::<CleanupProfile>(&content)
        {
            merge_user_cleanup_profile(&mut merged, user_os, os)?;
        }

        // Load profile packs from ~/.zero/profiles/cleanup_packs/*.toml
        let packs_dir = user_dir.join("cleanup_packs");
        if packs_dir.is_dir() {
            let mut pack_files: Vec<PathBuf> = fs::read_dir(&packs_dir)
                .into_iter()
                .flatten()
                .filter_map(std::result::Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
                .collect();
            pack_files.sort();

            for pack_file in pack_files {
                if let Ok(content) = fs::read_to_string(&pack_file)
                    && let Ok(pack) = toml::from_str::<CleanupProfile>(&content)
                {
                    merge_user_cleanup_profile(&mut merged, pack, os)?;
                }
            }
        }
    }

    Ok(merged)
}

/// Merge a user cleanup profile into an existing merged profile
fn merge_user_cleanup_profile(
    merged: &mut MergedCleanupProfile,
    user_profile: CleanupProfile,
    os: Os,
) -> ProfileResult<()> {
    for (id, category) in user_profile.categories {
        let resolved = resolve_category(id.clone(), category, os)?;

        if resolved.patterns.is_empty() {
            // If user specifies empty patterns, they might want to disable a category
            // We could either skip it or remove it - let's skip empty patterns
            continue;
        }

        // Check if this is an override of existing category
        if let Some(existing) = merged.categories.get_mut(&id) {
            // Merge fields: user values override existing, but keep existing if user didn't specify
            if !resolved.patterns.is_empty() {
                existing.patterns = resolved.patterns;
            }
            if resolved.description.is_some() {
                existing.description = resolved.description;
            }
            if resolved.warning.is_some() {
                existing.warning = resolved.warning;
            }
            if resolved.min_age_secs.is_some() {
                existing.min_age_secs = resolved.min_age_secs;
            }
            if resolved.min_size_bytes.is_some() {
                existing.min_size_bytes = resolved.min_size_bytes;
            }
            if !resolved.exclude.is_empty() {
                // Append user excludes to existing excludes
                existing.exclude.extend(resolved.exclude);
            }
            if resolved.sibling.is_some() {
                existing.sibling = resolved.sibling;
            }
            if resolved.aggregate != AggregateMode::default() {
                existing.aggregate = resolved.aggregate;
            }
        } else {
            // New category from user profile
            merged.categories.insert(id, resolved);
        }
    }
    Ok(())
}

/// Merge base and OS-specific cleanup profiles
fn merge_cleanup_profiles(
    base: CleanupProfile,
    os_specific: CleanupProfile,
    os: Os,
) -> ProfileResult<MergedCleanupProfile> {
    let mut categories = HashMap::new();

    // Process base categories first
    for (id, category) in base.categories {
        let resolved = resolve_category(id.clone(), category, os)?;
        if !resolved.patterns.is_empty() {
            categories.insert(id, resolved);
        }
    }

    // Merge OS-specific categories (overwrites base if same ID)
    for (id, category) in os_specific.categories {
        let resolved = resolve_category(id.clone(), category, os)?;
        if !resolved.patterns.is_empty() {
            categories.insert(id, resolved);
        }
    }

    Ok(MergedCleanupProfile { categories })
}

/// Resolve a cleanup category, parsing duration and size values
fn resolve_category(
    id: String,
    category: CleanupCategory,
    os: Os,
) -> ProfileResult<ResolvedCleanupCategory> {
    // Get patterns for this OS
    let patterns: Vec<String> = category
        .patterns
        .for_os(os)
        .into_iter()
        .map(std::string::ToString::to_string)
        .collect();

    // Parse min_age
    let min_age_secs = category
        .min_age
        .as_ref()
        .map(|s| parse_duration(s))
        .transpose()
        .map_err(|()| {
            ProfileError::invalid_duration(&id, category.min_age.as_deref().unwrap_or(""))
        })?;

    // Parse min_size
    let min_size_bytes = category
        .min_size
        .as_ref()
        .map(|s| parse_size(s))
        .transpose()
        .map_err(|()| {
            ProfileError::invalid_size(&id, category.min_size.as_deref().unwrap_or(""))
        })?;

    Ok(ResolvedCleanupCategory {
        id,
        name: category.name,
        group: category.group,
        patterns,
        description: category.description,
        warning: category.warning,
        min_age_secs,
        min_size_bytes,
        exclude: category.exclude,
        sibling: category.sibling,
        aggregate: category.aggregate.unwrap_or_default(),
    })
}

// =============================================================================
// Parsing Utilities
// =============================================================================

/// Parse a duration string like "30d", "24h", "60m" into seconds
pub fn parse_duration(s: &str) -> Result<u64, ()> {
    let s = s.trim();
    if s.is_empty() {
        return Err(());
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str.parse().map_err(|_| ())?;

    let multiplier = match unit {
        "d" => 24 * 60 * 60, // days
        "h" => 60 * 60,      // hours
        "m" => 60,           // minutes
        "s" => 1,            // seconds
        _ => return Err(()),
    };

    Ok(num * multiplier)
}

/// Parse a size string like "100MB", "1GB", "500KB" into bytes
pub fn parse_size(s: &str) -> Result<u64, ()> {
    let s = s.trim().to_uppercase();
    if s.is_empty() {
        return Err(());
    }

    // Find where the number ends and unit begins
    let num_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());

    let (num_str, unit) = s.split_at(num_end);
    let num: u64 = num_str.parse().map_err(|_| ())?;

    let multiplier = match unit {
        "B" | "" => 1,
        "KB" | "K" => 1024,
        "MB" | "M" => 1024 * 1024,
        "GB" | "G" => 1024 * 1024 * 1024,
        "TB" | "T" => 1024 * 1024 * 1024 * 1024,
        _ => return Err(()),
    };

    Ok(num * multiplier)
}

/// Get the user config directory for profiles (`~/.zero/profiles/`)
pub fn user_profiles_dir() -> Option<PathBuf> {
    foundation::dirs::profiles_dir()
}

/// Create the user profiles directory if it doesn't exist
///
/// Returns the path to the created directory, or None if creation failed.
pub fn ensure_user_profiles_dir() -> Option<PathBuf> {
    let dir = user_profiles_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).ok()?;
    }
    Some(dir)
}

/// Check if user has any profile overrides
pub fn has_user_overrides() -> bool {
    user_profiles_dir().is_some_and(|dir| dir.exists() && dir.is_dir())
}
