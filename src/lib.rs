//! # zero - Resilient File Synchronization & Deduplication
//!
//! A fast, parallel file sync tool with checksum verification, resumable transfers,
//! and duplicate detection. Built for reliability on flaky USB drives and large backups.
//!
//! ## Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                              CLI Layer                                   │
//! │  cli/        - Command parsing (clap)                                   │
//! │  output/     - Human/JSON response formatting                           │
//! │  progress/   - Real-time progress display & throughput tracking         │
//! └─────────────────────────────────────────────────────────────────────────┘
//!                                    │
//!                                    ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                           Orchestration Layer                            │
//! │  sync/       - High-level sync workflow (scan → diff → transfer)        │
//! │  automation/ - Event-driven execution (USB mount triggers, watchers)    │
//! │  templates/  - Predefined backup sources (Mac Essentials, Developer)    │
//! └─────────────────────────────────────────────────────────────────────────┘
//!                                    │
//!                                    ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                           Core Operations                                │
//! │  scanner/    - Parallel directory tree walking (jwalk)                  │
//! │  hasher/     - File hashing (blake3, xxh3) with streaming support       │
//! │  differ/     - Tree comparison → add/modify/delete/unchanged lists      │
//! │  transfer/   - File copy with resume support (fast copy + chunked)      │
//! │  dedup/      - Duplicate detection by size → hash grouping              │
//! └─────────────────────────────────────────────────────────────────────────┘
//!                                    │
//!                                    ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                           System Layer                                   │
//! │  cache/      - SQLite persistence (control.db + per-storage indexes)    │
//! │  disk/       - Hardware info (USB speed, serial, volume capacity)       │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Module Details
//!
//! ### Core Operations
//!
//! | Module | Purpose | Key Types |
//! |--------|---------|-----------|
//! | [`scanner`] | Parallel directory tree scanning using jwalk. Collects file metadata (path, size, mtime) without reading file contents. Foundation for all other operations. | `FileEntry`, `ScanOptions`, `scan_collect()` |
//! | [`hasher`] | Compute cryptographic hashes of file contents. Supports blake3 (fast, secure) and xxh3 (fastest, non-crypto). Streaming API for large files. | `HashAlgorithm`, `HashResult`, `hash_file()` |
//! | [`differ`] | Compare two directory trees and categorize files as Added/Modified/Deleted/Unchanged. Uses cached checksums when available, computes on-demand otherwise. | `DiffResult`, `DiffEntry`, `DiffStatus`, `diff()` |
//! | [`transfer`] | Copy files from source to destination. Two modes: fast (`fs::copy` with APFS clonefile) and chunked (4MB chunks, resumable, verified). Large files (>100MB) auto-use chunked. | `copy_file()`, `copy_chunked_with_resume()` |
//! | [`dedup`] | Find duplicate files by grouping on size first (fast filter), then hashing only size-matched files. Returns groups of identical files. | `DedupResult`, `DuplicateGroup`, `find_duplicates()` |
//!
//! ### Orchestration
//!
//! | Module | Purpose | Key Types |
//! |--------|---------|-----------|
//! | [`sync`] | High-level sync workflow: scan source → scan dest → diff → transfer changes. Handles progress reporting, parallel copying, and error aggregation. | `SyncJob`, `SyncOptions`, `SyncResult` |
//! | [`automation`] | Execute syncs in response to events (USB mount, file changes). Manages run history, prevents duplicate runs, tracks progress. | `Executor`, `AutomationEvent` |
//! | [`templates`] | Predefined backup source configurations. "Mac Essentials" = Documents/Desktop/etc. "macOS Developer" = + code/dotfiles/SSH. | `Template`, `all_templates()`, `get_template()` |
//!
//! ### System
//!
//! | Module | Purpose | Key Types |
//! |--------|---------|-----------|
//! | [`cache`] | SQLite persistence layer. `control.db` stores automations, runs, settings. `storages/*.db` store per-device file checksums. Split architecture for UI/daemon separation. | `ControlDb`, `StorageDb`, `CacheManager` |
//! | [`disk`] | Hardware and volume information. USB device details (speed, vendor, serial), volume capacity/usage. Used for drive identification and transfer speed estimation. | `DiskInfo`, `VolumeInfo`, `UsbDeviceInfo` |
//!
//! ### CLI
//!
//! | Module | Purpose | Key Types |
//! |--------|---------|-----------|
//! | [`cli`] | Command-line argument parsing with clap. Defines all subcommands (scan, sync, diff, dupes, etc.) and their options. | `Cli`, `Commands` |
//! | [`output`] | Response formatting for CLI output. Human mode = pretty terminal output. JSON mode = machine-readable for Swift GUI integration. | `Outputter`, `OutputMode`, `Response<T>` |
//! | [`progress`] | Real-time progress tracking and display. Thread-safe atomic counters, throughput calculation, ETA estimation, terminal rendering. | `AtomicProgress`, `ProgressDisplay` |
//!
//! ## Data Flow Example: `zero sync /source /dest`
//!
//! ```text
//! 1. scanner::scan_collect(/source) → Vec<FileEntry>
//! 2. scanner::scan_collect(/dest)   → Vec<FileEntry>
//! 3. cache::lookup checksums for both (if available)
//! 4. differ::diff(source, dest)     → DiffResult { added, modified, deleted, unchanged }
//! 5. For each added/modified file:
//!    - transfer::copy_file() for small files (<100MB)
//!    - transfer::copy_chunked_with_resume() for large files
//! 6. cache::store new checksums
//! 7. Return SyncResult with stats
//! ```
//!
//! ## Future Modules (Planned)
//!
//! | Module | Purpose | Phase |
//! |--------|---------|-------|
//! | `search` | Local file search by name, path, hash, content. Private Spotlight replacement. | Phase 4 |
//! | `send` | E2E encrypted file transfer (send.to). Zero-knowledge relay. | Phase 2 |
//! | `cloud` | Remote storage backends (S3, R2, B2, SFTP). | Phase 3 |
//! | `security` | File integrity monitoring, app inventory, system baseline. | Phase 6 |

