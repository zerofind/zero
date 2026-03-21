# Zero

Privacy-first Finder & Spotlight replacement in Rust. Local search, resumable sync, verified copies, dedup.

## Priorities

**Correctness > Performance > Ergonomics.** When in conflict, this is the order.

Zero's value is speed and privacy. Slow code in hot paths is a bug. Every operation must report progress in real time. All data stays local unless the user explicitly configures cloud storage.

## Codebase

- `crates/` — all 28 domain-organized crates. No prefix (like Zed).
- `crates/cli/` — CLI binary (`zero`). `crates/ui/` — desktop app (`Zero`).
- `./scripts/benchmark.sh` — benchmarks zero vs cp, rsync, rclone. Needs release build.
- Default subcommand is `sync` — CLI inserts it when it detects path-like args.

## Performance standards

These apply to all code. Enforced strictly in hot paths.

- **Zero-copy by default**: `&str` over `String`, `&[T]` over `Vec<T>`, `Arc<T>` for shared data.
- **`Cow<str>`** when ownership is conditional.
- **Iterator chains** over collecting into intermediate `Vec`s.
- **No allocations in hot paths** — pre-allocate, reuse buffers, use arena patterns.
- **`impl Trait`** over `Box<dyn Trait>` for return types when the concrete type is singular. Use `Box<dyn Trait>` for trait objects in collections or when returning different concrete types.

### Data structure choices

- **Point lookups**: `hashbrown::HashMap` (uses `foldhash` by default since 0.15) for O(1) amortized in hot paths. `std::HashMap` (SipHash) only when DoS resistance matters (external input). B-trees only for ordering or range queries.
- **Prefix matching**: Tries, bloom filters for probabilistic membership.
- **Filtering**: Roaring bitmaps for set operations (already used in TypeIndex).
- **Memory layout**: Cache-friendly, struct-of-arrays for hot paths.
- **SIMD**: Use when beneficial via `std::simd` or `portable-simd`.

### Critical paths

| Area | Module | How it achieves performance |
|---|---|---|
| Search | `crates/search/` | PathArena (contiguous `Vec<u8>`, 6-byte path refs), 24-byte `CompactNode`, roaring bitmaps for O(1) type filtering, sorted flat arrays for cache locality, lazy finalization |
| Sync | `crates/transfer/`, `crates/sync/` | Parallel chunked transfer (4MB chunks for >100MB files), resume from last verified chunk |
| Hashing | `crates/hasher/` | xxh3 (fast) + blake3 (crypto), memory-mapped I/O via `memmap2` (files >20MB) |
| Scanning | `crates/scanner/` | `jwalk` for parallel directory walking, roaring bitmap TypeIndex |

Do not introduce linear scans, string allocations, or heap-heavy structures in these modules.

### Allocator

Use **mimalloc** as the global allocator in both binaries (`cli`, `ui`). Under heavy multithreaded workloads it delivers ~5x throughput over system malloc with lower RSS. Two lines per binary:

```rust
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
```

## Transparency

All long-running operations must emit real-time progress.

- **Atomic counters** (`AtomicU64`/`AtomicUsize`, `Ordering::Relaxed`) for lock-free updates from worker threads.
- **Phase-aware tracking**: Operations progress through phases (Scanning → Diffing → Transferring → Complete), each with own counters.
- **Cancellation** via `AtomicBool` (`Release`/`Acquire`) — operations must check and exit cleanly.
- **No callbacks in hot paths** — poll atomics from display thread at 100-500ms intervals.
- **Channels** for streaming computed results (e.g., hash batching via `mpsc::channel`).

See `crates/progress/` for `AtomicProgress` and `crates/sync/src/types.rs` for `SyncAtomicProgress`.

## File and test structure

- **500 lines max per file.** Split into sub-components if larger.
- **Functions under 30 lines** typically. Extract helpers if longer.
- **Tests go in `*_test.rs` files, never inline `#[cfg(test)]`.** Opposite of Rust convention.
  - Add `#[cfg(test)] mod foo_test;` in the parent module.
  - Start with `use super::*;`
  - Name tests `test_<what>_<scenario>` (e.g., `test_hash_small_file_blake3`).
  - Use `tempfile::TempDir` for filesystem fixtures — no global state between tests.
  - Group related tests with section comment headers (`// -- topic ---`).
  - Test both success and error paths.

