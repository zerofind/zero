# Features

Unmarked = shipped. `(partial)` = partially implemented. `(planned)` = not yet started.


## Search

- Filename search — substring matching across millions of files.
- Context-aware scoring — results ranked by path depth, recency, hidden/noise directory penalties, and open frequency.
- Frequency tracking — files opened more often rank higher in future searches.
- Glob search — pattern matching with * and ? wildcards.
- Extension filter — filter by extension, comma-separated (pdf,docx,txt).
- Type filter — filter by category: images, videos, audio, documents, code, archives, config, executables, fonts.
- Language subsets — language-specific filters: rust, go, swift, typescript, javascript, python.
- Recent files — query N most recently modified files in 0.19ms.
- Size filter — min/max file size filtering.
- Sort modes — sort by relevance (default), recent, size-desc, size-asc, or name.
- Path filter — scope search to a subdirectory.
- Hidden exclusion — skip files under dot-directories.
- File/directory filter — show only files or only directories.
- Count mode — return match count without listing results.
- Open/Reveal — open first result or reveal in Finder.
- Watch mode — real-time index updates from file system events.
- Sub-millisecond queries — compressed index, ~26MB for 1.3M files.


## Code Intelligence

- Structural search — find functions, types, traits by name, not text grep.
- Cross-project — search across all indexed projects at once.
- Rust support — parses functions, structs, enums, traits, impls, consts, type aliases, modules via syn.
- Go support — parses functions, structs, interfaces, methods, consts, vars via tree-sitter.
- Project overview — languages, file count, LOC, module structure, key public types.
- Compressed storage — .cidx snapshots survive restarts, load on demand.
- AI integration — AI tools can search and understand code structure via MCP.
- CLI — zero code index, search, overview, list, remove, symbols.


## AI Assistant

- LLM agent — conversational assistant with streaming responses and tool calling.
- File search tools — agent queries your search index directly.
- Code search tools — agent searches functions, types, traits across projects.
- Markdown rendering — syntax-highlighted code blocks, links, lists in chat.
- Thinking mode — see the assistant's reasoning process in real-time.
- Tool call cards — visual indicators showing what the AI is doing.
- Copy button — one-click copy on assistant messages.
- Multi-provider — Anthropic (Claude), OpenAI-compatible endpoints.
- Latest models — Claude Sonnet 4.6, Opus 4.6, Haiku 4.5, GPT-5, GPT-5 Mini, GPT-5 Nano.
- MCP server — expose search and code intelligence to any AI tool or IDE.
- Configurable — model, max tokens, thinking budget.
- Embedded terminal — run commands without leaving the app.


## File Management

- Workspaces — multiple independent workspaces with Cmd+1-9 switching.
- Per-workspace state — bookmarks, navigation history, and view state per workspace.
- Create/rename/delete workspaces from the sidebar.
- Split pane — drag between folders without juggling windows.
- Sidebar — pinned/unpinned bookmarks, drives, workspace switcher.
- Git status — modified/added/untracked counts on sidebar bookmark folders.
- Command palette — context-ranked file search and actions (Cmd+K).
- Quick Look — preview files with Space.
- Multiple selection — operate on multiple files at once.
- Inline rename — rename files in place.
- Symlink display — broken links flagged.
- Syntax highlighting — 28 languages via tree-sitter.
- 6 native themes — light + dark variants.
- Native Rust UI — GPUI, not Electron.
- Keyboard-first — full shortcut system with Cmd+K palette.
- Git status in file browser — staged, modified, untracked indicators per file.
- Configurable columns — reorderable, persistent column layout.
- Location column — search results auto-show parent path so you know where each file lives.
- Search-aware toolbar — toolbar shows the search query when viewing results, not the previous folder name.
- Typeahead navigation — jump to files by typing.
- Natural sort — files sorted as humans expect (file2 before file10).
- Todo lists — tasks with lists, tags, due dates, assignees in .todo files.


## Sync & Backup

