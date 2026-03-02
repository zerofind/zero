# Command Reference

Full documentation for all zero commands.

## Primary Commands

### `sync` — Smart file synchronization (default command)

Compares source and destination, copies only what's new or changed. If interrupted, resumes from where it left off.

```bash
# Basic sync (note: 'sync' subcommand is optional!)
zero /source /dest                   # Incremental sync with auto-resume
zero /source /dest --verify          # Verify ALL files (hash + verify + cache)
zero /source /dest --mirror          # Make destination identical (delete orphans)
zero /source /dest --dry-run         # Preview what would happen
zero /source /dest -x node_modules -x "*.tmp"  # Exclude patterns
zero /source /dest -p                # Preserve directory permissions
zero /source /dest --no-chunked      # Disable chunked transfer (no resume)

# Cloud storage (same commands!)
zero ./local/backup s3://mybucket/backup       # Local → S3
zero s3://mybucket/data ./local/restore        # S3 → Local
zero ./photos b2://backups/photos --mirror     # Local → Backblaze B2
zero gs://bucket/data ./local/data             # Google Cloud → Local
```

**Supported cloud backends:** S3, Backblaze B2 (`b2://`), Google Cloud Storage (`gs://`), Dropbox (`dropbox://`), WebDAV (`webdav://`)

Large files (>100MB) automatically use chunked transfer. If interrupted, the next sync resumes from the last verified chunk — a 10GB file interrupted at 8GB resumes from 8GB, not zero.

---

### `ls` / `list` — List files

```bash
zero ls ./local/dir                  # List local directory
zero ls /path -l                     # Long format with sizes
zero ls s3://mybucket/path/          # List S3 bucket
zero ls b2://bucket/ -r              # List recursively
zero list ./dir                      # 'list' alias
```

---

### `cp` / `copy` — Copy files

```bash
zero cp ./file.txt /backup/file.txt              # Local to local
zero cp ./file.txt s3://bucket/file.txt          # Upload to S3
zero cp s3://bucket/file.txt ./local/            # Download from S3
zero cp ./local/dir s3://bucket/backup/ -r       # Upload directory
zero copy s3://bucket/data/ ./restore/ -r        # 'copy' alias
```

---

### `get` / `download` — Quick download from cloud

Download to current directory (like `wget` for cloud):

```bash
zero get s3://bucket/file.txt              # → ./file.txt
zero get s3://bucket/photos/ -r            # → ./photos/ (recursive)
zero get b2://bucket/data.zip -o ./dest/   # → ./dest/data.zip
zero download gs://bucket/archive/ -r      # 'download' alias
```

---

## Verification & Comparison

### `verify` — Confirm folders are identical

```bash
zero verify /source /dest            # Use cache if available, else hash
zero verify /source /dest --full     # Force rehash all (ignore cache)
zero verify /source /dest --quick    # Metadata only: size + mtime (fast)
zero verify /source /dest --check-permissions  # Also verify permissions
```

**Default mode** uses cached checksums — nearly instant for unchanged files. **Full mode** rehashes everything. **Quick mode** only checks sizes and timestamps.

---

### `diff` — Show differences between folders

```bash
zero diff /source /dest              # Compare by size + mtime (fast)
zero diff /source /dest --checksum   # Compare by checksums (thorough)
zero diff /source /dest --show-identical  # Also show matching files
```

---

## File Operations

### `delete` / `rm` — Delete files and directories

Moves files to Trash by default (safe). Use `--permanent` for immediate deletion.

```bash
zero delete file.txt                 # Move to Trash (safe, default)
zero delete dir1 dir2                # Move multiple items to Trash
zero delete file.txt --permanent     # Permanently delete (DANGEROUS)
zero delete *.log --dry-run          # Preview what would be deleted
zero delete folder/ -f               # Skip confirmation prompt
zero rm old-backup/                  # 'rm' alias
```

**Options:**

| Flag | Description |
|------|-------------|
| `-p, --permanent` | Permanently delete instead of moving to Trash |
| `-f, --force` | Skip confirmation prompt |
| `-n, --dry-run` | Preview what would be deleted |
| `--json` | Output results as JSON |

**Examples:**

```bash
# Safe cleanup - moves to Trash (can be recovered)
zero delete ~/Downloads/*.dmg

# Permanent delete with preview first
zero delete /tmp/build-artifacts --dry-run
zero delete /tmp/build-artifacts --permanent -f

# Batch delete from a list
cat files-to-delete.txt | xargs zero delete --permanent -f
```

