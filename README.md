# Zero

> **Privacy-first Finder & Spotlight replacement. What Apple should have built.**

Spotlight sends your searches to Apple, indexes your drives without asking, and often can't find what you're looking for. Finder copy fails? Start over. No resume. No way to see what happened. No verification it worked.

**Zero is different.** Fast search that stays on your machine. Copies that survive disconnect. Verification that proves it worked. Open source core you can audit.

![Zero Demo](assets/demo.gif)

## Download

**[Download Zero for macOS →](https://zero.app/download)**

Or install the CLI:

```bash
cargo install zero
```

---

## Features

| What | Why it matters |
|------|----------------|
| **Instant search** | 1.7M files in 83ms. Local-only. Nothing sent to Apple. |
| **Cmd+K command palette** | Files, actions, locations — keyboard-first. |
| **Resumable sync** | Survives disconnect, sleep, crashes. Picks up where it left off. |
| **Checksum verification** | If Zero says it copied, it actually did. |
| **Split pane** | Finally. No more juggling Finder windows. |
| **Inline editing** | Edit .md, .yaml, .json, .env without opening another app. |
| **Duplicate detection** | Find duplicates across drives in seconds. |
| **Cloud storage** | S3, B2, Dropbox, WebDAV. Same commands, same guarantees. |
| **Todo CLI** | Track tasks in `.todo` files. Lists, tags, due dates. Local-first. |
| **Open source** | Rust core. MIT licensed. Trust, verified. |

### Coming Soon

- E2EE encrypted index
- E2EE transfers (send files to friends, zero-knowledge)
- Automations UI (USB triggers, scheduled backups)
- Batch operations (convert images, resize)

---

## Performance

| Operation | Speed |
|-----------|-------|
| Search 1.7M files | 83ms |
| Type filter (images) | 0.04ms |
| Recent files query | 0.19ms |
| Sync (local SSD) | 874 MB/s |
| Sync + verify | 653 MB/s |
| Resume (vs rsync) | 3x faster |

---

## CLI Quick Start

```bash
# Search (private, fast)
zero search --index ~/                   # Build index once (~30s for 1.7M files)
zero search vacation --type images       # Find vacation photos
zero search --recent 100 --type images   # 100 most recent images

# Sync (resumable, verified)
zero ~/Photos /Volumes/Backup            # Incremental sync with resume
zero ~/Photos s3://bucket/backup         # Same command, cloud storage
zero verify ~/Photos /Volumes/Backup     # Prove backup is identical

# Cleanup
zero dupes ~/Photos                      # Find duplicates
zero dupes ~/Photos --delete             # Remove duplicates (keeps shortest path)

# Todos (local task tracking)
zero todo                                # Show all tasks grouped by list
zero todo add "Fix bug" bugs             # Add task to bugs list
zero todo toggle 1 2 3                   # Toggle status (open ↔ done)
zero todo open SECURITY.todo             # Switch to different todo file

# System
zero drives                              # List drives with hardware info
zero erase /Volumes/USB                  # Secure wipe with verification
```

Enable tab completion:

```bash
zero completions --install
```

---

## Why Zero?

**Privacy:** Your searches stay on your machine. No telemetry. No cloud index. Nothing sent to Apple or us.

**Reliability:** Copies that resume from any failure. Checksums that prove your backup worked. Real progress with file count, speed, ETA.

**Speed:** 1.7M files in 83ms. Not a typo.

**Trust:** Open source Rust core. MIT licensed. Audit it yourself.

---

## Documentation

- [COMMANDS.md](docs/COMMANDS.md) — Full CLI reference
- [ARCHITECTURE.md](ARCHITECTURE.md) — How it works

---

## Contact

Built by [Your Name](https://yoursite.com).

- Twitter: [@yourhandle](https://twitter.com/yourhandle)
- Email: hello@zero.app

Questions, feedback, or just want to say hi — reach out.

Like Zero? [Sponsor on GitHub](https://github.com/sponsors/yourhandle) or [get the Mac app](https://zero.app/download).

---

## License

MIT — use it, fork it, audit it.

---

**Your files. No one else's business.**