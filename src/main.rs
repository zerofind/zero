//! zero - A resilient file synchronization and deduplication tool

use std::env;
use std::panic;
use std::path::Path;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::{CommandFactory, Parser};
use cli::Cli;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

use zero::output::{OutputMode, Outputter};

mod cli;
use cli::{Commands, ShellType, print_intro};

/// Global flag to track if we're in a panic state
static PANICKING: AtomicBool = AtomicBool::new(false);

/// Set up a panic handler that ensures clean process exit
/// This prevents zombie rayon threads from hanging around after a panic/stack overflow
fn setup_panic_handler() {
    // Store the default panic hook
    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        // Mark that we're panicking to prevent recursive issues
        if PANICKING.swap(true, Ordering::SeqCst) {
            // Already panicking, just abort immediately
            eprintln!("\n⚠ Double panic detected, aborting immediately");
            process::abort();
        }

        // Print panic info using the default handler
        default_hook(panic_info);

        // Print cleanup message
        eprintln!("\n⚠ zero encountered a fatal error and must exit.");
        eprintln!("  This ensures no zombie processes are left running.");

        // Force immediate process exit to clean up all threads (including rayon pool)
        // Using exit(1) instead of abort() for cleaner shutdown
        // This is the "early exit strategy" - when things go wrong, exit fast and clean
        process::exit(1);
    }));
}

/// Configure the global rayon thread pool with proper panic handling
fn setup_rayon_global_pool() {
    // Configure the global rayon thread pool
    // This sets up proper defaults and panic handling for any code using par_iter() etc.
    let result = rayon::ThreadPoolBuilder::new()
        .stack_size(8 * 1024 * 1024) // 8MB stack per thread to prevent stack overflow
        .panic_handler(|panic_info| {
            // Log the panic from a rayon thread
            eprintln!("\n⚠ Panic in rayon worker thread: {:?}", panic_info);
            // The global panic handler will handle process exit
        })
        .build_global();

    if let Err(e) = result {
        // Global pool already initialized (e.g., in tests), that's fine
        tracing::debug!("Rayon global pool already initialized: {}", e);
    }
}

/// Check if an argument looks like a path (local or cloud URL)
/// Used to detect default sync command usage: `zero source dest`
fn looks_like_path(s: &str) -> bool {
    // Cloud URLs
    if s.contains("://") {
        return true;
    }
    // Absolute paths
    if s.starts_with('/') || s.starts_with('~') {
        return true;
    }
    // Relative paths
    if s.starts_with("./") || s.starts_with("../") {
        return true;
    }
    // Check if it's an existing path on disk
    if Path::new(s).exists() {
        return true;
    }
    // Could be a relative path that doesn't exist yet (destination)
    // If it doesn't start with '-' and contains a path separator or looks like a filename
    if !s.starts_with('-') && (s.contains('/') || s.contains('.') || !s.contains('-')) {
        // Heuristic: if it looks like a path segment (not a flag, not a known subcommand)
        let known_commands = [
            "sync",
            "ls",
            "list",
            "cp",
            "copy",
            "verify",
            "diff",
            "dupes",
            "scan",
            "transfer",
            "index",
            "drives",
            "disk",
            "erase",
            "wipe",
            "automation",
            "templates",
            "watch",
            "status",
            "cloud",
            "help",
            "--help",
            "-h",
            "--version",
            "-V",
        ];
        if !known_commands.contains(&s) {
            return true;
        }
    }
    false
}