**Notes:**
- On macOS, Trash integration uses Finder, so "Put Back" works correctly
- Directories are always deleted recursively
- Without `--force`, you'll be prompted to confirm before deletion

---

### `dupes` — Find and remove duplicate files

Groups files by content hash. Uses size as fast filter, then hashes only size-matched files.

Supports multiple input modes: directory scan, search filters, stdin pipe, or file list.

```bash
# Basic directory scan
zero dupes ~/Photos                  # List duplicates (read-only)
zero dupes ~/Photos --delete         # Delete duplicates (keeps shortest path)
zero dupes ~/Photos --verify         # Double-check hashes before deleting
zero dupes ~/Photos --min-size 1000  # Skip files smaller than 1KB

# With search filters (requires search index)
zero dupes ~/Photos "vacation"       # Duplicates matching "vacation"
zero dupes ~/Photos --type images    # Duplicate images only
zero dupes ~/Photos "2024" --type images  # Combine query + type filter

# From search pipe (fast path — skips filesystem scan)
zero search --type images | zero dupes           # Duplicate images (3x faster)
zero search --type images "vacation" | zero dupes  # With query
zero search --recent 1000 --type images | zero dupes  # Recent images

# From stdin pipe (auto-detected)
find ~/Downloads -name "*.zip" | zero dupes

# From file
zero dupes --from-file paths.txt     # Read paths from file
```

**Piped mode performance:** When piping from `zero search`, dupes receives paths + sizes directly, skipping the filesystem scan. This is ~3x faster for large directories.

**Type filters:** `images`, `videos`, `audio`, `documents`, `code`, `archives`, `config`

**Note:** Query and type filters require a search index. Build one first with:
```bash
zero search --index ~/Photos
```

---

### `transfer` — Simple full copy

Copies ALL files. No diffing, no state. Use when you want a fresh copy of everything.

```bash
zero transfer /source /dest          # Copy everything
zero transfer /source /dest --verify # Verify checksums after copy
zero transfer /source /dest -p       # Preserve directory permissions
```

For incremental backups, use `sync` instead.

---

### `scan` — Inspect a directory

```bash
zero scan /path                      # File count, total size, samples
zero scan /path -m 2                 # Limit to depth 2
zero scan /path --skip-hidden        # Exclude hidden files
zero scan /path --benchmark 5        # Benchmark with 5 iterations
```

---

### `index` — Build checksum index

Hash files and cache checksums. Once indexed, verification is nearly instant.

```bash
zero index /path                     # Build index (hash + cache)
zero index /path --stats             # Show index statistics
zero index /path --list              # List indexed files with hashes
zero index /path --prune             # Remove entries for deleted files
zero index /path -a blake3           # Use blake3 instead of xxh3
zero index /path --no-store          # Display hashes without storing
```

---

## Search

### `search` — Fast file search (private Spotlight replacement)

100% local indexing — nothing sent to Apple. Type filtering uses roaring bitmaps for sub-millisecond queries on 1M+ files.

```bash
# Build search index
zero search --index ~/Documents              # Index a directory
zero search --index ~ --cache ~/myindex.bin  # Custom index location

# Search
zero search report                           # Search for "report"
zero search config -n 20                     # Limit to 20 results
zero search --type images                    # List all images (no query needed!)
zero search --type images -n 0               # ALL images (unlimited)
zero search vacation --type images           # Search images for "vacation"
zero search --ext pdf                        # List all PDFs
zero search --files-only main                # Only files, no directories
zero search --dirs-only src                  # Only directories

# Recent files (uses mtime index for O(K) performance)
zero search --recent 50                      # 50 most recently modified files
zero search --recent 100 --type images       # 100 most recent images
zero search "report" --recent 20             # Recent files matching query
zero search --recent 50 --type code          # Recent code files

# Filter by directory (--in)
zero search --type rs --in ~/code/myproject  # Rust files in myproject
zero search lib --type rs --in .             # Rust files matching "lib" in cwd
zero search --type images --in ~/Pictures    # Images in Pictures folder

# Pipe to dupes (fast path — skips filesystem scan)
zero search --type images | zero dupes       # Find duplicate images
zero search --recent 1000 | zero dupes       # Duplicates in recent files
zero search "backup" --type archives | zero dupes  # Duplicate archives

# Count without listing
zero search --type images --count            # Just show count
zero search --type code -c                   # Short form
zero search --type rs --in . -c              # Count Rust files in cwd

# Watch for changes (real-time index updates)
zero search --watch --index ~/Documents      # Build index + watch for changes

# List available type filters
zero search --types
```

