//! Ultra-fast file type indexing using Roaring Bitmaps
//!
//! This module provides O(1) type filtering using pre-computed bitmaps.
//! Instead of scanning all files and checking extensions, we maintain
//! a bitmap per file type where bit N = 1 if file N matches that type.
//!
//! Extension mappings can be loaded from:
//! 1. Profile configuration (`profiles/file_types_base.toml`) - preferred
//! 2. Hardcoded fallback constants - for backward compatibility
//!
//! See `ARCHITECTURE.md` for the full design rationale and performance analysis.
//!
//! ## Why Roaring Bitmaps?
//!
//! The problem: Type filtering (e.g., "show all images") requires checking each
//! file's extension against a list. For 1.3M files × 20 image extensions = 26M
//! string comparisons = ~1.7 seconds.
//!
//! Roaring bitmaps solve this by pre-computing "is file N an image?" at index time:
//! - Query becomes `bitmap.iter().take(limit)` — `O(result_count)` not `O(total_files)`
//! - SIMD-optimized: 256 files checked per CPU instruction
//! - Compressed: Run-length encoding for clustered indices
//! - Battle-tested: Used by `ClickHouse`, Elasticsearch, Apache Druid
//!
//! ## Performance
//!
//! | Query | Before | After | Speedup |
//! |-------|--------|-------|---------|
//! | `--type images` | 1,700ms | 0.04ms | 42,500× |
//! | `--type code` | 1,700ms | 0.01ms | 170,000× |
//!
//! ## Memory Usage
//!
//! Roaring bitmaps are compressed and typically use:
//! - ~1 bit per file for sparse types
//! - Much less for clustered file indices (run-length encoding)
//!
//! For 1.3M files: ~200KB total for all type bitmaps
//!
//! ## Usage
//!
//! ```ignore
//! // At index time
//! type_index.add_file(idx, Some("jpg"), false);
//!
//! // At query time - O(limit) not O(total_files)!
//! let image_indices = type_index.get_indices_limited(FileTypeCategory::Images, 100);
//! ```

use clap::ValueEnum;
use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// File type categories for filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
pub enum FileTypeCategory {
    #[value(
        alias = "image",
        alias = "img",
        alias = "picture",
        alias = "pictures",
        alias = "photo",
        alias = "photos"
    )]
    Images,
    #[value(alias = "video", alias = "vid", alias = "movie", alias = "movies")]
    Videos,
    #[value(alias = "music", alias = "sound", alias = "song", alias = "songs")]
    Audio,
    #[value(
        alias = "docs",
        alias = "doc",
        alias = "document",
        alias = "text",
        alias = "office"
    )]
    Documents,
    #[value(alias = "source", alias = "src", alias = "dev")]
    Code,
    #[value(alias = "archive", alias = "zip", alias = "compressed")]
    Archives,
    #[value(alias = "configs", alias = "configuration")]
    Config,
    #[value(
        alias = "executable",
        alias = "exe",
        alias = "exec",
        alias = "program",
        alias = "programs",
        alias = "app",
        alias = "apps"
    )]
    Executables,
    #[value(alias = "font")]
    Fonts,
    /// All regular files (not directories)
    #[value(alias = "file")]
    Files,
    /// All directories
    #[value(
        alias = "directory",
        alias = "dirs",
        alias = "dir",
        alias = "folders",
        alias = "folder"
    )]
    Directories,
    // Quick language-specific types
    #[value(alias = "rs")]
    Rust,
    #[value(alias = "golang")]
    Go,
    Swift,
    #[value(alias = "ts")]
    TypeScript,
    #[value(alias = "js")]
    JavaScript,
    #[value(alias = "py")]
    Python,
}