/// Pre-process arguments to support default sync command
/// Transforms `zero source dest [options]` into `zero sync source dest [options]`
fn preprocess_args() -> Vec<String> {
    let args: Vec<String> = env::args().collect();

    // Need at least: zero source dest
    if args.len() < 3 {
        return args;
    }

    // Check if the first argument after program name looks like a path
    // and is NOT a known subcommand
    let first_arg = &args[1];

    // Skip if it's a global flag
    if first_arg.starts_with('-') {
        return args;
    }

    // Skip if it's a known subcommand
    let known_commands = [
        "sync",
        "ls",
        "list",
        "cp",
        "copy",
        "get",
        "download",
        "verify",
        "diff",
        "delete",
        "rm",
        "dupes",
        "scan",
        "transfer",
        "index",
        "drives",
        "disk",
        "erase",
        "wipe",
        "automation",
        "templates",
        "watch",
        "status",
        "completions",
        "cloud",
        "search",
        "service",
        "todo",
        "help",
    ];

    if known_commands.contains(&first_arg.as_str()) {
        return args;
    }

    // Check if it looks like a path
    if looks_like_path(first_arg) {
        // Insert "sync" as the subcommand
        let mut new_args = vec![args[0].clone(), "sync".to_string()];
        new_args.extend(args[1..].iter().cloned());
        return new_args;
    }

    args
}