- Fast copy — OS-native copy with APFS clonefile, sendfile, copy_file_range at ~874 MB/s.
- Chunked transfer — files >100MB split into 4MB chunks, resume from last verified chunk.
- Hash-on-copy — checksum computed during transfer, no second pass, ~653 MB/s verified.
- Mirror mode — make destination identical to source, delete orphans.
- Verify mode — post-sync checksum comparison with hash caching.
- Exclude patterns — skip files by glob pattern.
- Dry run — preview all changes without writing.
- Permission preservation — optionally preserve directory permissions.
- Progress tracking — real-time ETA, throughput, file counts.
- Symlink handling — configurable follow or skip.
- Backup templates — mac-essentials (Documents, Desktop, Pictures, etc.) and macos-developer (code, dotfiles, SSH keys).
- Cloud exclusion — auto-exclude iCloud, Dropbox, Google Drive folders.
- System exclusion — auto-exclude system and library folders.


## Cloud Storage

- Amazon S3 — s3://bucket/path via OpenDAL.
- Backblaze B2 — b2://bucket/path via OpenDAL.
- Google Cloud Storage — gs://bucket/path via OpenDAL.
- Dropbox — dropbox://path via OpenDAL.
- WebDAV — webdav://host/path via OpenDAL.
- Unified API — same commands (sync, verify, ls, cp, get, delete) work across all backends.


## Cleanup & Dedup

36 categories across 8 groups for reclaiming disk space.

- Node modules, Rust target, Python cache, Go cache, Gradle, Maven, Composer, .NET.
- Next.js, Python venv, Tox, Flutter, Zig, CMake, Elixir, Ruby gems, CocoaPods, Parcel, Turborepo.
- Thumbnail cache, log files, downloads, disk images.
- Photos cache, Music cache, Mail attachments, iMessage attachments, iOS backups, Trash.
- Age filtering — filter by file age.
- Size estimation — calculate reclaimable space before deletion.
- Dry run — preview cleanup without deleting.
- Duplicate detection — find files with identical content across directories.
- Dedup by type — scope to images, videos, audio, or any category.
- Dedup by query — combine with search to narrow scope.
- Batch deletion — keeps shortest-path variant, re-verifies before removing.
- Min-size filter — skip files below a size threshold.
- Trash-based deletion — reversible with Put Back support.


## Automation

- USB mount trigger — auto-sync when a specific drive connects.
- File change trigger — auto-sync when source files change.
- Manual trigger — run any automation on demand.
- Device filtering — match by USB serial number or volume name.
- Template sources — use backup templates for source selection.
- Mirror and verify options per automation.
- Debounce — configurable delay for file change triggers (default: 5s).
- Run history — timestamps, status, and error tracking.
- Daemon mode — background service watching for triggers.


## Disk & Security

- Drive listing — all connected volumes with capacity and filesystem.
- Disk info — detailed info for any path: capacity, available, used, filesystem type.
- USB details — vendor, product, serial, USB speed, protocol type.
- Secure erase quick — single zero pass.
- Secure erase normal — double random pass.
- Secure erase strong — VSITR 7-pass.
- Erase verification — verify passes: none, last, or all.
- Safety checks — refuse to erase system disks.


## CLI

- 20+ commands — sync, search, diff, dupes, ls, cp, get, delete, scan, index, drives, disk, erase, cleanup, automation, todo, watch, templates, service, code, telemetry.
- Default command — path-like arguments auto-insert sync: zero /src /dst just works.
- Shell completions — auto-install for bash, zsh, fish, elvish, PowerShell.
- JSON output — --json flag on all commands.
- Self-update — check for and install updates from CDN.
- Pipe-friendly — zero search "*.rs" --json | jq.
- Modular crates — 28 independent crates, each usable standalone for custom tools.


## Privacy

- 100% local search — no queries sent to Apple, cloud, or third parties.
- Anonymous telemetry — opt-in usage statistics, no file names or paths collected.
- Telemetry controls — zero telemetry show (inspect payload), off, on, reset-id.
- 24-hour grace period — no data sent until you've had time to review.
- Local-first — all data stays on your machine or your chosen storage backend.
- Open source — MIT/Apache-2.0 dual-licensed, audit it yourself.