impl FileTypeCategory {
    /// Get all categories
    pub fn all() -> &'static [FileTypeCategory] {
        &[
            FileTypeCategory::Images,
            FileTypeCategory::Videos,
            FileTypeCategory::Audio,
            FileTypeCategory::Documents,
            FileTypeCategory::Code,
            FileTypeCategory::Archives,
            FileTypeCategory::Config,
            FileTypeCategory::Executables,
            FileTypeCategory::Fonts,
            FileTypeCategory::Files,
            FileTypeCategory::Directories,
            FileTypeCategory::Rust,
            FileTypeCategory::Go,
            FileTypeCategory::Swift,
            FileTypeCategory::TypeScript,
            FileTypeCategory::JavaScript,
            FileTypeCategory::Python,
        ]
    }

    /// Parse from string (case-insensitive)
    pub fn parse_str(s: &str) -> Option<FileTypeCategory> {
        match s.to_lowercase().as_str() {
            "images" | "image" | "img" | "picture" | "pictures" | "photo" | "photos" => {
                Some(FileTypeCategory::Images)
            }
            "videos" | "video" | "vid" | "movie" | "movies" => Some(FileTypeCategory::Videos),
            "audio" | "music" | "sound" | "song" | "songs" => Some(FileTypeCategory::Audio),
            "documents" | "docs" | "doc" | "document" | "text" | "office" => {
                Some(FileTypeCategory::Documents)
            }
            "code" | "source" | "src" | "dev" => Some(FileTypeCategory::Code),
            // Quick language aliases
            "rs" | "rust" => Some(FileTypeCategory::Rust),
            "go" | "golang" => Some(FileTypeCategory::Go),
            "swift" => Some(FileTypeCategory::Swift),
            "ts" | "typescript" => Some(FileTypeCategory::TypeScript),
            "js" | "javascript" => Some(FileTypeCategory::JavaScript),
            "py" | "python" => Some(FileTypeCategory::Python),
            "archives" | "archive" | "zip" | "compressed" => Some(FileTypeCategory::Archives),
            "config" | "configs" | "configuration" => Some(FileTypeCategory::Config),
            "executables" | "executable" | "exe" | "exec" | "program" | "programs" | "app"
            | "apps" => Some(FileTypeCategory::Executables),
            "fonts" | "font" => Some(FileTypeCategory::Fonts),
            "files" | "file" => Some(FileTypeCategory::Files),
            "directories" | "directory" | "dirs" | "dir" | "folders" | "folder" => {
                Some(FileTypeCategory::Directories)
            }
            _ => None,
        }
    }

    /// Get extensions for this category (None for Files/Directories which use node type)
    pub fn extensions(&self) -> Option<&'static [&'static str]> {
        match self {
            FileTypeCategory::Images => Some(IMAGE_EXTENSIONS),
            FileTypeCategory::Videos => Some(VIDEO_EXTENSIONS),
            FileTypeCategory::Audio => Some(AUDIO_EXTENSIONS),
            FileTypeCategory::Documents => Some(DOCUMENT_EXTENSIONS),
            FileTypeCategory::Code => Some(CODE_EXTENSIONS),
            FileTypeCategory::Archives => Some(ARCHIVE_EXTENSIONS),
            FileTypeCategory::Config => Some(CONFIG_EXTENSIONS),
            FileTypeCategory::Executables => Some(EXECUTABLE_EXTENSIONS),
            FileTypeCategory::Fonts => Some(FONT_EXTENSIONS),
            FileTypeCategory::Rust => Some(RUST_EXTENSIONS),
            FileTypeCategory::Go => Some(GO_EXTENSIONS),
            FileTypeCategory::Swift => Some(SWIFT_EXTENSIONS),
            FileTypeCategory::TypeScript => Some(TYPESCRIPT_EXTENSIONS),
            FileTypeCategory::JavaScript => Some(JAVASCRIPT_EXTENSIONS),
            FileTypeCategory::Python => Some(PYTHON_EXTENSIONS),
            FileTypeCategory::Files | FileTypeCategory::Directories => None,
        }
    }
}

// Extension lists for each category
const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "heic", "heif", "bmp", "tiff", "tif", "svg", "ico", "raw",
    "cr2", "nef", "arw", "orf", "raf", "psd", "ai", "eps", "dng", "rw2", "srw", "x3f",
];

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "avi", "mkv", "webm", "m4v", "wmv", "flv", "mpg", "mpeg", "3gp", "3g2", "m2ts",
    "vob", "ogv", "rm", "rmvb", "asf",
    "divx",
    // Note: .ts and .mts excluded - too ambiguous with TypeScript
];

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "flac", "aac", "ogg", "oga", "opus", "wma", "m4a", "alac", "aiff", "aif", "mid",
    "midi", "ape", "wv", "mka", "ac3", "dts",
];

const DOCUMENT_EXTENSIONS: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "rtf", "txt", "md",
    "markdown", "tex", "pages", "numbers", "key", "epub", "mobi", "azw", "azw3", "djvu", "chm",
];

const CODE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "jsx", "tsx", "go", "java", "c", "cpp", "cc", "cxx", "h", "hpp", "hh",
    "swift", "kt", "kts", "rb", "php", "cs", "scala", "zig", "hs", "ml", "mli", "ex", "exs", "erl",
    "hrl", "clj", "cljs", "lisp", "scm", "rkt", "lua", "pl", "pm", "r", "jl", "nim", "cr", "v",
    "d", "f", "f90", "f95", "asm", "s", "sql", "sh", "bash", "zsh", "fish", "ps1", "psm1", "bat",
    "cmd", "vue", "svelte", "elm", "zig",
];

const ARCHIVE_EXTENSIONS: &[&str] = &[
    "zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar", "dmg", "iso", "cab", "lzma", "lz", "zst",
    "lz4", "br", "Z", "arj", "ace", "sit", "sitx",
];

const CONFIG_EXTENSIONS: &[&str] = &[
    "json",
    "yaml",
    "yml",
    "toml",
    "xml",
    "ini",
    "conf",
    "cfg",
    "env",
    "properties",
    "plist",
    "editorconfig",
    "gitignore",
    "gitattributes",
    "dockerignore",
    "npmrc",
    "nvmrc",
];

const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "exe", "msi", "app", "apk", "ipa", "jar", "deb", "rpm", "pkg", "bin", "run", "appimage", "com",
    "scr", "gadget", "msp", "msu",
];

const FONT_EXTENSIONS: &[&str] = &[
    "ttf", "otf", "woff", "woff2", "eot", "fon", "fnt", "pfb", "pfm", "afm",
];

// Quick language-specific extensions
const RUST_EXTENSIONS: &[&str] = &["rs"];
const GO_EXTENSIONS: &[&str] = &["go"];
const SWIFT_EXTENSIONS: &[&str] = &["swift"];
const TYPESCRIPT_EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts"];
const JAVASCRIPT_EXTENSIONS: &[&str] = &["js", "jsx", "mjs", "cjs"];
const PYTHON_EXTENSIONS: &[&str] = &["py", "pyi", "pyw"];

