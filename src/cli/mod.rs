//! CLI argument parsing and command definitions
//!
//! The CLI is designed to be intuitive:
//! - `zero source dest` - default sync behavior (no subcommand needed)
//! - `zero ls path` - list files (local or cloud)
//! - `zero cp src dest` - copy files (local or cloud)

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

pub mod commands;

#[derive(Parser)]
#[command(name = "zero")]
#[command(author, version, about = "A resilient file synchronization and deduplication tool", long_about = None)]
pub struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Output in JSON format (for programmatic use)
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Shell types for completions
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ShellType {
    Bash,
    Zsh,
    Fish,
    Elvish,
    PowerShell,
}

impl From<ShellType> for clap_complete::Shell {
    fn from(shell: ShellType) -> Self {
        match shell {
            ShellType::Bash => clap_complete::Shell::Bash,
            ShellType::Zsh => clap_complete::Shell::Zsh,
            ShellType::Fish => clap_complete::Shell::Fish,
            ShellType::Elvish => clap_complete::Shell::Elvish,
            ShellType::PowerShell => clap_complete::Shell::PowerShell,
        }
    }
}

#[derive(Subcommand)]
pub enum Commands {
    // =========================================================================
    // Primary commands (most common operations)
    // =========================================================================
    /// Sync source to destination (only transfer changes) [default command]
    ///
    /// Supports both local paths and cloud storage URLs:
    ///   zero sync ./local/dir /backup/dir
    ///   zero sync ./local/dir s3://bucket/backup
    ///   zero sync s3://bucket/data ./local/restore
    ///
    /// Tip: You can also just use `zero source dest` without the 'sync' subcommand.
    Sync {
        /// Source (local path or cloud URL like s3://bucket/path)
        source: String,

        /// Destination (local path or cloud URL like s3://bucket/path)
        dest: String,

        /// Mirror mode: make destination identical to source by deleting files that don't exist in source
        #[arg(long)]
        mirror: bool,

        /// Full verification: compare by checksum, hash while copying, cache results
        /// Use this when you want to ensure source and destination are identical
        #[arg(long)]
        verify: bool,

        /// Hash files while copying and cache results (but still compare by mtime+size)
        /// Power user flag: faster than --verify but caches hashes for future verification
        #[arg(long)]
        hash_on_copy: bool,

        /// Preserve directory permissions (file permissions are always preserved)
        #[arg(short = 'p', long)]
        preserve_permissions: bool,

        /// Exclude files/directories matching pattern (can be specified multiple times)
        /// Supports wildcards: "*.tmp", "node_modules", "._*"
        #[arg(short = 'x', long = "exclude", value_name = "PATTERN")]
        exclude: Vec<String>,

        /// Dry run: show what would be done without actually copying/deleting
        #[arg(long, alias = "trial")]
        dry_run: bool,

        /// Disable chunked transfer (chunked is enabled by default for resume capability)
        #[arg(long)]
        no_chunked: bool,

        /// Minimum file size to use chunked transfer (default: 100MB)
        /// Files above this threshold are copied in chunks that can be resumed if interrupted
        #[arg(long, default_value = "104857600", value_name = "BYTES")]
        chunk_threshold: u64,
    },

    /// List files in a directory (local or cloud)
    ///
    /// Examples:
    ///   zero ls ./local/dir
    ///   zero ls s3://bucket/path/ -l
    ///   zero list /path --recursive
    #[command(alias = "list")]
    Ls {
        /// Path to list (local path or cloud URL)
        path: String,

        /// List recursively
        #[arg(short = 'r', long)]
        recursive: bool,

        /// Long format (show sizes)
        #[arg(short = 'l', long)]
        long: bool,
    },

    /// Copy files (local or cloud)
    ///
    /// Examples:
    ///   zero cp ./file.txt /backup/file.txt
    ///   zero cp ./local/dir s3://bucket/backup/ -r
    ///   zero copy s3://bucket/file.txt ./local/
    #[command(alias = "copy")]
    Cp {
        /// Source path (local path or cloud URL)
        source: String,

        /// Destination path (local path or cloud URL)
        dest: String,

        /// Copy directories recursively
        #[arg(short = 'r', long)]
        recursive: bool,
    },

