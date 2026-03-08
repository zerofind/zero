# Changelog

## v0.6.0

New:
- zero-llm: LLM agent crate with streaming responses, tool use (search_files, search_by_type), multi-provider support (Anthropic, OpenAI-compatible), and conversation history
- zero-mcp: MCP server crate exposing file search, directory listing, and file metadata over Streamable HTTP with Bearer auth
- terminal: embedded terminal view using alacritty_terminal with mouse support, ANSI colors, selection, and Lilex font
- ask: AI chat view with streaming messages, tool call visualization, and provider/model setup
- cleanup: dev garbage scanner with sibling-aware matching and 12 new developer categories (Next.js, Python venv, Flutter, Zig, CMake, Elixir, Ruby, CocoaPods, Parcel, Turborepo)
- cli: cleanup dev subcommand for interactive build artifact scanning and trash-based cleaning
- file-browser: git status via libgit2, configurable/reorderable columns with persistence, typeahead navigation, mount point filesystem detection, inline rename, symlink display with broken-link detection, owner/permissions/flags metadata, natural sort via natord
- dedup: streaming API with channel-based results, prefix hashing for early rejection, and index-accelerated discovery via IndexManager
- settings: MCP server controls (enable, endpoint, API key copy/regenerate) and LLM provider configuration
- platform: NSFileManager-based Trash with Put Back support replacing trash crate
- service: migrated from legacy single-file SearchIndex to multi-root IndexManager
- status-bar: multi-selection summary showing count and aggregate size, cleanup selection mode
- profiles: sibling and aggregate_mode fields on cleanup categories, user pack loading from packs/ directory
- index: remove_paths API for targeted entry removal from all indexes
- sidebar: pin/unpin bookmarks, index location action, eject drive on macOS

Fix:
- keybindings: file clipboard shortcuts (cmd+c/v/x/d) scoped to DataTableView context, preventing conflicts with text inputs
- index: search limit overflow protection via saturating_mul
- index: zstd decompression switched from bounded bulk (256MB limit) to streaming
- cleanup: detail view rewritten around group-based drill-in with virtualized table and delegate pattern
- file-browser: trash is immediate multi-select with optimistic removal instead of single-file confirm dialog
- quicklook: suppressed stdout/stderr noise from qlmanage
- logging: centralized init() with alacritty_terminal and vte logs suppressed to warn level
- profiles: removed raw from images extensions list

## v0.5.0

New:
- index: PathArena replaces per-node String allocations — single contiguous Vec<u8> for all paths (~60% less memory for 2.5M files)
- index: CompactNode (24 bytes, Copy) replaces heap-allocated FileNode in the slab
- index: CompactNameIndex and CompactMtimeIndex use sorted flat arrays instead of BTreeMap
- index: zstd-compressed .zidx snapshot persistence replaces etch WAL directories
- index: FileNode.name field removed — name() now derived from path zero-copy
- editor: tree-sitter syntax highlighting for 28 languages via tree-sitter-languages feature
- ui: ZeroMenuExt trait for context menu items with auto-resolved keyboard shortcuts
- context-menu: reorganized with New/Pane/Actions submenus and shortcut display

Fix:
- sidebar: default start path uses first pinned bookmark (Desktop) instead of home dir, fixing Macintosh HD false highlight
- app: removed premature "Loading search index" banner that appeared before indexing actually starts
- app: toolbar forced visible when sidebar is hidden so nav controls and traffic lights remain accessible
- session: sidebar defaults to open, toolbar defaults to hidden on fresh installs
- cleanup: folder pattern query falls back to text search when component missing from selective index
- search-service: watcher rebuild keeps old index searchable during shadow-build instead of removing first
- theme: emphasis.strong gets foreground color across all 6 themes, title colors adjusted
- theme: CONTENT_INSET reduced to 0
- editor: font changed to Menlo 12px

## v0.4.9

