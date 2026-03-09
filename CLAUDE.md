# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Zero is a privacy-first Finder & Spotlight replacement built in Rust. It provides resumable file sync, instant local search, verified copies, and duplicate detection. All search is 100% local - nothing sent to Apple or cloud services.

## Code Navigation

- **`.compass`** - Index of all 1602 public symbols with doc comments. Check here first to find which file contains a symbol.
- **`grep`** - Use for finding implementations, call sites, or specific patterns.
- **Module structure** - See "Architecture" section below for module purposes.

## Commands

### Development

```bash
# Build
cargo build --release

# Run
cargo run -- <subcommand>
cargo run -- sync /source /dest
cargo run -- search --index ~/Documents

# Test
cargo test                           # Run all tests
cargo test <module>                  # Run specific module tests
cargo test -p zero-watcher           # Test watcher crate
cargo test --no-fail-fast            # Run all tests, don't stop on first failure

# Lint
cargo clippy
cargo clippy --all-targets --all-features

# Format
cargo fmt
```

### Install Locally

```bash
cargo install --path .
```

### Benchmark

```bash
./scripts/benchmark.sh                    # Run with generated test files
./scripts/benchmark.sh /path/to/folder    # Benchmark with existing folder
./scripts/benchmark.sh -r 5 ~/Documents   # 5 runs on Documents folder
```

Compares zero against `cp`, `rsync`, and `rclone`. Requires release build first.

## Coding Standards (Non-Negotiable)

### Structure

- **500 lines max** per file - split into logical sub-components if larger
- **One file, one job** - each module has a single, well-defined responsibility
- **Tests in `*_test.rs`** files, never inline `#[cfg(test)]`
- **Functions <30 lines** typically

### Safety

- **Early exit pattern**: Return errors early, avoid deeply nested logic
- **No `unwrap()`** in library code - always use `?` or explicit error handling
- **`#[must_use]`** on important return types that shouldn't be ignored
- **`catch_unwind`** at thread boundaries for FFI and panic-prone code
- **Every `Result` must be handled** - no silent failures

### Performance

- **Zero-copy**: Prefer `&str` over `String`, `&[T]` over `Vec<T>`
- **`Arc<T>`** for shared ownership instead of cloning large data
- **`Cow<str>`** when ownership is conditional
- **Iterator chains** over collecting intermediate results
- **No allocations in hot paths** - pre-allocate or use iterators
- **Profile before optimizing** - measure don't guess; use `cargo flamegraph` or `perf`

### Algorithm & Data Structure Guidance

For performance-critical code, choose optimal data structures:

- **Lookups**: B-trees, tries, bloom filters - aim for O(log n) or better
- **Filtering**: Roaring bitmaps for set operations (already used in TypeIndex)
- **Memory layout**: Cache-friendly, consider struct-of-arrays for hot paths
- **SIMD**: Use when beneficial via `std::simd` or `portable-simd`

### Concurrency (in order of preference)

1. **Channels** (mpsc, crossbeam) - default choice
2. **Atomics** (`AtomicU64`, etc.) - for counters and flags
3. **Locks** (`Mutex`/`RwLock`) - only when truly necessary

### Style (Rust 2024 Edition)

- **`impl Trait`** over `Box<dyn Trait>` for return types
- **`thiserror`** for library errors, **`anyhow`** for applications
- **Comments for "why" not "what"** - code should be self-documenting
- **Rayon** for CPU-bound work, **Tokio** for IO-bound work
- Standard `rustfmt`, no custom rules

## Architecture

Zero is organized into focused modules that follow strict separation of concerns:

### Core Pipeline: Scan → Diff → Transfer

- **Scanner** (`src/scanner/`) - Parallel directory walking using `jwalk`
- **Differ** (`src/differ/`) - Compares source/dest to identify files needing transfer
- **Transfer** (`src/transfer/`) - Chunked copy with resume for large files (>100MB split into 4MB chunks)
- **Sync** (`src/sync/`) - Orchestrates the full scan → diff → transfer → cache pipeline

### Filesystems operations

- **Hasher** (`src/hasher/`) - xxh3 (fast) + blake3 (cryptographic) hashing with `memmap2`
- **Cache** (`src/cache/`) - Split SQLite database architecture:
  - `control.db` - UI-owned: settings, automations, runs, storage registry
  - `storages/{identifier}.db` - Per-device checksum caches (daemon-owned)
  - Device identification: USB serial → UUID → path hash (in priority order)