    /// Download files from cloud storage to current directory
    ///
    /// Quick way to download without specifying destination:
    ///   zero get s3://bucket/file.txt           # → ./file.txt
    ///   zero get s3://bucket/backup/            # → ./backup/
    ///   zero get b2://bucket/data/ -o ./dest/   # → ./dest/data/
    #[command(alias = "download")]
    Get {
        /// Source URL (cloud storage path)
        url: String,

        /// Output directory (default: current directory)
        #[arg(short = 'o', long = "output", default_value = ".")]
        output: String,

        /// Download directories recursively
        #[arg(short = 'r', long)]
        recursive: bool,
    },

    // =========================================================================
    // Verification and comparison
    // =========================================================================
    /// Compare two directories and show differences
    ///
    /// By default compares by metadata (size + mtime). Use --checksum for
    /// cryptographic verification with cached results.
    #[command(visible_alias = "verify")]
    Diff {
        /// Source directory
        source: PathBuf,

        /// Destination directory
        dest: PathBuf,

        /// Use checksums instead of mtime+size (enables hash cache for speed)
        #[arg(long)]
        checksum: bool,

        /// Force rehash all files, ignore cache (implies --checksum)
        #[arg(long)]
        full: bool,

        /// Also check file/directory permissions match
        #[arg(long)]
        check_permissions: bool,

        /// Show identical files too
        #[arg(long)]
        show_identical: bool,

        /// Maximum depth to traverse
        #[arg(short, long)]
        max_depth: Option<usize>,
    },

    // =========================================================================
    // File operations
    // =========================================================================
    /// Delete files and directories (moves to Trash by default)
    ///
    /// Examples:
    ///   zero delete file.txt                    # Move to Trash (safe)
    ///   zero delete dir1 dir2                   # Move multiple to Trash
    ///   zero delete file.txt --permanent        # Permanently delete (dangerous)
    ///   zero delete *.log --dry-run             # Preview what would be deleted
    ///   zero delete folder/ -f                  # Skip confirmation
    #[command(visible_alias = "rm")]
    Delete {
        /// Paths to delete (files or directories)
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Permanently delete instead of moving to Trash (DANGEROUS)
        #[arg(long, short = 'p')]
        permanent: bool,

        /// Skip confirmation prompt
        #[arg(long, short = 'f')]
        force: bool,

        /// Show what would be deleted without actually deleting
        #[arg(long, short = 'n')]
        dry_run: bool,
    },

    /// Find duplicate files in a directory
    ///
    /// Examples:
    ///   zero dupes ~/Photos                      # All duplicates
    ///   zero dupes ~/Photos "vacation"           # Duplicates matching "vacation"
    ///   zero dupes ~/Photos --type images        # Duplicate images only
    ///   zero dupes ~/Photos "2024" --type images # Combine query + type
    ///   zero search "old" | zero dupes         # From pipe (auto-detected)
    ///   zero dupes --from-file paths.txt         # From file
    Dupes {
        /// Path to scan for duplicates (optional if reading from stdin/file)
        path: Option<PathBuf>,

        /// Search query to filter files before dedup (uses search index)
        query: Option<String>,

        /// Filter by file type (images, videos, audio, documents, code, archives, config)
        #[arg(short = 't', long = "type")]
        type_filter: Option<zero::index::FileTypeCategory>,

        /// Read paths from file instead of scanning directory
        #[arg(long = "from-file", value_name = "FILE")]
        from_file: Option<PathBuf>,

        /// Delete duplicates (keeps shortest path)
        #[arg(long)]
        delete: bool,

        /// Verify checksums before deleting (slower but safer)
        #[arg(long)]
        verify: bool,

        /// Maximum depth to traverse
        #[arg(short, long)]
        max_depth: Option<usize>,

        /// Minimum file size in bytes (skip smaller files)
        #[arg(long, default_value = "1")]
        min_size: u64,
    },

    /// Scan a directory and show file statistics
    Scan {
        /// Path to scan
        path: PathBuf,

        /// Maximum depth to traverse
        #[arg(short, long)]
        max_depth: Option<usize>,

        /// Follow symbolic links
        #[arg(long)]
        follow_symlinks: bool,

        /// Run benchmark with N iterations
        #[arg(long, value_name = "N")]
        benchmark: Option<usize>,

        /// Skip hidden files and directories (default: include them)
        #[arg(long)]
        skip_hidden: bool,
    },