New:
- navigation: view-aware nav stack replaces path-only history — back/forward works across all views (editor, data table, dedup, settings)
- permissions: Full Disk Access onboarding gate defers all filesystem access until FDA is granted
- command-palette: type-filtered search results forwarded to file browser with ShowTypeInBrowser event
- command-palette: render logic extracted to render.rs, added Go Home action
- data-table: CSV delimiter sniffing tests comma, semicolon, tab, and pipe against first 20 lines
- theme: markdown syntax tokens (title, emphasis, links, list markers) across all 6 themes

Fix:
- services: yield between index chunks during loading so UI stays responsive
- file-browser: summary bar pinned to bottom of content instead of above it
- navigation: closing editor or data table navigates back instead of hardcoding FileBrowser

## v0.4.8

New:
- index: progressive chunked loading so search is available per-root instead of blocking until all roots finish
- index: UI shows loading progress as "N/M locations loaded" during startup
- manager: with_index, with_index_mut, remove_stale_root accessors for fine-grained index control

Fix:
- ui: tracing subscriber panic when launched from CLI or .app bundle (init → try_init)

## v0.4.7

New:
- command-palette: drill-in mode for type-filtered search (images, videos, code, etc.) with search-as-you-type, back navigation, and app browsing
- sidebar: custom traffic light buttons with inactive-window tinting, nav controls and path bar when toolbar is hidden
- theme: dual light/dark theme model with independent selectors and system-follow mode
- apps: package inventory skeleton for future multi-source discovery (macOS bundles, Homebrew)
- updater: update Info.plist version when binary lives inside a .app bundle
- keybindings: Cmd+, opens settings, Cmd+/ toggles toolbar visibility

Fix:
- command-palette: input subscription was dropped immediately, preventing all search-as-you-type
- command-palette: mouse clicks on drill-in actions (Search Images, etc.) now work instead of closing the palette
- command-palette: removed distracting mouse hover highlight from result items and action rows
- theme: table.hover.background set to transparent across all 6 themes to stop mouse-tracking highlights during keyboard nav
- file-browser: focus routed to Table's own handle so arrow-key navigation works after clicking status bar
- search: expose search_with_type for combined query + type filtering

## v0.4.6

New:
- data-table: CSV/TSV viewer with sortable columns, numeric alignment, and 100k row limit
- command-palette: search icon prefix, category badges, keyboard shortcut hints, and action labels on selected items
- command-palette: navigation and file actions (new folder, toggle sidebar, go back/forward/up) as palette commands
- settings: segmented TabBar replacing manual toggle buttons, scrollable content area
- dedup: streaming scan progress updates file/byte counters in real-time instead of after collect

Fix:
- platform: gate eject_drive, reveal_in_finder, and Command import behind cfg(target_os = macos) for cross-platform compilation
- settings: version string reads from zero::VERSION constant instead of zero-ui's own CARGO_PKG_VERSION
- settings: general tab uses w_full instead of size_full to prevent overflow clipping
- navigation: alt-left/alt-right keybindings for back/forward on non-macOS
- cli: collapse nested if-let chains to satisfy clippy

## v0.4.5

New:
- updater: self-update system with CDN version checks, SHA512-verified downloads, and atomic binary swap
- updater: background 24h polling in CLI and service mode, `zero update` command
- sidebar: pin/unpin bookmarks as icon tiles with context menu actions
- theme: Catppuccin Latte, Frappe, Macchiato, and Mocha color schemes
- status-bar: reusable StatusBar component for directory and search result summaries

Fix:
- theme: brand, selection, and semantic colors read from active theme instead of hardcoded values
- theme: consolidated tokens module replacing container, icon, layout, shadow files
- navigation: in-place directory navigation avoids entity teardown on folder change
- progress: indexing banner shows live file count with cancel button across all index paths
- index: total_file_count falls back to persisted metadata when indexes not loaded in memory
- editor: toolbar extracted to app-level, save/modified state exposed as public API
- settings: theme picker with dark/light/system mode, auto-update toggle, reactive file count
- deps: ureq, flate2, tar for update support