fn main() -> anyhow::Result<()> {
    // IMPORTANT: Set up panic handler FIRST before anything else
    // This ensures clean exit on panics, preventing zombie rayon threads
    setup_panic_handler();

    // Configure rayon with larger stacks and panic handling
    setup_rayon_global_pool();

    // Pre-process args to support default sync command: `zero source dest`
    let args = preprocess_args();
    let cli = Cli::parse_from(args);

    // Create output handler
    let output_mode = if cli.json {
        OutputMode::Json
    } else {
        OutputMode::Human
    };
    let out = Outputter::new(output_mode);

    // Set up logging (only in human mode, and only if verbose)
    if !cli.json {
        let filter = if cli.verbose {
            EnvFilter::new("zero=debug,info")
        } else {
            EnvFilter::new("warn")
        };

        FmtSubscriber::builder()
            .with_env_filter(filter)
            .with_target(false)
            .without_time()
            .init();
    }

    // Handle no command (show intro)
    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            print_intro();
            return Ok(());
        }
    };

    match command {
        // =====================================================================
        // Primary commands
        // =====================================================================
        Commands::Sync {
            source,
            dest,
            verify,
            hash_on_copy,
            preserve_permissions,
            exclude,
            dry_run,
            mirror,
            no_chunked,
            chunk_threshold,
        } => {
            // --verify implies checksum comparison AND hash-on-copy
            // --hash-on-copy only does hash-on-copy (mtime+size comparison)
            let use_checksum = verify;
            let do_hash_on_copy = verify || hash_on_copy;
            cli::commands::cmd_sync(
                &out,
                &cli::commands::CmdSyncOptions {
                    source: &source,
                    dest: &dest,
                    use_checksum,
                    delete_orphans: mirror,
                    hash_on_copy: do_hash_on_copy,
                    preserve_permissions,
                    exclude_patterns: &exclude,
                    dry_run,
                    chunked: !no_chunked, // chunked is default, flag disables it
                    chunk_threshold,
                },
            )?;
        }

        // Top-level ls command (works with local and cloud paths)
        Commands::Ls {
            path,
            recursive,
            long,
        } => {
            cli::commands::cmd_cloud_ls(&out, &path, recursive, long)?;
        }

        // Top-level cp command (works with local and cloud paths)
        Commands::Cp {
            source,
            dest,
            recursive,
        } => {
            cli::commands::cmd_cloud_cp(&out, &source, &dest, recursive)?;
        }

        // Top-level get command (quick download from cloud)
        Commands::Get {
            url,
            output,
            recursive,
        } => {
            cli::commands::cmd_get(&out, &url, &output, recursive)?;
        }

        // =====================================================================
        // Verification and comparison
        // =====================================================================
        Commands::Verify {
            source,
            dest,
            max_depth,
            quick,
            full,
            check_permissions,
        } => {
            cli::commands::cmd_verify(
                &out,
                &source,
                &dest,
                max_depth,
                quick,
                full,
                check_permissions,
            )?;
        }

        Commands::Diff {
            source,
            dest,
            checksum,
            show_identical,
            max_depth,
        } => {
            cli::commands::cmd_diff(&out, &source, &dest, checksum, show_identical, max_depth)?;
        }

        // =====================================================================
        // File operations
        // =====================================================================
        Commands::Delete {
            paths,
            permanent,
            force,
            dry_run,
        } => {
            let args = cli::commands::delete::DeleteArgs {
                paths,
                trash: !permanent, // Default is trash (safe), --permanent disables it
                recursive: true,   // Always recursive for directories
                force,
                dry_run,
            };
            cli::commands::cmd_delete(&out, &args)?;
        }

        Commands::Dupes {
            path,
            query,
            type_filter,
            from_file,
            delete,
            verify,
            max_depth,
            min_size,
        } => {
            let args = cli::commands::dupes::DupesArgs {
                path,
                query,
                type_filter,
                from_file,
                delete,
                verify,
                max_depth,
                min_size,
            };
            cli::commands::cmd_dupes(&out, &args)?;
        }

        Commands::Scan {
            path,
            max_depth,
            follow_symlinks,
            benchmark,
            skip_hidden,
        } => {
            if let Some(iterations) = benchmark {
                cli::commands::cmd_benchmark(
                    &out,
                    &path,
                    max_depth,
                    follow_symlinks,
                    iterations,
                    skip_hidden,
                )?;
            } else {
                cli::commands::cmd_scan(&out, &path, max_depth, follow_symlinks, skip_hidden)?;
            }
        }

        Commands::Transfer {
            source,
            dest,
            verify,
            max_depth,
            preserve_permissions,
        } => {
            cli::commands::cmd_transfer(
                &out,
                &source,
                &dest,
                verify,
                max_depth,
                preserve_permissions,
            )?;
        }

        Commands::Index {
            path,
            algorithm,
            max_depth,
            stats,
            list,
            no_store,
            prune,
            benchmark,
        } => {
            cli::commands::cmd_index(
                &out,
                &cli::commands::IndexOptions {
                    path: &path,
                    algorithm,
                    max_depth,
                    stats,
                    list,
                    no_store,
                    prune,
                    benchmark,
                },
            )?;
        }

        // =====================================================================
        // System information
        // =====================================================================
        Commands::Drives => {
            cli::commands::cmd_drives(&out)?;
        }

        Commands::Disk { path } => {
            cli::commands::cmd_disk(&out, &path)?;
        }

        Commands::Erase {
            path,
            level,
            verify,
            force,
            dry_run,
        } => {
            let args = cli::commands::erase::EraseArgs {
                path,
                level,
                verify,
                force,
                dry_run,
            };
            cli::commands::cmd_erase(&out, &args)?;
        }

        // =====================================================================
        // Automation and monitoring
        // =====================================================================
        Commands::Automation { auto_cmd } => {
            cli::commands::cmd_automation(&out, auto_cmd)?;
        }

        Commands::Templates { show, resolve } => {
            cli::commands::cmd_templates(&out, show, resolve)?;
        }

        Commands::Watch { watch_cmd } => {
            cli::commands::cmd_watch(&out, watch_cmd)?;
        }

        Commands::Status { job_id: _ } => {
            out.info("Status not yet implemented");
        }

        // =====================================================================
        // Search
        // =====================================================================
        Commands::Search {
            query,
            r#in: path_filter,
            index,
            cache,
            limit,
            count,
            files_only,
            dirs_only,
            extension,
            type_filter,
            types,
            recent,
            watch,
        } => {
            if types {
                cli::commands::cmd_search_types(&out)?;
            } else if watch {
                // Watch mode requires --index to specify what to watch
                if let Some(index_path) = index {
                    cli::commands::cmd_search_watch(&out, &index_path, cache.as_deref())?;
                } else {
                    out.error("Usage: zero search --watch --index <path>");
                    out.info("Specify the directory to watch with --index");
                }
            } else if let Some(index_path) = index {
                cli::commands::cmd_search_index(&out, &index_path, cache.as_deref())?;
            } else if query.is_some()
                || type_filter.is_some()
                || extension.is_some()
                || path_filter.is_some()
                || recent.is_some()
            {
                // Allow search with: query, type filter, extension filter, path, or recent (or combination)
                cli::commands::cmd_search(
                    &out,
                    &cli::commands::SearchOptions {
                        query: query.as_deref(),
                        path_filter: path_filter.as_deref(),
                        cache_path: cache.as_deref(),
                        limit,
                        count_only: count,
                        files_only,
                        dirs_only,
                        extension: extension.as_deref(),
                        type_filter,
                        recent,
                    },
                )?;
            } else {
                out.error("Usage: zero search <query> or zero search --type <type>");
                out.info("Run 'zero search --types' to see available type filters");
                out.info("Example: zero search --type images");
            }
        }

        // =====================================================================
        // Service mode (XPC daemon integration)
        // =====================================================================
        Commands::Service { verbose } => {
            // Service mode runs in JSON-RPC mode for XPC daemon
            // It doesn't use the normal outputter since it communicates via stdin/stdout
            zero::service::run_service(verbose)?;
        }

        // =====================================================================
        // Shell completions
        // =====================================================================
        Commands::Completions { shell, install } => {
            cmd_completions(shell, install)?;
        }

        // =====================================================================
        // Legacy commands (kept for backwards compatibility)
        // =====================================================================
        // =====================================================================
        // Todo management
        // =====================================================================
        Commands::Todo { todo_cmd } => {
            cli::commands::cmd_todo(&out, todo_cmd.as_ref())?;
        }

    }

    Ok(())
}