    /// Build and manage the file index (checksums database)
    ///
    /// The index stores file checksums for fast verification. Once indexed,
    /// `zero verify` becomes nearly instant for unchanged files.
    Index {
        /// Path to index
        path: PathBuf,

        /// Hash algorithm to use (xxh3 or blake3)
        #[arg(short, long, default_value = "xxh3")]
        algorithm: zero::hasher::HashAlgorithm,

        /// Maximum depth to traverse
        #[arg(short, long)]
        max_depth: Option<usize>,

        /// Show index statistics (coverage, file count, last scan)
        #[arg(long)]
        stats: bool,

        /// List indexed files with their hashes
        #[arg(long)]
        list: bool,

        /// Display hashes without storing in index (like `sha256sum`)
        #[arg(long)]
        no_store: bool,

        /// Remove entries for files that no longer exist
        #[arg(long)]
        prune: bool,

        /// Run benchmark: hash N files and measure index performance
        #[arg(long, value_name = "N")]
        benchmark: Option<usize>,
    },

    // =========================================================================
    // System information
    // =========================================================================
    /// List connected drives/volumes
    Drives,

    /// Show detailed disk/volume info for a path
    Disk {
        /// Path to volume or any path on the volume (e.g., /Volumes/Ghost or /Volumes/Ghost/somefile.txt)
        path: PathBuf,
    },

    /// Securely erase a disk/volume (DESTRUCTIVE - cannot be undone!)
    ///
    /// Overwrites all data with random patterns to prevent recovery.
    /// Only works on external/removable drives - refuses to erase system disks.
    ///
    /// Security levels:
    ///   quick  - Single zero pass (fast, not secure)
    ///   normal - Double random pass (recommended)
    ///   strong - VSITR 7-pass (maximum security, slow)
    #[command(visible_alias = "wipe")]
    Erase {
        /// Path to volume to erase (e.g., /Volumes/MyUSB)
        path: PathBuf,

        /// Security level: quick, normal, strong
        #[arg(long, short, default_value = "normal")]
        level: zero::disk::erase::EraseLevel,

        /// Verification mode: no, last, all
        #[arg(long, default_value = "last")]
        verify: zero::disk::erase::Verify,

        /// Skip confirmation prompt (DANGEROUS)
        #[arg(long, short = 'y')]
        force: bool,

        /// Show what would be done without actually erasing
        #[arg(long, short = 'n')]
        dry_run: bool,
    },

    // =========================================================================
    // Automation and monitoring
    // =========================================================================
    /// Manage automations (sync rules)
    Automation {
        #[command(subcommand)]
        auto_cmd: AutomationCommands,
    },

    /// List available backup templates
    Templates {
        /// Show detailed information for a specific template
        #[arg(long)]
        show: Option<String>,

        /// Resolve template and show which paths exist on this system
        #[arg(long)]
        resolve: bool,
    },

    /// Watch for file system or USB mount events (dev/testing tool)
    ///
    /// This command is primarily for development and testing of the watcher
    /// functionality that will be used by the GUI app for auto-sync triggers.
    Watch {
        #[command(subcommand)]
        watch_cmd: WatchCommands,
    },

    /// Generate shell completions
    ///
    /// Examples:
    ///   zero completions fish              # Print to stdout
    ///   zero completions fish > ~/.config/fish/completions/zero.fish
    ///   zero completions --install         # Auto-detect shell & install
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Option<ShellType>,

        /// Auto-detect shell and install completions
        #[arg(short, long)]
        install: bool,
    },

    // =========================================================================
    // Search (private Spotlight replacement)
    // =========================================================================
    /// Fast file search (private Spotlight replacement)
    ///
    /// Build an index first, then search:
    ///   zero search --index ~/Documents    # Build index
    ///   zero search report                 # Search for "report"
    ///   zero search "*.pdf" --type images  # Search with type filter
    ///   zero search --types                # List available type filters
    Search {
        /// Search query (filename substring)
        query: Option<String>,

        /// Limit search to this directory (subdirectory of indexed path)
        #[arg(long, value_name = "PATH")]
        r#in: Option<PathBuf>,

        /// Build/rebuild index from this path
        #[arg(long)]
        index: Option<PathBuf>,

        /// Custom path to index file (default: ~/.cache/zero/search_index.bin)
        #[arg(long)]
        cache: Option<PathBuf>,

        /// Maximum results to return (0 = unlimited)
        #[arg(short = 'n', long, default_value = "50")]
        limit: usize,

        /// Only show count of matching files, don't list them
        #[arg(short = 'c', long)]
        count: bool,

        /// Only search for files (exclude directories)
        #[arg(long)]
        files_only: bool,

        /// Only search for directories (exclude files)
        #[arg(long)]
        dirs_only: bool,

        /// Filter by file extension (e.g., "pdf", "rs")
        #[arg(short = 'e', long = "ext")]
        extension: Option<String>,

        /// Filter by file type (images, videos, audio, documents, code, archives, config)
        #[arg(short = 't', long = "type")]
        type_filter: Option<zero::index::FileTypeCategory>,

        /// List available file type filters
        #[arg(long)]
        types: bool,

        /// Show N most recently modified files (uses mtime index for fast lookup)
        #[arg(short = 'r', long)]
        recent: Option<usize>,

        /// Watch directory for changes and update index in real-time
        #[arg(long)]
        watch: bool,
    },

    // =========================================================================
    // Service mode (for XPC daemon integration)
    // =========================================================================
    /// Run as a background service (JSON-RPC mode for XPC daemon)
    ///
    /// This mode is used by the macOS app's XPC service to communicate
    /// with the Rust core. It reads JSON-RPC requests from stdin and
    /// writes responses/events to stdout.
    #[command(name = "service")]
    Service {
        /// Enable verbose logging (DEBUG level instead of INFO)
        #[arg(long, short)]
        verbose: bool,
    },

    /// Manage todo lists and tasks
    Todo {
        #[command(subcommand)]
        todo_cmd: Option<commands::todo::TodoCommands>,
    },
}