/// Fast type index using roaring bitmaps
///
/// Each category has a bitmap where bit N = 1 means file at index N matches that type.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TypeIndex {
    /// Bitmap per file type category
    bitmaps: HashMap<FileTypeCategory, RoaringBitmap>,

    /// Extension to category mapping for fast lookups during indexing
    #[serde(skip)]
    extension_map: HashMap<String, Vec<FileTypeCategory>>,

    /// Total files indexed
    total_files: u32,

    /// Bitmap for files in Trash (path contains /.Trash/)
    /// Used for O(1) trash filtering during search
    #[serde(default)]
    trash_bitmap: RoaringBitmap,

    /// Bitmap for files under hidden directories (component starts with '.')
    /// Used for O(1) hidden file filtering during search
    #[serde(default)]
    hidden_bitmap: RoaringBitmap,

    /// Per-extension bitmaps for O(1) extension lookups
    /// Key is lowercase extension without dot (e.g., "log", "dmg", "rs")
    #[serde(default)]
    extension_bitmaps: HashMap<String, RoaringBitmap>,

    /// Per-path-component bitmaps for O(1) folder pattern lookups
    /// Key is lowercase directory name (e.g., "`node_modules`", "target", ".trash")
    /// Value is bitmap of all file indices whose path contains that component
    /// Only components in `indexed_components` are tracked (selective indexing).
    #[serde(default)]
    path_component_bitmaps: HashMap<String, RoaringBitmap>,

    /// Allowlist of path components to index. Only these components get bitmaps.
    /// Loaded from cleanup profiles at construction time.
    #[serde(skip)]
    indexed_components: HashSet<String>,
}

impl TypeIndex {
    /// Create a new empty type index
    ///
    /// Attempts to load extension mappings from profile configuration.
    /// Falls back to hardcoded defaults if profile loading fails.
    pub fn new() -> Self {
        let indexed_components = Self::load_indexed_components();
        let mut index = Self {
            bitmaps: HashMap::new(),
            extension_map: HashMap::new(),
            total_files: 0,
            trash_bitmap: RoaringBitmap::new(),
            hidden_bitmap: RoaringBitmap::new(),
            extension_bitmaps: HashMap::new(),
            path_component_bitmaps: HashMap::new(),
            indexed_components,
        };

        // Try to load from profile, fall back to hardcoded
        if !index.try_load_from_profile() {
            index.build_extension_map_from_constants();
        }

        index
    }

    /// Create a new type index from a file types profile
    ///
    /// This is the preferred way to create a `TypeIndex` when you have
    /// already loaded the profile.
    pub fn from_profile(profile: &profiles::FileTypesProfile) -> Self {
        let indexed_components = Self::load_indexed_components();
        let mut index = Self {
            bitmaps: HashMap::new(),
            extension_map: HashMap::new(),
            total_files: 0,
            trash_bitmap: RoaringBitmap::new(),
            hidden_bitmap: RoaringBitmap::new(),
            extension_bitmaps: HashMap::new(),
            path_component_bitmaps: HashMap::new(),
            indexed_components,
        };
        index.build_extension_map_from_profile(profile);
        index
    }

    /// Load the set of path components to index from cleanup profiles.
    ///
    /// Extracts folder patterns (e.g., `**/node_modules` → `node_modules`)
    /// from the cleanup profile. Only these components get `path_component_bitmaps`.
    fn load_indexed_components() -> HashSet<String> {
        let mut components = HashSet::new();

        // Always include these common components
        components.insert(".trash".to_string());

        if let Ok(profile) = profiles::load_cleanup() {
            for cat in profile.all_categories() {
                for pattern in &cat.patterns {
                    if let Some(search_term) = pattern.strip_prefix("**/")
                        && !search_term.starts_with('.')
                        && !search_term.starts_with("*.")
                        && !search_term.ends_with("/*")
                    {
                        components.insert(search_term.to_lowercase());
                    }
                }
            }
        }

        components
    }

    /// Set custom indexed components (for testing)
    pub fn set_indexed_components(&mut self, components: HashSet<String>) {
        self.indexed_components = components;
    }

    /// Try to load extension mappings from the profile system
    ///
    /// Returns true if successful, false if profile loading failed.
    fn try_load_from_profile(&mut self) -> bool {
        match profiles::load_file_types() {
            Ok(profile) => {
                self.build_extension_map_from_profile(profile);
                true
            }
            Err(_) => false,
        }
    }

    /// Build extension map from profile configuration
    fn build_extension_map_from_profile(&mut self, profile: &profiles::FileTypesProfile) {
        self.extension_map.clear();

        for (category_name, config) in &profile.types {
            // Map profile category name to FileTypeCategory
            if let Some(category) = Self::profile_category_to_type(category_name) {
                for ext in &config.extensions {
                    self.extension_map
                        .entry(ext.to_lowercase())
                        .or_default()
                        .push(category);
                }
            }
        }
    }

    /// Map profile category name to `FileTypeCategory` enum
    fn profile_category_to_type(name: &str) -> Option<FileTypeCategory> {
        match name.to_lowercase().as_str() {
            "images" => Some(FileTypeCategory::Images),
            "videos" => Some(FileTypeCategory::Videos),
            "audio" => Some(FileTypeCategory::Audio),
            "documents" => Some(FileTypeCategory::Documents),
            "code" => Some(FileTypeCategory::Code),
            "archives" => Some(FileTypeCategory::Archives),
            "config" => Some(FileTypeCategory::Config),
            "executables" => Some(FileTypeCategory::Executables),
            "fonts" => Some(FileTypeCategory::Fonts),
            "rust" => Some(FileTypeCategory::Rust),
            "go" => Some(FileTypeCategory::Go),
            "swift" => Some(FileTypeCategory::Swift),
            "typescript" => Some(FileTypeCategory::TypeScript),
            "javascript" => Some(FileTypeCategory::JavaScript),
            "python" => Some(FileTypeCategory::Python),
            _ => None, // Unknown categories are ignored
        }
    }