/// Generate or install shell completions
fn cmd_completions(shell: Option<ShellType>, install: bool) -> anyhow::Result<()> {
    if install {
        // Auto-detect shell and install
        let shell_name = env::var("SHELL").unwrap_or_default();
        let detected = if shell_name.contains("fish") {
            ShellType::Fish
        } else if shell_name.contains("zsh") {
            ShellType::Zsh
        } else if shell_name.contains("bash") {
            ShellType::Bash
        } else {
            anyhow::bail!(
                "Could not detect shell from $SHELL='{}'\n\
                 Specify shell explicitly: zero completions fish",
                shell_name
            );
        };

        let shell_type: clap_complete::Shell = detected.into();
        let (path, instructions) = get_completion_path(detected)?;

        // Generate completions
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        clap_complete::generate(shell_type, &mut cmd, "zero", &mut buf);

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write to file
        std::fs::write(&path, &buf)?;

        eprintln!("✓ Installed completions to {}", path.display());
        eprintln!("  {}", instructions);

        return Ok(());
    }

    // Generate to stdout
    let shell = shell.ok_or_else(|| {
        anyhow::anyhow!(
            "Specify a shell: zero completions fish\n\
             Or use --install to auto-detect and install"
        )
    })?;

    let shell_type: clap_complete::Shell = shell.into();
    let mut cmd = Cli::command();
    clap_complete::generate(shell_type, &mut cmd, "zero", &mut std::io::stdout());

    Ok(())
}

/// Get the completion file path and reload instructions for a shell
fn get_completion_path(shell: ShellType) -> anyhow::Result<(std::path::PathBuf, &'static str)> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;

    match shell {
        ShellType::Fish => {
            let path = home.join(".config/fish/completions/zero.fish");
            Ok((
                path,
                "Restart fish or run: source ~/.config/fish/completions/zero.fish",
            ))
        }
        ShellType::Zsh => {
            let path = home.join(".zfunc/_zero");
            Ok((
                path,
                "Add to .zshrc: fpath=(~/.zfunc $fpath); autoload -Uz compinit && compinit",
            ))
        }
        ShellType::Bash => {
            let path = home.join(".local/share/bash-completion/completions/zero");
            Ok((
                path,
                "Restart bash or run: source ~/.local/share/bash-completion/completions/zero",
            ))
        }
        ShellType::Elvish => {
            let path = home.join(".elvish/lib/completions/zero.elv");
            Ok((path, "Restart elvish to load completions"))
        }
        ShellType::PowerShell => {
            let path = home.join("Documents/PowerShell/Completions/zero.ps1");
            Ok((
                path,
                "Add to $PROFILE: . ~/Documents/PowerShell/Completions/zero.ps1",
            ))
        }
    }
}
