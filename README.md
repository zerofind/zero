# Zero

The file manager your Mac is missing. Files, search, AI, code intelligence, cloud transfer, sync, cleanup, and automation. Made in Rust. [alpha]

![Zero](assets/screenshot.png)


## Your native apps are spying on you

**Spotlight** — you search for a file on your own computer, and that query gets sent to Apple, Microsoft Bing, and unnamed third parties. Along with your location, the apps you use, your interests, and what you click. Enabled by default since 2014.

**Siri** — recordings sent to Apple servers, including accidental activations. A whistleblower revealed contractors listened to intimate conversations, medical details, and business calls. Apple admitted it. Paid $95M to settle the lawsuit in January 2025.

**iCloud** — in February 2025, the UK government secretly ordered Apple to backdoor iCloud under the Investigatory Powers Act. Apple complied by removing end-to-end encryption for all UK users. UK intelligence now has full access to every UK citizen's iCloud data — photos, documents, backups, everything.

Your native apps are broken too. Spotlight can't find your own files. Finder copy fails? Start over — no resume, no verification, no idea if your backup is complete or corrupted. No git status. No cloud transfers. No way to find duplicates. No way to clean up build artifacts eating your disk. No secure erase.


## Install

```bash
curl -sSfL https://zero-coral-tau.vercel.app | bash
```


## What it does

- **File browser** — Browse files with native git status, permissions, and inline documents. Split pane, workspaces, Cmd+K palette, Quick Look.

- **Search** — Search 1.7M files in 83ms, ranked by frequency and recency. Filter by type, extension, language, size, or path.

- **Text editor** — Edit files inline. Markdown, JSON, TOML, YAML, XML, plain text, config files, and code.

- **Data tables** — Open a CSV and see a real table. Sortable columns, auto-detected delimiters.

- **Git integration** — See modified, staged, and untracked files in the file browser.

- **Copy & sync** — Transfer files that resume from the exact byte. Survives sleep, crashes, and disconnects. Every file checksum-verified. Mirror, verify, exclude, dry run.

- **Cloud transfer** — Sync to S3, Backblaze B2, Google Cloud Storage, Dropbox, and WebDAV.

- **Cleanup** — Find and remove files eating your disk — node_modules, build caches, system logs, old downloads. Preview size before deleting. Reversible.

- **Deduplication** — Find identical files across folders and drives. Filter by type or query. Batch deletion with verification.

- **Code intelligence** — Search for functions, types, and traits by name across every project on your machine. Rust and Go.

- **AI agent** — Automate tasks and discover your files with AI. Ask it to find, move, rename, or clean up — has your full file index. Bring your own key.

- **MCP server** — Expose search and code intelligence to any external AI tool or IDE.

- **Automation** — Run syncs automatically when a USB drive connects or source files change. Runs in the background.

- **Secure erase** — Permanently wipe files with quick, normal, or strong multi-pass methods. Refuses system disks.

- **Themes** — Native themes with light and dark variants.

- **CLI** — JSON output, shell completions, pipe-friendly. `zero /src /dst` just works.

- **Privacy** — 100% local search — nothing leaves your machine. Opt-in telemetry with full inspection. No silent network calls.


## Performance

| Operation | Speed |
|-----------|-------|
| Search 1.7M files | 83ms |
| Type filter (images) | 0.04ms |
| Recent files query | 0.19ms |
| Sync (local SSD) | 874 MB/s |
| Sync + verify | 653 MB/s |
| Resume vs rsync | 3x faster |


## Coming Soon

- Encrypted search index
- P2P file sharing — zero-knowledge, no server middleman
- Automations UI
- Cleanup profiles — community-maintained rules for OS artifacts, dev environments, app traces
- Cross-platform (Linux, Windows)


## Documentation

- [FEATURES.md](FEATURES.md) — Complete feature list
- [COMMANDS.md](COMMANDS.md) — Full CLI reference


## License

MIT / Apache-2.0

---

Built by [Jimmi Andersen](https://github.com/aejimmi) — building [Tell](https://tell.rs). Founded Logpoint (acquired).