    /// Build extension map from hardcoded constants (fallback)
    fn build_extension_map_from_constants(&mut self) {
        self.extension_map.clear();

        for category in FileTypeCategory::all() {
            if let Some(extensions) = category.extensions() {
                for ext in extensions {
                    self.extension_map
                        .entry((*ext).to_string())
                        .or_default()
                        .push(*category);
                }
            }
        }
    }

    /// Ensure extension map is populated (call after deserialization)
    ///
    /// Attempts to load from profile first, falls back to constants.
    pub fn ensure_extension_map(&mut self) {
        if self.extension_map.is_empty() && !self.try_load_from_profile() {
            self.build_extension_map_from_constants();
        }
        if self.indexed_components.is_empty() {
            self.indexed_components = Self::load_indexed_components();
        }
    }

    /// Get the number of extensions mapped
    pub fn extension_count(&self) -> usize {
        self.extension_map.len()
    }

    /// Check if an extension is mapped to any category
    pub fn has_extension(&self, ext: &str) -> bool {
        self.extension_map.contains_key(&ext.to_lowercase())
    }

    /// Get categories for an extension
    pub fn categories_for_extension(&self, ext: &str) -> Option<&Vec<FileTypeCategory>> {
        self.extension_map.get(&ext.to_lowercase())
    }

    /// Add a file to the index
    ///
    /// # Arguments
    /// * `index` - The slab index of the file
    /// * `path` - The full path of the file (for trash detection)
    /// * `extension` - The file extension (lowercase, without dot)
    /// * `is_directory` - Whether this is a directory
    pub fn add_file(
        &mut self,
        index: u32,
        path: &str,
        extension: Option<&str>,
        is_directory: bool,
    ) {
        self.total_files = self.total_files.max(index + 1);

        // Check if file is in Trash
        if path.contains("/.Trash/") || path.ends_with("/.Trash") {
            self.trash_bitmap.insert(index);
        }

        // Scan path components for hidden detection and selective bitmap indexing.
        let mut is_hidden = false;
        for component in path.split('/') {
            if component.is_empty() || component == "." || component == ".." {
                continue;
            }

            // Hidden: starts with '.' and length > 1
            if !is_hidden && component.len() > 1 && component.as_bytes()[0] == b'.' {
                is_hidden = true;
            }

            // Selective path component bitmap indexing
            if !self.indexed_components.is_empty() {
                let component_lower = component.to_lowercase();
                if self.indexed_components.contains(&component_lower) {
                    self.path_component_bitmaps
                        .entry(component_lower)
                        .or_default()
                        .insert(index);
                }
            }
        }

        if is_hidden {
            self.hidden_bitmap.insert(index);
        }

        if is_directory {
            self.bitmaps
                .entry(FileTypeCategory::Directories)
                .or_default()
                .insert(index);
        } else {
            // Mark as a file
            self.bitmaps
                .entry(FileTypeCategory::Files)
                .or_default()
                .insert(index);

            // Check extension categories and add to extension bitmap
            if let Some(ext) = extension {
                let ext_lower = ext.to_lowercase();

                // Add to per-extension bitmap for O(1) extension lookups
                self.extension_bitmaps
                    .entry(ext_lower.clone())
                    .or_default()
                    .insert(index);

                // Add to category bitmaps
                if let Some(categories) = self.extension_map.get(&ext_lower) {
                    for category in categories {
                        self.bitmaps.entry(*category).or_default().insert(index);
                    }
                }
            }
        }
    }

    /// Remove a file from the index
    pub fn remove_file(&mut self, index: u32) {
        for bitmap in self.bitmaps.values_mut() {
            bitmap.remove(index);
        }
        for bitmap in self.extension_bitmaps.values_mut() {
            bitmap.remove(index);
        }
        for bitmap in self.path_component_bitmaps.values_mut() {
            bitmap.remove(index);
        }
        self.trash_bitmap.remove(index);
        self.hidden_bitmap.remove(index);
    }

    /// Check if a file index is in trash
    #[inline]
    pub fn is_in_trash(&self, index: u32) -> bool {
        self.trash_bitmap.contains(index)
    }

    /// Get the trash bitmap for bulk filtering
    pub fn trash_bitmap(&self) -> &RoaringBitmap {
        &self.trash_bitmap
    }

    /// Get count of files in trash
    pub fn trash_count(&self) -> u64 {
        self.trash_bitmap.len()
    }

    /// Check if a file index is under a hidden directory
    #[inline]
    pub fn is_hidden(&self, index: u32) -> bool {
        self.hidden_bitmap.contains(index)
    }

    /// Get all file indices matching a specific extension
    ///
    /// This is O(1) bitmap lookup - much faster than text search.
    /// Extension should be lowercase without the dot (e.g., "log", "dmg", "rs").
    #[inline]
    pub fn get_by_extension(&self, ext: &str) -> Option<&RoaringBitmap> {
        self.extension_bitmaps.get(&ext.to_lowercase())
    }

    /// Get file count for a specific extension
    pub fn count_by_extension(&self, ext: &str) -> u64 {
        self.extension_bitmaps
            .get(&ext.to_lowercase())
            .map_or(0, roaring::RoaringBitmap::len)
    }

