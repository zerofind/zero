# Checksum Cache & Database Design

Design for hash-on-transfer, checksum caching, and the Turso database strategy.

## Problem Statement

Current pain points:
1. **Verify is slow** — 20+ minutes to hash large backups, every time
2. **Redundant work** — We read files during copy but throw away that data
3. **No incremental trust** — Can't know if files are valid without full re-hash
4. **USB instability** — Interruptions require re-verification from scratch

## Goals

1. **Hash-on-transfer** — Compute checksum while copying (free, we're reading anyway)
2. **Cache checksums** — Store (path, size, mtime, hash) for instant future verification
3. **Smart verify** — Use cached hashes when metadata matches, only rehash changed files
4. **Trusted backups** — Know with confidence that files are intact

## Solution Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         Sync Flow                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. Read source file ──────┬──▶ Compute hash (while reading)    │
│                            │                                     │
│  2. Write to destination ──┼──▶ Store in dest/.zero/cache.db  │
│                            │                                     │
│  3. (Optional) Read back ──┴──▶ Verify write, update cache      │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                        Verify Flow                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  For each file:                                                  │
│    1. Stat file (get size, mtime)                               │
│    2. Lookup in cache.db                                         │
│    3. If size+mtime match cached → trust cached hash            │
│    4. If different → rehash, update cache                       │
│                                                                  │
│  Result: "Verified 10,000 files (9,950 cached, 50 rehashed)"    │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Two Types of Storage

### 1. Local Checksum Cache (per backup destination)

**Location:** `<dest>/.zero/cache.db` (SQLite via libsql)

This cache travels with the backup and contains only files in that tree.

```sql
-- Schema for per-destination cache
CREATE TABLE files (
    path TEXT PRIMARY KEY,         -- Relative path from dest root
    size INTEGER NOT NULL,         -- File size in bytes
    mtime INTEGER NOT NULL,        -- Modification time (unix timestamp)
    hash_xxh3 BLOB,                -- 16 bytes (128-bit xxh3)
    hash_blake3 BLOB,              -- 32 bytes (optional, for crypto verification)
    hashed_at INTEGER NOT NULL     -- When we computed this hash
);

CREATE INDEX idx_files_mtime ON files(mtime);
```

**Why SQLite at destination?**
- Travels with the backup (portable)
- Easy to inspect (`sqlite3 .zero/cache.db "SELECT * FROM files"`)
- No central DB corruption risk
- Works offline / on external drives
- Single file, easy to back up

**Staleness handling:**
- On verify: stat file, compare size+mtime with cached
- If match → use cached hash (instant)
- If mismatch → rehash, update cache entry
- Deleted files: remove from cache on full scan

### 2. Central Index (for rules, history, global dedup)

**Location:** `~/.local/share/zero/zero.db` (Turso/libsql)

This is the "brain" that stores sync rules, history, and optionally a global file index.

```sql
-- Rules: which folders sync to which destinations
CREATE TABLE rules (
    id INTEGER PRIMARY KEY,
    name TEXT,                     -- User-friendly name
    source_path TEXT NOT NULL,     -- /Users/me/Documents
    dest_path TEXT NOT NULL,       -- /Volumes/Backup/Documents
    usb_serial TEXT,               -- Match specific drive by serial
    enabled INTEGER DEFAULT 1,
    verify_on_sync INTEGER DEFAULT 0,  -- Auto-verify after sync
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Sync history: when did syncs happen, what was transferred
CREATE TABLE sync_history (
    id INTEGER PRIMARY KEY,
    rule_id INTEGER REFERENCES rules(id),
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    status TEXT NOT NULL,          -- 'running', 'completed', 'failed', 'interrupted'
    files_synced INTEGER DEFAULT 0,
    bytes_synced INTEGER DEFAULT 0,
    files_verified INTEGER DEFAULT 0,
    errors TEXT                    -- JSON array of error messages
);

-- Known drives (for USB serial matching)
CREATE TABLE drives (
    id INTEGER PRIMARY KEY,
    serial TEXT UNIQUE,            -- USB serial number
    label TEXT,                    -- User-friendly name
    vendor TEXT,
    product TEXT,
    last_seen_at INTEGER,
    last_mount_path TEXT
);
```

**Optional: Global file index (Phase 2+)**

For cross-drive dedup and search:

```sql
CREATE TABLE roots (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL,            -- Mount path or folder path
    drive_id INTEGER REFERENCES drives(id),
    label TEXT,
    last_scan INTEGER
);

CREATE TABLE global_files (
    id INTEGER PRIMARY KEY,
    root_id INTEGER REFERENCES roots(id),
    path TEXT NOT NULL,            -- Relative to root
    size INTEGER NOT NULL,
    mtime INTEGER NOT NULL,
    hash_xxh3 BLOB,
    indexed_at INTEGER NOT NULL,
    UNIQUE(root_id, path)
);

CREATE INDEX idx_global_hash ON global_files(hash_xxh3);
```

## Hash-on-Transfer Implementation

### Current Flow (transfer/chunk.rs)

```rust
// Current: copy then optionally verify (2x read)
pub fn copy_file(source: &Path, dest: &Path, options: &CopyOptions) 
    -> Result<(CopyProgress, Option<String>), CopyError>
{
    fs::copy(source, dest)?;  // OS-level copy
    
    if options.verify {
        let src_hash = hash_file(source)?;   // Read source again
        let dst_hash = hash_file(dest)?;     // Read dest
        // Compare...
    }
}
```

### Proposed: Hash-on-copy

```rust
// New: hash while reading, verify by reading dest once
pub fn copy_file_hashed(
    source: &Path, 
    dest: &Path, 
    options: &CopyOptions
) -> Result<CopyResult, CopyError> {
    let source_hash = copy_with_hash(source, dest)?;  // Hash while copying
    
    if options.verify {
        let dest_hash = hash_file(dest)?;  // Read dest once
        if source_hash != dest_hash {
            return Err(CopyError::ChecksumMismatch { ... });
        }
    }
    
    Ok(CopyResult {
        bytes_copied: ...,
        source_hash: Some(source_hash),
        dest_hash: options.verify.then_some(dest_hash),
    })
}

// Hash while streaming copy
fn copy_with_hash(source: &Path, dest: &Path) -> Result<Hash, CopyError> {
    let mut src = File::open(source)?;
    let mut dst = File::create(dest)?;
    let mut hasher = Xxh3::new();
    let mut buffer = [0u8; 128 * 1024];
    
    loop {
        let n = src.read(&mut buffer)?;
        if n == 0 { break; }
        hasher.update(&buffer[..n]);
        dst.write_all(&buffer[..n])?;
    }
    
    Ok(hasher.finalize())
}
```

**Trade-off:** Manual copy is slower than `fs::copy()` on APFS (no clonefile optimization).

**Solution:** Detect if source and dest are on same volume:
- Same volume → use `fs::copy()` (clonefile), then hash if needed
- Different volumes → use `copy_with_hash()` (we're reading anyway)

```rust
fn should_use_streaming_copy(source: &Path, dest: &Path) -> bool {
    // Different volumes = we're transferring bytes anyway, might as well hash
    get_volume_id(source) != get_volume_id(dest)
}
```

## Verify Optimization

### Current: Always rehash everything

```
zero verify /source /dest
→ Scan source, hash all files
→ Scan dest, hash all files  
→ Compare hashes
→ 20 minutes for 100GB
```

### Proposed: Use cache when available

```
zero verify /source /dest
→ Scan dest, load .zero/cache.db
→ For each file:
    - Stat file (size, mtime)
    - If cached && size+mtime match → use cached hash
    - Else → hash file, update cache
→ Compare with source (similarly cached)
→ 2 seconds for 100GB (if nothing changed)
```

### New CLI flags

```bash
# Default: use cache if available, rehash if metadata changed
zero verify /source /dest

# Force full rehash (ignore cache)
zero verify /source /dest --full

# Build/update cache without comparing
zero cache /path/to/folder

# Show cache stats
zero cache /path/to/folder --stats
```

## Cache Invalidation Strategy

### When is a cached hash valid?

A cached entry `(path, size, mtime, hash)` is valid when:
1. File exists at `path`
2. Current `size` == cached `size`
3. Current `mtime` == cached `mtime`

If any differ → rehash and update cache.

### When to clean up stale entries?

Option 1: **Lazy cleanup** — Remove entries for deleted files during verify scan
Option 2: **Periodic sweep** — `zero cache --prune` removes entries for missing files
Option 3: **On sync** — After sync completes, remove entries not in current file list

Recommendation: Option 1 (lazy) by default, with explicit `--prune` available.

### Edge cases

**File modified within same second:**
- Unlikely but possible
- Could add hash of (size, mtime, first-N-bytes) as extra validation
- For now: accept this limitation (same as rsync)

**File replaced with same size/mtime:**
- Extremely rare
- Use `--full` verify if paranoid
- Could optionally store inode number (but doesn't survive copy)

## Performance Projections

| Operation | Current | With Cache | Improvement |
|-----------|---------|------------|-------------|
| Verify 100GB (unchanged) | 20 min | 5 sec | 240x |
| Verify 100GB (10% changed) | 20 min | 2 min | 10x |
| Sync with verify | 2x transfer time | 1.1x transfer time | ~2x |
| Second verify (post-sync) | 20 min | 5 sec | 240x |

## Turso/SQLite Performance

**Will SQLite handle millions of files?**

Yes. SQLite easily handles tens of millions of rows with proper indexing:
- Primary key lookup: O(1)
- Index scan: O(log n)
- 1M files ≈ 50-100MB database
- 10M files ≈ 500MB-1GB database

**Key optimizations:**
1. Use `INTEGER PRIMARY KEY` (rowid alias) where possible
2. Batch inserts (100-1000 at a time) with transactions
3. Use WAL mode for concurrent reads during write
4. Index only what you query (path, hash)

```sql
-- Enable WAL mode (faster concurrent access)
PRAGMA journal_mode=WAL;

-- Batch insert example
BEGIN TRANSACTION;
INSERT OR REPLACE INTO files (path, size, mtime, hash_xxh3, hashed_at) VALUES (?, ?, ?, ?, ?);
-- ... 1000 more ...
COMMIT;
```

## Implementation Plan

### Phase 1: Local checksum cache (Week 2)

1. **Add cache module** (`src/cache/`)
   - `cache.rs` — Open/create SQLite DB
   - `schema.rs` — Create tables, migrations
   - `files.rs` — CRUD operations for file entries

2. **Modify transfer** (`src/transfer/`)
   - Add `copy_with_hash()` for streaming copy+hash
   - Detect cross-volume transfers
   - Store hash in cache after successful copy

3. **Update verify** (`src/main.rs`)
   - Load cache at start
   - Check metadata before hashing
   - Update cache with new hashes
   - Report cache hit rate

4. **New flags**
   - `zero sync --verify` — Verify during transfer
   - `zero verify --full` — Ignore cache
   - `zero cache <path>` — Build/inspect cache

### Phase 2: Central index for rules (Week 2-3)

1. **Add index module** (`src/index/`)
   - `db.rs` — Connection management
   - `rules.rs` — CRUD for sync rules
   - `history.rs` — Sync history tracking
   - `drives.rs` — Known drives registry

2. **Rule-based sync**
   - Store rules in central DB
   - Match USB drives by serial
   - Auto-trigger sync on mount

### Phase 3: Global file index (Future)

1. **Cross-drive dedup**
   - Index files across all roots
   - Query by hash to find duplicates
   - "You have 3 copies of this 2GB file"

2. **Search/discovery**
   - Full-text search on paths
   - Find files by size, type, date
   - Foundation for future AI features

## File Structure

```
~/.local/share/zero/
├── zero.db              # Central index (rules, history, drives)
└── logs/                  # Operation logs

/Volumes/Backup/
└── .zero/
    ├── cache.db           # Checksum cache for this backup
    ├── job.json           # Active transfer state (existing)
    └── completed.log      # Completed files (existing)
```

## Migration Path

1. **New installs:** Start with cache.db from day one
2. **Existing backups:** Build cache on first verify (populate as we hash)
3. **No breaking changes:** Old backups work, just slower until cache builds

## Security Considerations

1. **Hash algorithm choice:**
   - xxh3: Fast, good for dedup/comparison (non-cryptographic)
   - blake3: Fast + cryptographic, use for integrity verification
   - Store both? Or make configurable?

2. **Cache tampering:**
   - Local cache could be modified maliciously
   - For high-security: use `--full` verify
   - Could sign cache entries with machine key (overkill for v1)

3. **Race conditions:**
   - File modified between stat and hash
   - Solution: re-stat after hash, compare mtime
   - Warn if mtime changed during hash

## Open Questions

1. **Store both xxh3 and blake3?**
   - xxh3 for speed (dedup, quick verify)
   - blake3 for cryptographic integrity
   - Recommendation: xxh3 by default, blake3 optional (`--crypto`)

2. **Cache location on source side?**
   - Currently only caching at destination
   - Could also cache source to speed up "what changed?" detection
   - Recommendation: Start with dest-only, add source later if needed

3. **Remote sync future:**
   - Cache would need to sync/merge
   - Turso has built-in replication
   - Design cache schema to support this

## Summary

| Component | Storage | Purpose |
|-----------|---------|---------|
| `dest/.zero/cache.db` | SQLite (local) | Checksum cache per backup |
| `~/.local/share/zero/zero.db` | Turso (central) | Rules, history, drives |
| Global file index | Turso (central) | Cross-drive dedup, search (future) |

**Key wins:**
- Verify goes from 20min → 5sec for unchanged backups
- Hash-on-transfer = free source hashes
- Cache travels with backup (inspect anywhere)
- Foundation for rules, auto-sync, global dedup