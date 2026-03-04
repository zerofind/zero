# Changelog

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