**Options:**

| Flag | Description |
|------|-------------|
| `-n, --limit` | Maximum results (default: 50, use 0 for unlimited) |
| `-r, --recent` | Show N most recently modified files |
| `-c, --count` | Only show count, don't list files |
| `-t, --type` | Filter by type (images, videos, audio, etc.) |
| `-e, --ext` | Filter by extension |
| `--in <PATH>` | Limit search to this directory |
| `--files-only` | Exclude directories |
| `--dirs-only` | Exclude files |

**Type filters:**

| Type | Extensions |
|------|------------|
| `images` | png, jpg, jpeg, gif, webp, heic, bmp, tiff, svg, raw... |
| `videos` | mp4, mov, avi, mkv, webm, m4v, wmv... |
| `audio` | mp3, wav, flac, aac, ogg, m4a... |
| `documents` | pdf, doc, docx, xls, xlsx, ppt, txt, md... |
| `code` | rs, py, js, ts, go, java, c, cpp, swift... |
| `archives` | zip, tar, gz, 7z, rar, dmg, iso... |
| `config` | json, yaml, toml, xml, ini, env... |

**Quick language filters:**

| Type | Extensions |
|------|------------|
| `rs` | Rust (.rs) |
| `go` | Go (.go) |
| `swift` | Swift (.swift) |
| `ts` | TypeScript (.ts, .tsx, .mts) |
| `js` | JavaScript (.js, .jsx, .mjs) |
| `py` | Python (.py, .pyi) |

**Performance:** 

- Index persisted with zstd compression (~26MB for 1.3M files)
- Indexing: ~1.3M files in ~32s
- Type queries: **0.04ms** (roaring bitmap lookup)
- Recent queries: **0.19ms** (mtime index lookup)
- Text queries: <50ms
- Combined (text + type): ~45ms

The 500-600ms you see in CLI output is index loading from disk. The actual query is sub-millisecond.

**Piping to dupes:** When stdout is piped, search outputs `path\tsize` format with unlimited results. This allows `zero dupes` to skip filesystem scanning entirely — ~3x faster than `zero dupes --type images`.

---

## System Information

### `drives` — List connected drives

```bash
zero drives                          # List all drives
zero --json drives                   # JSON output for scripts
```

Shows USB hardware details (vendor, speed, serial), capacity, file system. Example:

```
Ghost (SanDisk 3.2Gen1)
├─ Speed: USB 3.0 SuperSpeed (5 Gbps)
├─ Serial: 00023731092024212130
├─ Capacity: 114.60 GB (9.90 GB free)
└─ Mount Point: /Volumes/Ghost
```

---

### `disk` — Detailed volume info

```bash
zero disk /Volumes/Ghost             # Info for USB drive
zero disk /                          # Info for system drive
zero disk /path/to/any/file          # Info for volume containing file
```

---

### `erase` / `wipe` — Secure disk erasure

Securely erase a disk by overwriting all data with random patterns. **Irreversible!**

Only works on external/removable drives — refuses to erase internal/system disks.

```bash
# Basic usage (requires confirmation)
zero erase /Volumes/MyUSB            # Normal: 2-pass random (recommended)
zero erase /Volumes/MyUSB --level quick   # Quick: 1-pass zero (fast, less secure)
zero erase /Volumes/MyUSB --level strong  # Strong: VSITR 7-pass (maximum security)

# Options
zero erase /Volumes/MyUSB --dry-run       # Preview without erasing
zero erase /Volumes/MyUSB --verify all    # Verify every pass (slower)
zero erase /Volumes/MyUSB --verify no     # Skip verification (faster)
zero erase /Volumes/MyUSB -y              # Skip confirmation (DANGEROUS)

# Resume after interruption (automatic!)
zero erase /Volumes/MyUSB --level strong  # Prompts to resume if interrupted
zero erase /dev/rdisk22s1 --level strong  # Or use device path directly
zero erase /dev/rdisk22s1 -y              # Auto-resume without prompt
zero wipe /dev/rdisk22s1                  # 'wipe' alias
```