## Path handling

Filesystem paths are not UTF-8 on Linux/macOS. Never use `String`/`&str` for paths in general code.

- **`Path`/`PathBuf`** for all filesystem operations.
- **`path.to_str()`** with explicit `None` handling when string conversion is needed. Never silently `to_string_lossy()` on data that will be used for file operations.
- **`Path::join()`**, **`Path::parent()`**, **`Path::file_name()`** — never split on `/` or use `format!` to build paths.
- **`dunce::canonicalize()`** on Windows to avoid UNC path issues.
- **PathArena** (`crates/search/`) stores paths as `Vec<u8>` — reconstruct as `Path`/`OsStr`, never `String`.

## Error handling

- **No `.unwrap()` in library code.** Use `?` or explicit handling. Acceptable only in tests.
- **Never discard errors with `let _ =`** on fallible operations. Propagate with `?`, handle explicitly, or log.
- **Careful with indexing (`vec[i]`).** Use `.get(i)` when bounds aren't guaranteed.
- **`#[must_use]`** on important return types that shouldn't be ignored.
- **`catch_unwind`** at Rayon thread boundaries.
- **`thiserror`** for library errors, **`anyhow`** for binaries. Library crates must not depend on `anyhow`.

## Concurrency

In preference order:
1. **Channels** (`crossfire`) — default for all new channel usage
2. **Atomics** — counters, flags, progress
3. **Locks** (Mutex/RwLock) — last resort, never in hot paths

**Rayon** for CPU-bound parallelism, **Tokio** for IO-bound async.

### Channel patterns (crossfire v3)

- **Sync blocking**: `crossfire::mpsc::unbounded_blocking::<T>()` — returns `(MTx, Rx)`
- **Async**: `crossfire::mpsc::bounded_async::<T>(cap)` — returns `(MAsyncTx, AsyncRx)`
- **Sync→Async bridge**: `crossfire::mpsc::bounded_blocking_async::<T>(cap)` — blocking sender, async receiver (used in service runner for stdin/USB threads feeding into tokio::select)
- `MTx` is `Clone` — no `Arc<Sender>` needed. `Rx`/`AsyncRx` are single-consumer (not Clone).
- Async `recv()` returns `Result<T, RecvError>`, not `Option<T>` — use `Ok(val) = rx.recv()` in `tokio::select!`.
- `Rx` does not implement `IntoIterator` — use `while let Ok(val) = rx.recv()` instead of `for val in rx`.
- Exception: `futures::channel::mpsc` in `ui/terminal/` — required by alacritty integration.

Use **`std::sync::LazyLock`** and **`std::sync::OnceLock`** (stable since 1.80) instead of `lazy_static!` or `once_cell`. No external crate needed.

### Rayon ↔ Tokio bridging

The two runtimes must never mix on the same thread. Violating these rules causes deadlocks that the compiler cannot catch.

- **Never `block_on` inside async context.** Create `tokio::runtime::Runtime` only at entry points (CLI commands, service main). Nesting runtimes deadlocks immediately.
- **`spawn_blocking` for CPU work from async.** Any call into rayon (SyncJob, parallel scan, parallel hash) from a Tokio task must go through `tokio::task::spawn_blocking`. Never call rayon APIs directly from an async fn.
- **`blocking_send` / `Handle::current().spawn()` for sync→async.** Sync threads (e.g., USB watcher) that need to send to Tokio use `tokio::sync::mpsc::Sender::blocking_send` or grab `Handle::current()` — but only from threads spawned within an active runtime's `block_on` scope.
- **No Tokio primitives from rayon threads.** Rayon workers must not call `.await`, `Handle::current()`, or any tokio sync primitives. Communicate results back via `std::sync` channels or `Arc<Mutex>` collection.

### Lock selection

- **`tokio::sync::Mutex` / `RwLock`** in async code (safe to hold across `.await`).
- **`std::sync::Mutex` / `RwLock`** in sync code and rayon closures.
- Never hold a `std::sync::Mutex` across an `.await` point (blocks the executor). Never use `tokio::sync::Mutex` from a rayon thread (panics without runtime context).

### Send + Sync discipline

