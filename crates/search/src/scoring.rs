//! Per-node context scoring for search results
//!
//! Scores files based on name match quality, recency, path depth,
//! path proximity to user dirs, and context penalties (hidden, noise,
//! trash, system, library).

/// Known noise directories that indicate non-user content
const NOISE_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "vendor",
    "__pycache__",
    ".venv",
    "venv",
    "dist",
    ".cache",
    ".cargo",
    ".rustup",
    ".npm",
    ".yarn",
    ".pnpm",
];

/// Well-known user content directories (matched after /Users/<name>/ or ~/)
const USER_DIRS: &[&str] = &[
    "Desktop",
    "Documents",
    "Downloads",
    "Movies",
    "Music",
    "Pictures",
];

/// Context extracted from a file's path and metadata for scoring.
/// All fields derived from a single `O(path_len)` scan.
pub struct NodeContext<'a> {
    pub name: &'a str,
    pub mtime: u64,
    pub depth: u16,
    pub has_hidden_component: bool,
    pub has_noise_component: bool,
    pub is_system_path: bool,
    pub is_library_path: bool,
    pub is_trash: bool,
    pub is_user_dir: bool,
}

impl<'a> NodeContext<'a> {
    /// Build from path + mtime. Single pass over path string.
    pub fn new(path: &'a str, mtime: u64) -> Self {
        let mut depth: u16 = 0;
        let mut has_hidden = false;
        let mut has_noise = false;
        let mut is_system = false;
        let mut is_library = false;
        let mut is_trash = false;
        let mut is_user_dir = false;

        // Extract name (last component)
        let name = match path.rfind('/') {
            Some(pos) => &path[pos + 1..],
            None => path,
        };

        // Track whether we're under /Users/<name>/
        let mut in_users_home = false;

        for component in path.split('/') {
            if component.is_empty() {
                continue;
            }
            depth += 1;

            // Hidden: starts with '.' and length > 1 (not just ".")
            // Index 0 is valid: guarded by component.len() > 1 above
            #[allow(clippy::indexing_slicing)]
            if component.len() > 1 && component.as_bytes()[0] == b'.' {
                has_hidden = true;
            }

            // Trash detection
            if !is_trash && component.eq_ignore_ascii_case(".trash") {
                is_trash = true;
            }

            // Noise directory check (no allocation)
            if !has_noise {
                for &noise in NOISE_DIRS {
                    if component.eq_ignore_ascii_case(noise) {
                        has_noise = true;
                        break;
                    }
                }
            }

            // System/Library path detection (first few components)
            if depth <= 2 {
                match component {
                    "usr" | "System" | "sbin" | "bin" | "opt" | "private" => is_system = true,
                    "Library" => is_library = true,
                    "Users" => in_users_home = true,
                    _ => {}
                }
            }

            // User directory detection (depth 3 = /Users/<name>/<dir>)
            if depth == 3 && in_users_home {
                for &ud in USER_DIRS {
                    if component == ud {
                        is_user_dir = true;
                        break;
                    }
                }
            }
        }

        Self {
            name,
            mtime,
            depth,
            has_hidden_component: has_hidden,
            has_noise_component: has_noise,
            is_system_path: is_system,
            is_library_path: is_library,
            is_trash,
            is_user_dir,
        }
    }
}

/// Name match quality (0–1100).
pub fn name_score(name: &str, query: &str) -> u32 {
    let mut score = if name == query {
        1000
    } else if name.starts_with(query) {
        500
    } else {
        100
    };
    score += 100u32.saturating_sub(name.len() as u32);
    score
}

/// Full per-node score. Called once per candidate node.
pub fn score_result(ctx: &NodeContext<'_>, query: &str, now: u64) -> u32 {
    let base = name_score(ctx.name, query);
    let bonus = recency_bonus(ctx.mtime, now) + depth_bonus(ctx.depth) + proximity_bonus(ctx);
    let penalty = context_penalty(ctx);
    (base + bonus).saturating_sub(penalty)
}

/// Recency boost (0–200).
fn recency_bonus(mtime: u64, now: u64) -> u32 {
    let age_days = now.saturating_sub(mtime) / 86400;
    match age_days {
        0 => 200,
        1..=7 => 150,
        8..=30 => 100,
        31..=90 => 50,
        _ => 0,
    }
}

/// Shallow paths score higher, progressive penalty for deep nesting (0–100).
fn depth_bonus(depth: u16) -> u32 {
    // depth 1 → 100, depth 2 → 90, ... depth 10 → 10, depth 11+ → 0
    100u32.saturating_sub(u32::from(depth) * 10)
}

/// Proximity bonus for user-content directories (0–120).
fn proximity_bonus(ctx: &NodeContext<'_>) -> u32 {
    if ctx.is_user_dir { 120 } else { 0 }
}

/// Context penalty (0–500, capped).
fn context_penalty(ctx: &NodeContext<'_>) -> u32 {
    let mut p = 0u32;

    // Trash is the strongest demotion
    if ctx.is_trash {
        p += 300;
    }

    if ctx.has_hidden_component && !ctx.is_trash {
        // Don't double-count .Trash as hidden
        p += 100;
    }
    if ctx.has_noise_component {
        p += 80;
    }
    if ctx.is_library_path {
        p += 80;
    }
    if ctx.is_system_path {
        p += 100;
    }
    p.min(500)
}

/// Get current unix timestamp in seconds.
#[inline]
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