    /// Get all indexed extensions
    pub fn all_extensions(&self) -> impl Iterator<Item = &String> {
        self.extension_bitmaps.keys()
    }

    /// Get file indices for an extension, with optional limit
    pub fn get_extension_indices_limited(&self, ext: &str, limit: usize) -> Vec<u32> {
        self.extension_bitmaps
            .get(&ext.to_lowercase())
            .map(|bitmap| bitmap.iter().take(limit).collect())
            .unwrap_or_default()
    }

    /// Get all file indices whose path contains a specific directory component
    ///
    /// This is O(1) bitmap lookup - much faster than text search.
    /// Component should be the directory name (e.g., "`node_modules`", "target", ".Trash").
    #[inline]
    pub fn get_by_path_component(&self, component: &str) -> Option<&RoaringBitmap> {
        self.path_component_bitmaps.get(&component.to_lowercase())
    }

    /// Get file count for a specific path component
    pub fn count_by_path_component(&self, component: &str) -> u64 {
        self.path_component_bitmaps
            .get(&component.to_lowercase())
            .map_or(0, roaring::RoaringBitmap::len)
    }

    /// Get all indexed path components
    pub fn all_path_components(&self) -> impl Iterator<Item = &String> {
        self.path_component_bitmaps.keys()
    }

    /// Get file indices for a path component, with optional limit
    pub fn get_path_component_indices_limited(&self, component: &str, limit: usize) -> Vec<u32> {
        self.path_component_bitmaps
            .get(&component.to_lowercase())
            .map(|bitmap| bitmap.iter().take(limit).collect())
            .unwrap_or_default()
    }

    /// Get count of unique path components indexed
    pub fn unique_path_component_count(&self) -> usize {
        self.path_component_bitmaps.len()
    }

    /// Get all file indices matching a type category
    ///
    /// This is the fast path - `O(result_count)` instead of `O(total_files)`
    pub fn get_indices(&self, category: FileTypeCategory) -> Option<&RoaringBitmap> {
        self.bitmaps.get(&category)
    }

    /// Get file indices matching a type, with optional limit
    ///
    /// Returns indices in ascending order
    pub fn get_indices_limited(&self, category: FileTypeCategory, limit: usize) -> Vec<u32> {
        self.bitmaps
            .get(&category)
            .map(|bitmap| bitmap.iter().take(limit).collect())
            .unwrap_or_default()
    }

    /// Get count of files in a category
    pub fn count(&self, category: FileTypeCategory) -> u64 {
        self.bitmaps
            .get(&category)
            .map_or(0, roaring::RoaringBitmap::len)
    }

    /// Get total files indexed
    pub fn total_files(&self) -> u32 {
        self.total_files
    }

    /// Check if a specific file matches a category
    pub fn contains(&self, category: FileTypeCategory, index: u32) -> bool {
        self.bitmaps
            .get(&category)
            .is_some_and(|b| b.contains(index))
    }

    /// Intersect with another bitmap (for combining filters)
    pub fn intersect(
        &self,
        category: FileTypeCategory,
        other: &RoaringBitmap,
    ) -> Option<RoaringBitmap> {
        self.bitmaps.get(&category).map(|b| b & other)
    }

    /// Union multiple categories (e.g., images OR videos)
    pub fn union_categories(&self, categories: &[FileTypeCategory]) -> RoaringBitmap {
        let mut result = RoaringBitmap::new();
        for category in categories {
            if let Some(bitmap) = self.bitmaps.get(category) {
                result |= bitmap;
            }
        }
        result
    }

    /// Clear all bitmaps
    /// Clear all data
    pub fn clear(&mut self) {
        self.bitmaps.clear();
        self.trash_bitmap.clear();
        self.hidden_bitmap.clear();
        self.total_files = 0;
    }

    /// Get memory usage estimate in bytes
    pub fn memory_usage(&self) -> usize {
        let category_size: usize = self
            .bitmaps
            .values()
            .map(roaring::RoaringBitmap::serialized_size)
            .sum();
        let extension_size: usize = self
            .extension_bitmaps
            .values()
            .map(roaring::RoaringBitmap::serialized_size)
            .sum();
        let path_component_size: usize = self
            .path_component_bitmaps
            .values()
            .map(roaring::RoaringBitmap::serialized_size)
            .sum();
        category_size + extension_size + path_component_size + self.trash_bitmap.serialized_size()
    }

    /// Get count of unique extensions indexed
    pub fn unique_extension_count(&self) -> usize {
        self.extension_bitmaps.len()
    }

    /// Get statistics about the index
    pub fn stats(&self) -> TypeIndexStats {
        TypeIndexStats {
            total_files: u64::from(self.total_files),
            images: self.count(FileTypeCategory::Images),
            videos: self.count(FileTypeCategory::Videos),
            audio: self.count(FileTypeCategory::Audio),
            documents: self.count(FileTypeCategory::Documents),
            code: self.count(FileTypeCategory::Code),
            archives: self.count(FileTypeCategory::Archives),
            config: self.count(FileTypeCategory::Config),
            executables: self.count(FileTypeCategory::Executables),
            fonts: self.count(FileTypeCategory::Fonts),
            files: self.count(FileTypeCategory::Files),
            directories: self.count(FileTypeCategory::Directories),
            trash: self.trash_count(),
            memory_bytes: self.memory_usage(),
        }
    }
}