- **Trait objects crossing thread boundaries** must be `Box<dyn Trait + Send + Sync>` or `Arc<dyn Trait + Send + Sync>`. See `StorageBackend`, `LanguageParser`.
- **`unsafe impl Send/Sync`** requires a `// SAFETY:` comment listing every field and why it is safe. Adding a non-atomic field (e.g., `String`, `Vec`) to a type with manual Send/Sync impls invalidates the safety argument — update or remove the unsafe impl.
- **UI-only types** (`Rc`, `RefCell`, GPUI closures) must stay on the main thread. Never wrap them in `Arc` or send across threads.

## Database boundaries

Cache uses **etch** (postcard snapshots + WAL), not SQLite.

- `control/` is **UI-owned**: settings, automations, runs, storage registry.
- `storages/{identifier}/` is **daemon-owned**: per-device checksums.
- Never write to the other's store. No shared locks between UI and sync.

## Privacy

- All search is 100% local. Nothing leaves the machine.
- Telemetry is opt-in and transparent (`crates/telemetry/`).
- No silent network calls. Cloud storage requires explicit user configuration.

## Product domains

Maps code paths to user-facing domains. Used by changelog and features skills. Names match FEATURES.md headings.

| Code path | Domain |
|---|---|
| `crates/search/`, `crates/scanner/` | Search |
| `crates/code/` | Code Intelligence |
| `crates/llm/`, `crates/mcp/`, `crates/ui/src/views/ask/` | AI Assistant |
| `crates/ui/` (sidebar, file_browser, workspace) | File Management |
| `crates/sync/`, `crates/transfer/`, `crates/differ/`, `crates/templates/` | Sync & Backup |
| `crates/storage/` | Cloud Storage |
| `crates/cleanup/`, `crates/dedup/` | Cleanup & Dedup |
| `crates/automation/`, `crates/watcher/` | Automation |
| `crates/hasher/`, `crates/service/` | (internal) |
| `crates/cache/` | (internal) |
| `crates/telemetry/` | Privacy |
| `crates/cli/` | CLI |
| `crates/todo/` | File Management (todo) |

## Logging

Use **`tracing`**, not `log`. Controlled via `RUST_LOG` env var (see `crates/foundation/src/logging.rs`).

**Levels**:
- `error!` — operation failed, result is affected
- `warn!` — unexpected but recovered (skipped file, fallback path taken)
- `info!` — operation lifecycle: started, completed, key milestones
- `debug!` — internal flow useful during development (query parameters, config values)

**Style**:
- **Structured fields** over format strings: `info!(root = %path, files = count, "rebuild complete")` not `info!("rebuild complete: {} files in {}", count, path)`.
- **`#[instrument]`** on public entry points in hot-path crates (search, sync, transfer, scanner, dedup). Use `skip` for large/non-Debug args, `fields` for display-formatted values: `#[instrument(skip(self, progress), fields(root = %root.display()))]`.
- **Fully qualified `tracing::warn!(...)`** in modules that don't `use tracing::*`. Crates that use `#[instrument]` import `use tracing::instrument;`.

## Sanitizers (nightly, periodic)

- **AddressSanitizer** — on PRs touching unsafe or FFI code.
- **ThreadSanitizer** — on PRs touching concurrent code (`progress/`, `sync/`, `transfer/`, `watcher/`).
- **Miri** (`cargo +nightly miri test`) — for modules with `unsafe` blocks.

## Gotchas

- **Rayon global pool**: 8MB stacks to prevent overflow, panic handler for clean exit.
- **USB identification**: Serial → UUID → path hash (in fallback order).
- **Index loading**: 500-600ms in CLI output is disk I/O — queries themselves are <1ms.
- **Search pipe format**: When stdout is piped, outputs `path\tsize` for dupes integration.
- **Cache is etch, not SQLite**: Postcard snapshots + WAL. Don't add SQLite dependencies.
- **mmap safety**: `memmap2` mappings are `unsafe` because the OS can deliver changes from other processes through the mapping, violating Rust's aliasing guarantees on `&[u8]`. Every `unsafe` block must have a `// SAFETY:` comment that addresses: (1) the mapping is read-only, (2) what happens if another process modifies or truncates the file (e.g., incorrect hash — acceptable; SIGBUS on truncation — same risk as regular I/O), (3) why the performance tradeoff is worth it. Never claim "exclusive access" unless you actually hold a file lock.