/// The version of the zero crate, for use by dependent crates like zero-ui.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod apps;
pub mod automation;
pub mod cache;
pub mod cleanup;
pub mod code;
pub mod dedup;
pub mod delete;
pub mod differ;
pub mod dirs;
pub mod disk;
pub mod hasher;
pub mod index;
pub mod logging;
pub mod output;
pub mod profiles;
pub mod progress;
pub mod scanner;
pub mod service;
pub mod storage;
pub mod sync;
pub mod telemetry;
pub mod templates;
pub mod todo;
pub mod transfer;
pub mod updater;
pub mod util;

/// Re-export commonly used types for convenient access
pub mod prelude {
    // Automation
    pub use crate::automation::{AutomationEvent, Executor, ExecutorConfig};

    // Cache/Database
    pub use crate::cache::{
        CacheDb, CacheEntry, CacheManager, CachedFileEntry, ChecksumBatch, Storage,
    };

    // Disk/Hardware
    pub use crate::disk::{DiskInfo, UsbDeviceInfo, UsbSpeed, VolumeInfo};

    // Scanner
    pub use crate::scanner::{
        CrawlProgress, FileEntry, ScanError, ScanOptions, scan, scan_collect, scan_with_progress,
    };

    // Search Index
    pub use crate::index::{
        FileNode, IndexManager, RootStats, SearchIndex, SearchOptions, SearchResult,
        SharedIndexManager, hash_path,
    };

    // Hasher
    pub use crate::hasher::{HashAlgorithm, HashResult, hash_file};

    // Differ
    pub use crate::differ::{DiffEntry, DiffResult, DiffStatus, diff};

    // Transfer
    pub use crate::transfer::{
        CopyError, CopyOptions, copy_chunked_with_resume, copy_file, copy_file_with_hash,
    };

    // Sync
    pub use crate::sync::{SyncJob, SyncOptions, SyncProgress, SyncResult};

    // Dedup
    pub use crate::dedup::{DedupOptions, DedupResult, DuplicateGroup, find_duplicates};

    // Cleanup (profile-based)
    pub use crate::cleanup::{
        DevGarbageItem, DevScanError, DevScanProgress, DevScanResult, GroupSummary,
        ProfileCleanupItem, ProfileCleanupQuery, ProfileCleanupResult, ProfileCleanupSummary,
        execute_category_cleanup, execute_full_cleanup_scan, execute_group_cleanup,
        scan_dev_garbage,
    };

    // Profiles
    pub use crate::profiles::{
        AggregateMode, CleanupGroup, MergedCleanupProfile, ResolvedCleanupCategory, load_cleanup,
        load_file_types,
    };

    // Delete
    pub use crate::delete::{
        DeleteError, DeleteOptions, DeleteResult, PreviewItem, delete_path, delete_paths,
        preview_delete,
    };

    // Templates
    pub use crate::templates::{ResolvedTemplate, Template, all_templates, get_template};

    // Progress
    pub use crate::progress::{AtomicProgress, ProgressDisplay};

    // Output
    pub use crate::output::{OutputMode, Outputter};

    // Storage backends
    pub use crate::storage::{LocalStorage, StorageBackend, StorageError, StorageMetadata};

    // Todo
    pub use crate::todo::{
        DEFAULT_LIST, ListCounts, StatusCounts, Task, TaskFilter, TaskStatus, TodoError, TodoFile,
        TodoManager,
    };

    // Code Index
    pub use crate::code::{
        CodeElement, CodeIndex, CodeIndexError, CodeProject, CodeSearchOpts, CodeSearchResult,
        ElementKind, Language, ProjectOverview, ProjectSummary, Visibility,
    };
}

// CLI module is internal to the binary, not part of the library API
// It's defined in main.rs scope, not exported here