/// Statistics about the type index
#[derive(Debug, Clone)]
pub struct TypeIndexStats {
    pub total_files: u64,
    pub images: u64,
    pub videos: u64,
    pub audio: u64,
    pub documents: u64,
    pub code: u64,
    pub archives: u64,
    pub config: u64,
    pub executables: u64,
    pub fonts: u64,
    pub files: u64,
    pub directories: u64,
    pub trash: u64,
    pub memory_bytes: usize,
}

impl std::fmt::Display for TypeIndexStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Type Index Statistics:")?;
        writeln!(f, "  Total files: {}", self.total_files)?;
        writeln!(f, "  Images: {}", self.images)?;
        writeln!(f, "  Videos: {}", self.videos)?;
        writeln!(f, "  Audio: {}", self.audio)?;
        writeln!(f, "  Documents: {}", self.documents)?;
        writeln!(f, "  Code: {}", self.code)?;
        writeln!(f, "  Archives: {}", self.archives)?;
        writeln!(f, "  Config: {}", self.config)?;
        writeln!(f, "  Executables: {}", self.executables)?;
        writeln!(f, "  Fonts: {}", self.fonts)?;
        writeln!(f, "  Files: {}", self.files)?;
        writeln!(f, "  Directories: {}", self.directories)?;
        writeln!(f, "  Trash: {}", self.trash)?;
        writeln!(f, "  Memory usage: {} bytes", self.memory_bytes)
    }
}