/// Print the intro/help message when no command is given
pub fn print_intro() {
    println!(
        r#"
  zero — fast, resilient file sync

  Commands:
    automation    Manage sync automations
    completions   Generate shell completions
    cp            Copy files (local or cloud)
    delete        Delete files (moves to Trash by default)
    diff          Compare two directories
    disk          Show disk/volume info
    drives        List connected drives
    dupes         Find duplicate files
    erase         Securely erase a disk (destructive)
    get           Download from cloud storage
    index         Build/manage file checksum index
    ls            List files (local or cloud)
    scan          Scan directory and show statistics
    search        Fast file search (private Spotlight replacement)
    sync          Sync source to destination [default]
    todo          Manage tasks and todo lists
    watch         Watch for filesystem/USB events

  Examples:
    zero ./photos /backup/photos         Sync files
    zero ./data s3://bucket/backup       Sync to cloud
    zero get s3://bucket/file.txt        Download from cloud
    zero diff ./src /backup/src          Compare directories
    zero diff ./src /backup --checksum   Verify with checksums

  Tab completion:
    zero completions --install

  Run 'zero help' or 'zero --help' for full details.
"#
    );
}

/// Subcommands for the watch command (dev/testing)
#[derive(Subcommand)]
pub enum WatchCommands {
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

    /// Watch for USB mount/unmount events
    Usb {
        /// Only report events for volumes matching these names
        #[arg(long)]
        filter: Vec<String>,
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

/// Subcommands for automation management
#[derive(Subcommand)]
pub enum AutomationCommands {
    /// List all automations
    List,

    /// Show details of an automation
    Show {
        /// Automation ID
        id: i64,
    },

    /// Create a new automation
    Create {
        /// Name for the automation
        #[arg(long)]
        name: String,

        /// Use a template for source paths (e.g., "mac-essentials", "macos-developer")
        #[arg(long)]
        template: Option<String>,

        /// Source paths to sync (can be specified multiple times, or use --template)
        #[arg(long = "source")]
        sources: Vec<PathBuf>,

        /// Destination subpaths (one per source, or empty for root)
        #[arg(long = "dest")]
        dests: Vec<String>,

        /// USB device serial to trigger on
        #[arg(long)]
        device_serial: Option<String>,

        /// Volume name for display
        #[arg(long)]
        volume_name: Option<String>,

        /// Direct destination path (for non-USB)
        #[arg(long)]
        dest_path: Option<PathBuf>,

        /// Trigger on USB mount
        #[arg(long, default_value = "true")]
        on_mount: bool,

        /// Trigger on file changes
        #[arg(long)]
        on_change: bool,

        /// Verify checksums after sync
        #[arg(long, default_value = "true")]
        verify: bool,

        /// Delete files not in source
        #[arg(long)]
        delete_orphans: bool,

        /// Debounce delay for file change triggers (milliseconds)
        #[arg(long, default_value = "5000")]
        debounce_ms: u64,
    },

    /// Delete an automation
    Delete {
        /// Automation ID
        id: i64,
    },

    /// Run an automation manually
    Run {
        /// Automation ID
        id: i64,
    },

    /// Show run history for an automation
    History {
        /// Automation ID
        id: i64,

        /// Number of runs to show
        #[arg(long, default_value = "10")]
        limit: i64,
    },

    /// Start daemon mode - watch for events and run automations
    Daemon,
}
