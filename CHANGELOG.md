# Changelog

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