**Security levels:**

| Level | Method | Passes | Use case |
|-------|--------|--------|----------|
| `quick` | Zero fill | 1 | Non-sensitive data, quick reformat |
| `normal` | Random fill | 2 | Personal data, general use (recommended) |
| `strong` | VSITR (zero/one alternating + random) | 7 | Highly sensitive data, compliance |

**Example output:**

```
Secure Erase

⚠️  WARNING: THIS OPERATION IS IRREVERSIBLE ⚠️

Target device:
  Name:        Ghost
  Device:      /dev/rdisk22s1 ← use this to restart if interrupted
  Size:        123.0 GB
  Mount Point: /Volumes/Ghost

Erase settings:
  Level:       strong (VSITR 7-pass)
  Passes:      7
  Verify:      Last
  Est. Time:   ~2 hours 23 minutes

Pass 1/7: zero fill (writing)
  Writing 45%  [██████████████████████                    ]  67.2 MB/s  Pass ETA: 15m | Total ETA: 1h 45m
```

**Resume support:**

If interrupted (laptop closed, power loss, Ctrl+C), just run the same command again:

```bash
# Original command
sudo zero erase /Volumes/Ghost --level strong

# After interruption, run the same command - it will detect and offer to resume
sudo zero erase /Volumes/Ghost --level strong
# Or use device path if volume is unmounted
sudo zero erase /dev/rdisk22s1 --level strong
```

Resume state is saved every 10 seconds and includes:
- Which pass you're on (e.g., Pass 3/7)
- Exact byte position within the pass
- Random seeds used (so verification still works)

**Notes:**
- Requires `sudo` on macOS (direct disk access needs root)
- For flash/SSD devices, a warning is shown about wear leveling limitations
- State is saved to `~/.local/share/zero/erase/` and auto-deleted on completion
- The volume is automatically unmounted before erasing

---

## Automation

### `automation` — Manage sync automations

Automations connect triggers (USB mount, file changes) to sync actions.

```bash
zero automation list                 # List all automations
zero automation show 1               # Show automation details
zero automation run 1                # Run manually
zero automation history 1            # Show run history
zero automation delete 1             # Delete automation
zero automation daemon               # Start daemon (watches for events)
```

**Create automations:**

```bash
# Using a template (recommended)
zero automation create \
  --name "Mac Backup" \
  --template mac-essentials \
  --device-serial "ABC123" \
  --volume-name "Backup Drive"

# Developer template (excludes node_modules, .git, etc.)
zero automation create \
  --name "Dev Backup" \
  --template macos-developer \
  --dest-path /Volumes/BackupDrive

# Custom sources with triggers
zero automation create \
  --name "Work Backup" \
  --source ~/Documents --dest Documents \
  --source ~/code --dest code \
  --dest-path /Volumes/BackupDrive \
  --on-mount \
  --on-change \
  --debounce-ms 10000
```

---

### `templates` — Backup templates

```bash
zero templates                       # List all templates
zero templates --show mac-essentials # Show template details
zero templates --show macos-developer --resolve  # Check which paths exist
```

| Template | Contents |
|----------|----------|
| `mac-essentials` | Documents, Downloads, Desktop, Pictures, Movies, Music |
| `macos-developer` | Essentials + code, .ssh, .config, dotfiles (excludes node_modules, .git) |

---

### `watch` — Monitor events (dev/testing)

```bash
zero watch files /path               # Watch for file changes
zero watch usb                       # Watch for USB mount/unmount
zero watch latency /path             # Measure event latency
```

---

## Utilities

### `completions` — Shell completions

```bash
zero completions --install           # Auto-detect shell & install
zero completions fish                # Print fish completions
zero completions bash                # Print bash completions
zero completions zsh                 # Print zsh completions
```

---

## Global Options

All commands support:

```bash
zero -v ...                          # Verbose output
zero --json ...                      # JSON output (for scripting/GUI)
zero -h, --help                      # Print help
zero -V, --version                   # Print version
```

---

## JSON Output

```bash
zero --json /source /dest
zero --json drives
```

```json
{
  "success": true,
  "command": "scan",
  "duration_ms": 1234,
  "data": { "total_files": 1234, "total_bytes": 5678901234 }
}
```