/// Helper to extract extension from a filename
pub fn extract_extension(filename: &str) -> Option<&str> {
    let pos = filename.rfind('.')?;
    if pos + 1 >= filename.len() {
        return None;
    }
    Some(&filename[pos + 1..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_category_from_str() {
        assert_eq!(
            FileTypeCategory::parse_str("images"),
            Some(FileTypeCategory::Images)
        );
        assert_eq!(
            FileTypeCategory::parse_str("IMAGES"),
            Some(FileTypeCategory::Images)
        );
        assert_eq!(
            FileTypeCategory::parse_str("picture"),
            Some(FileTypeCategory::Images)
        );
        assert_eq!(
            FileTypeCategory::parse_str("videos"),
            Some(FileTypeCategory::Videos)
        );
        assert_eq!(FileTypeCategory::parse_str("unknown"), None);
    }

    #[test]
    fn test_add_and_query() {
        let mut index = TypeIndex::new();

        // Add some files
        index.add_file(0, "/path/to/image.jpg", Some("jpg"), false);
        index.add_file(1, "/path/to/image.png", Some("png"), false);
        index.add_file(2, "/path/to/video.mp4", Some("mp4"), false);
        index.add_file(3, "/path/to/code.rs", Some("rs"), false);
        index.add_file(4, "/path/to/dir", None, true); // directory

        // Query images
        let images = index.get_indices_limited(FileTypeCategory::Images, 100);
        assert_eq!(images, vec![0, 1]);

        // Query videos
        let videos = index.get_indices_limited(FileTypeCategory::Videos, 100);
        assert_eq!(videos, vec![2]);

        // Query code
        let code = index.get_indices_limited(FileTypeCategory::Code, 100);
        assert_eq!(code, vec![3]);

        // Query directories
        let dirs = index.get_indices_limited(FileTypeCategory::Directories, 100);
        assert_eq!(dirs, vec![4]);

        // Query all files (not dirs)
        let files = index.get_indices_limited(FileTypeCategory::Files, 100);
        assert_eq!(files, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_count() {
        let mut index = TypeIndex::new();

        for i in 0..1000 {
            if i % 3 == 0 {
                index.add_file(i, &format!("/path/file{}.jpg", i), Some("jpg"), false);
            } else if i % 3 == 1 {
                index.add_file(i, &format!("/path/file{}.mp4", i), Some("mp4"), false);
            } else {
                index.add_file(i, &format!("/path/file{}.rs", i), Some("rs"), false);
            }
        }

        assert_eq!(index.count(FileTypeCategory::Images), 334);
        assert_eq!(index.count(FileTypeCategory::Videos), 333);
        assert_eq!(index.count(FileTypeCategory::Code), 333);
        assert_eq!(index.count(FileTypeCategory::Files), 1000);
    }

    #[test]
    fn test_union_categories() {
        let mut index = TypeIndex::new();

        index.add_file(0, "/path/image.jpg", Some("jpg"), false);
        index.add_file(1, "/path/video.mp4", Some("mp4"), false);
        index.add_file(2, "/path/audio.mp3", Some("mp3"), false);
        index.add_file(3, "/path/doc.txt", Some("txt"), false);

        let media = index.union_categories(&[
            FileTypeCategory::Images,
            FileTypeCategory::Videos,
            FileTypeCategory::Audio,
        ]);

        assert_eq!(media.len(), 3);
        assert!(media.contains(0));
        assert!(media.contains(1));
        assert!(media.contains(2));
        assert!(!media.contains(3));
    }

    #[test]
    fn test_remove_file() {
        let mut index = TypeIndex::new();

        index.add_file(0, "/path/image1.jpg", Some("jpg"), false);
        index.add_file(1, "/path/image2.png", Some("png"), false);

        assert_eq!(index.count(FileTypeCategory::Images), 2);

        index.remove_file(0);

        assert_eq!(index.count(FileTypeCategory::Images), 1);
        assert!(!index.contains(FileTypeCategory::Images, 0));
        assert!(index.contains(FileTypeCategory::Images, 1));
    }

    #[test]
    fn test_extract_extension() {
        assert_eq!(extract_extension("file.jpg"), Some("jpg"));
        assert_eq!(extract_extension("file.tar.gz"), Some("gz"));
        assert_eq!(extract_extension("file"), None);
        assert_eq!(extract_extension("file."), None);
        assert_eq!(extract_extension(".hidden"), Some("hidden"));
    }

    #[test]
    fn test_trash_bitmap() {
        let mut index = TypeIndex::new();

        // Add regular files
        index.add_file(0, "/Users/test/Documents/file.jpg", Some("jpg"), false);
        index.add_file(1, "/Users/test/Music/song.mp3", Some("mp3"), false);

        // Add files in Trash
        index.add_file(2, "/Users/test/.Trash/deleted.jpg", Some("jpg"), false);
        index.add_file(
            3,
            "/Users/test/.Trash/subfolder/another.png",
            Some("png"),
            false,
        );

        // Check trash detection
        assert!(!index.is_in_trash(0));
        assert!(!index.is_in_trash(1));
        assert!(index.is_in_trash(2));
        assert!(index.is_in_trash(3));

        // Check trash count
        assert_eq!(index.trash_count(), 2);

        // Check that trash bitmap works for filtering
        let all_images = index.get_indices_limited(FileTypeCategory::Images, 100);
        assert_eq!(all_images.len(), 3); // 0, 2, 3

        // Simulate filtering by subtracting trash bitmap
        let images_bitmap = index.get_indices(FileTypeCategory::Images).unwrap();
        let filtered = images_bitmap - index.trash_bitmap();
        let non_trash_images: Vec<_> = filtered.iter().collect();
        assert_eq!(non_trash_images, vec![0]); // Only the non-trash image

        // Remove a trash file
        index.remove_file(2);
        assert_eq!(index.trash_count(), 1);
        assert!(!index.is_in_trash(2));
    }

    #[test]
    fn test_serialization() {
        let mut index = TypeIndex::new();
        index.add_file(0, "/path/image.jpg", Some("jpg"), false);
        index.add_file(1, "/path/video.mp4", Some("mp4"), false);
        index.add_file(2, "/path/dir", None, true);

        // Serialize
        let serialized = postcard::to_allocvec(&index).unwrap();

        // Deserialize
        let mut deserialized: TypeIndex = postcard::from_bytes(&serialized).unwrap();
        deserialized.ensure_extension_map();

        assert_eq!(deserialized.count(FileTypeCategory::Images), 1);
        assert_eq!(deserialized.count(FileTypeCategory::Videos), 1);
    }

    #[test]
    fn test_profile_loading() {
        // TypeIndex::new() should load from profile
        let index = TypeIndex::new();

        // Should have extensions mapped
        assert!(index.extension_count() > 0);

        // Should recognize common extensions from profile
        assert!(index.has_extension("jpg"));
        assert!(index.has_extension("png"));
        assert!(index.has_extension("mp4"));
        assert!(index.has_extension("rs"));
        assert!(index.has_extension("py"));
    }

    #[test]
    fn test_from_profile_explicit() {
        let profile = profiles::load_file_types().expect("should load profile");
        let index = TypeIndex::from_profile(profile);

        // Should have extensions mapped
        assert!(index.extension_count() > 0);

        // Verify some extensions from the profile
        assert!(index.has_extension("jpg"));
        assert!(index.has_extension("png"));
    }

    #[test]
    fn test_categories_for_extension() {
        let index = TypeIndex::new();

        // jpg should map to Images
        let jpg_cats = index.categories_for_extension("jpg");
        assert!(jpg_cats.is_some());
        assert!(jpg_cats.unwrap().contains(&FileTypeCategory::Images));

        // rs should map to Code and Rust
        let rs_cats = index.categories_for_extension("rs");
        assert!(rs_cats.is_some());
        let rs_cats = rs_cats.unwrap();
        assert!(
            rs_cats.contains(&FileTypeCategory::Code) || rs_cats.contains(&FileTypeCategory::Rust)
        );

        // Unknown extension should return None
        assert!(index.categories_for_extension("xyz123unknown").is_none());
    }

    #[test]
    fn test_extension_case_insensitive() {
        let index = TypeIndex::new();

        // Extensions should be case-insensitive
        assert!(index.has_extension("JPG"));
        assert!(index.has_extension("Jpg"));
        assert!(index.has_extension("jpg"));
    }

    #[test]
    fn test_extension_bitmap_lookup() {
        let mut index = TypeIndex::new();

        // Add files with various extensions
        index.add_file(0, "/path/to/app.log", Some("log"), false);
        index.add_file(1, "/path/to/error.log", Some("log"), false);
        index.add_file(2, "/path/to/debug.log", Some("log"), false);
        index.add_file(3, "/path/to/image.jpg", Some("jpg"), false);
        index.add_file(4, "/path/to/installer.dmg", Some("dmg"), false);

        // Query by extension using O(1) bitmap lookup
        let log_bitmap = index.get_by_extension("log");
        assert!(log_bitmap.is_some());
        let log_bitmap = log_bitmap.unwrap();
        assert_eq!(log_bitmap.len(), 3);
        assert!(log_bitmap.contains(0));
        assert!(log_bitmap.contains(1));
        assert!(log_bitmap.contains(2));

        // Query another extension
        let jpg_bitmap = index.get_by_extension("jpg");
        assert!(jpg_bitmap.is_some());
        assert_eq!(jpg_bitmap.unwrap().len(), 1);

        // Query non-existent extension
        let xyz_bitmap = index.get_by_extension("xyz");
        assert!(xyz_bitmap.is_none());

        // Test count_by_extension
        assert_eq!(index.count_by_extension("log"), 3);
        assert_eq!(index.count_by_extension("jpg"), 1);
        assert_eq!(index.count_by_extension("dmg"), 1);
        assert_eq!(index.count_by_extension("xyz"), 0);

        // Test get_extension_indices_limited
        let limited = index.get_extension_indices_limited("log", 2);
        assert_eq!(limited.len(), 2);

        // Test unique_extension_count
        assert_eq!(index.unique_extension_count(), 3); // log, jpg, dmg
    }

    #[test]
    fn test_extension_bitmap_case_insensitive() {
        let mut index = TypeIndex::new();

        // Add files with mixed case extensions
        index.add_file(0, "/path/to/FILE.LOG", Some("LOG"), false);
        index.add_file(1, "/path/to/file.Log", Some("Log"), false);
        index.add_file(2, "/path/to/file.log", Some("log"), false);

        // All should be in the same bitmap (lowercase normalized)
        let bitmap = index.get_by_extension("log");
        assert!(bitmap.is_some());
        assert_eq!(bitmap.unwrap().len(), 3);

        // Lookup should be case-insensitive
        assert!(index.get_by_extension("LOG").is_some());
        assert!(index.get_by_extension("Log").is_some());
        assert!(index.get_by_extension("log").is_some());
    }

    #[test]
    fn test_extension_bitmap_removal() {
        let mut index = TypeIndex::new();

        index.add_file(0, "/path/to/a.log", Some("log"), false);
        index.add_file(1, "/path/to/b.log", Some("log"), false);

        assert_eq!(index.count_by_extension("log"), 2);

        // Remove one file
        index.remove_file(0);
        assert_eq!(index.count_by_extension("log"), 1);

        // Remove the other
        index.remove_file(1);
        assert_eq!(index.count_by_extension("log"), 0);
    }

    #[test]
    fn test_path_component_bitmap_lookup() {
        let mut index = TypeIndex::new();

        // Add files in various directories
        index.add_file(0, "/project/node_modules/pkg/index.js", Some("js"), false);
        index.add_file(1, "/project/node_modules/other/lib.js", Some("js"), false);
        index.add_file(2, "/other/node_modules/pkg/main.js", Some("js"), false);
        index.add_file(3, "/project/src/app.rs", Some("rs"), false);
        index.add_file(4, "/project/target/debug/app", None, false);
        index.add_file(5, "/home/.Trash/old.txt", Some("txt"), false);

        // Query by path component using O(1) bitmap lookup
        let node_modules_bitmap = index.get_by_path_component("node_modules");
        assert!(node_modules_bitmap.is_some());
        let nm_bitmap = node_modules_bitmap.unwrap();
        assert_eq!(nm_bitmap.len(), 3);
        assert!(nm_bitmap.contains(0));
        assert!(nm_bitmap.contains(1));
        assert!(nm_bitmap.contains(2));

        // Query target directory
        let target_bitmap = index.get_by_path_component("target");
        assert!(target_bitmap.is_some());
        assert_eq!(target_bitmap.unwrap().len(), 1);
        assert!(target_bitmap.unwrap().contains(4));

        // Query .Trash
        let trash_component = index.get_by_path_component(".Trash");
        assert!(trash_component.is_some());
        assert_eq!(trash_component.unwrap().len(), 1);

        // Query non-existent component
        let xyz_bitmap = index.get_by_path_component("xyz_nonexistent");
        assert!(xyz_bitmap.is_none());

        // Test count_by_path_component (only allowlisted components are indexed)
        assert_eq!(index.count_by_path_component("node_modules"), 3);
        assert_eq!(index.count_by_path_component("target"), 1);
        // "project" is NOT in the allowlist, so it's not indexed
        assert_eq!(index.count_by_path_component("project"), 0);
        assert_eq!(index.count_by_path_component("xyz"), 0);
    }

    #[test]
    fn test_path_component_case_insensitive() {
        let mut index = TypeIndex::new();

        index.add_file(0, "/Project/Node_Modules/pkg.js", Some("js"), false);
        index.add_file(1, "/project/node_modules/lib.js", Some("js"), false);
        index.add_file(2, "/PROJECT/NODE_MODULES/app.js", Some("js"), false);

        // All should be in the same bitmap (lowercase normalized)
        let bitmap = index.get_by_path_component("node_modules");
        assert!(bitmap.is_some());
        assert_eq!(bitmap.unwrap().len(), 3);

        // Lookup should be case-insensitive
        assert!(index.get_by_path_component("NODE_MODULES").is_some());
        assert!(index.get_by_path_component("Node_Modules").is_some());
        assert!(index.get_by_path_component("node_modules").is_some());
    }

    #[test]
    fn test_path_component_removal() {
        let mut index = TypeIndex::new();

        index.add_file(0, "/project/node_modules/a.js", Some("js"), false);
        index.add_file(1, "/project/node_modules/b.js", Some("js"), false);

        assert_eq!(index.count_by_path_component("node_modules"), 2);

        // Remove one file
        index.remove_file(0);
        assert_eq!(index.count_by_path_component("node_modules"), 1);

        // Remove the other
        index.remove_file(1);
        assert_eq!(index.count_by_path_component("node_modules"), 0);
    }
}