- **Index** (`src/index/`) - Roaring bitmap-based search index for sub-millisecond type filtering
  - TypeIndex with `extension_bitmaps`, `path_component_bitmaps`, `trash_bitmap`
  - Persisted with zstd compression (~26MB for 1.3M files)
- **Dedup** (`src/dedup/`) - Size-group then hash for duplicate detection
- **Cleanup** (`src/cleanup/`) - Category-based storage cleanup (36 categories across 8 groups)
- **Storage** (`src/storage/`) - Unified abstraction for local + cloud (S3, B2, GCS, Dropbox, WebDAV via `opendal`)

### Code Intelligence

- **Code** (`src/code/`) - Structural code indexing (functions, types, traits) from Rust (syn) and Go (tree-sitter)
  - `parsers/` - Language-specific AST parsers with `LanguageParser` trait
  - `scanner.rs` - Project discovery and file scanning
  - `persistence.rs` - Compressed .cidx snapshots in `~/.zero/code/`

### Content (Documents and Data)
- **Todo** (`src/todo/`) - Local task management with file-based storage
  - Context-based workflow: `open` a file, then all commands work within it
  - Lists as groupings within a file (inbox, bugs, features, etc.)
  - Postcard binary format with atomic writes

### User-Facing Systems

- **FFI** (`src/ffi/`) - C ABI for Swift integration (macOS GUI)
- **Service** (`src/service/`) - JSON-RPC mode for XPC daemon communication
- **Automation** (`src/automation/`) - Trigger-based sync (USB mount, file changes)
- **Watcher** (`crates/watcher/`) - File events (`notify`) + USB events (DiskArbitration on macOS)
- **LLM** (`crates/zero-llm/`) - LLM agent with streaming, tool calling, multi-provider support
- **MCP** (`crates/zero-mcp/`) - MCP server exposing search + code intelligence over HTTP
- **UI** (`crates/zero-ui/`) - GPUI desktop app: file browser, sidebar, ask view, settings
- **CLI** (`crates/zero-cli/`) - Command-line interface (20+ subcommands)

### Configuration

- **Profiles** (`profiles/file_types.toml`) - File type definitions used by TypeIndex and cleanup
- **Templates** (`src/templates/`) - Backup templates (mac-essentials, macos-developer)

### Product Domains

Maps code paths to user-facing product domains. Used by `/zero:changelog` and `/zero:features` to translate implementation changes into user language. Domain names match FEATURES.md section headings.

| Code path | Product domain |
|---|---|
| `src/index/`, `src/scanner/` | Search |
| `src/code/`, `parsers/` | Code Intelligence |
| `crates/zero-llm/`, `crates/zero-mcp/`, `views/ask/` | AI Assistant |
| `crates/zero-ui/` (sidebar, file_browser, workspace) | File Management |
| `src/sync/`, `src/transfer/`, `src/differ/`, `src/templates/` | Sync & Backup |
| `src/storage/` | Cloud Storage |
| `src/cleanup/`, `src/dedup/` | Cleanup & Dedup |
| `src/automation/`, `crates/watcher/` | Automation |
| `src/hasher/`, `src/ffi/`, `src/service/` | (internal — attribute to the feature it supports) |
| `src/cache/` | (internal — attribute to the feature it supports) |
| `src/telemetry/` | Privacy |
| `crates/zero-cli/` | CLI |
| `src/todo/` | File Management (todo) |

## Key Patterns

### Progress Tracking

All long-running operations use atomic counters for lock-free progress reporting:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

struct Progress {
    bytes_done: AtomicU64,
    bytes_total: AtomicU64,
}

// Update from parallel threads
progress.bytes_done.fetch_add(chunk_size, Ordering::Relaxed);

// Poll from main thread (every 500ms, no callback contention)
let done = progress.bytes_done.load(Ordering::Relaxed);
```

### Roaring Bitmaps for O(1) Filtering

TypeIndex uses roaring bitmaps for instant type/extension queries:

```rust
// Extension filter: O(1) lookup, 0.04ms for images in 1.7M files
let image_files = extension_bitmaps.get("jpg")
    .union(extension_bitmaps.get("png"))
    .union(...);

// Path component filter: O(1) for **/node_modules
let node_modules = path_component_bitmaps.get("node_modules");

// Exclude trash: single bitmap AND
let results = query_results & !trash_bitmap;
```

### Chunked Transfer with Resume

Files >100MB are split into 4MB chunks. If interrupted, resume from last verified chunk:

```rust
// State persisted per-chunk
struct TransferState {
    chunk_index: usize,
    verified_up_to: u64,
}

// Resume: skip to chunk_index, verify chunk hash, continue
```

### Split Database Architecture

Separate concerns to avoid lock contention:

- UI queries `control.db` for automations, runs, storage registry
- Daemon writes to `storages/{serial}.db` for per-device file hashes
- No shared locks between UI and sync operations

### Error Handling

- Libraries use `thiserror` for typed errors
- Binaries use `anyhow` for context
- Always use `?` operator - no `.unwrap()` in library code
- Thread boundaries wrap with `catch_unwind`

## Test Structure

Tests live in separate `*_test.rs` files (never inline `#[cfg(test)]`):

```
src/
├── hasher/
│   ├── mod.rs
│   ├── algorithm.rs
│   ├── hash.rs
│   ├── algorithm_test.rs    # Tests for algorithm.rs
│   └── hash_test.rs         # Tests for hash.rs
```

Run module-specific tests:

```bash
cargo test hasher::
cargo test cache::db_test
```

## Performance Benchmarks

| Operation | Speed | Implementation |
|-----------|-------|----------------|
| Search 1.7M files | 83ms | Tantivy index |
| Type filter (images) | 0.04ms | Roaring bitmap lookup |
| Recent files query | 0.19ms | mtime index |
| Sync (local SSD) | 874 MB/s | Parallel copy |
| Sync + verify | 653 MB/s | Hash-on-copy |
| Resume vs rsync | 3× faster | Chunked transfer |

## Cloud Storage

Unified storage abstraction via `opendal` supports:

- S3: `s3://bucket/path`
- Backblaze B2: `b2://bucket/path`
- Google Cloud: `gs://bucket/path`
- Dropbox: `dropbox://path`
- WebDAV: `webdav://host/path`

Same sync commands work for all backends:

```bash
zero sync ./local s3://bucket/backup
zero verify ./local s3://bucket/backup
```

## Default Command Behavior

The sync command is the default - these are equivalent:

```bash
zero /source /dest
zero sync /source /dest
```

Argument preprocessing in `main.rs` detects path-like arguments and inserts `sync` subcommand.

## Service Mode (XPC Integration)

The `service` subcommand runs in JSON-RPC mode for macOS daemon communication:

```bash
zero service --verbose
```

Communicates via stdin/stdout with the macOS GUI app over XPC.

## Code Review Checklist

When reviewing code, check for:

- [ ] File size under 500 lines
- [ ] Tests in separate `*_test.rs` files
- [ ] No `.unwrap()` in library code
- [ ] All `Result`s handled - no silent failures
- [ ] Zero-copy opportunities (`&str`, `&[T]`, `Arc<T>`)
- [ ] No allocations in hot paths
- [ ] Concurrency follows hierarchy (channels > atomics > locks)
- [ ] `#[must_use]` on important return types
- [ ] `cargo clippy` passes

## Todo CLI

Track tasks via `zero todo`. Tasks live in `.todo` files in your project directory.

```bash
zero todo                                  # Show all tasks grouped by list
zero todo bugs                             # Show bugs list only
zero todo add "Fix crash" bugs             # Add to bugs list
zero todo add "New feature" features       # Add to features list
zero todo toggle 1 2 3                     # Toggle status (open ↔ done)
zero todo update 1 --text "New" --due tomorrow --add-tag priority
zero todo move 3 1                         # Move task #3 after #1
zero todo move 3 top                       # Move task #3 to top of list
zero todo move 3 top --list inbox          # Move task #3 to top of inbox
zero todo open SECURITY.todo               # Switch to different todo file
zero todo close                            # Back to project.todo
```

Suggested lists: `inbox` (default), `bugs`, `features`, `refactor`, `security`, `docs`

All commands support `--json` for structured output.

## Important Gotchas

- **SQLite (turso)**: Logs can be noisy - filtered to `warn` level by default
- **Rayon global pool**: Configured with 8MB stacks to prevent overflow, panic handler for clean exit
- **USB identification**: Serial number is most stable, fallback to UUID then path hash
- **Index loading time**: 500-600ms in CLI output is disk I/O - actual queries are <1ms
- **Search pipe format**: When stdout is piped, outputs `path\t size` for efficient dupes integration
