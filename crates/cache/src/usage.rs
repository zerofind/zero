//! Frequency tracking for file opens
//!
//! Tracks which files users open most frequently and recently,
//! providing a scoring bonus for search results. Stored in `ControlDb`
//! (collection 5), survives index rebuilds.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Usage data for a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUsage {
    /// Number of times the file has been opened
    pub open_count: u32,
    /// Unix timestamp of most recent open
    pub last_opened: u64,
}

const MAX_ENTRIES: usize = 10_000;

/// In-memory store of file open frequency data
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageStore {
    entries: BTreeMap<String, FileUsage>,
}

impl UsageStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a file open. Returns true if this is a new entry.
    pub fn record_open(&mut self, path: &str, now: u64) -> bool {
        if let Some(usage) = self.entries.get_mut(path) {
            usage.open_count += 1;
            usage.last_opened = now;
            false
        } else {
            self.entries.insert(
                path.to_string(),
                FileUsage {
                    open_count: 1,
                    last_opened: now,
                },
            );
            // Prune if we exceed the limit
            if self.entries.len() > MAX_ENTRIES {
                self.prune(now);
            }
            true
        }
    }

    /// Frequency bonus for search scoring (0–150).
    ///
    /// Based on `open_count` and recency of last open.
    /// Uses log scale for count to avoid runaway scores.
    pub fn frequency_bonus(&self, path: &str, now: u64) -> u32 {
        let Some(usage) = self.entries.get(path) else {
            return 0;
        };
        let age_days = now.saturating_sub(usage.last_opened) / 86400;
        let recency_factor = 1.0 / (1.0 + age_days as f32 * 0.1);
        let count_factor = (usage.open_count as f32).ln_1p();
        let raw = recency_factor * count_factor * 50.0;
        (raw as u32).min(150)
    }

    /// Prune to `MAX_ENTRIES`, keeping highest-scoring entries.
    pub fn prune(&mut self, now: u64) {
        if self.entries.len() <= MAX_ENTRIES {
            return;
        }

        // Score each entry and sort by score descending
        let mut scored: Vec<(String, f32)> = self
            .entries
            .iter()
            .map(|(path, usage)| {
                let age_days = now.saturating_sub(usage.last_opened) / 86400;
                let recency = 1.0 / (1.0 + age_days as f32 * 0.1);
                let count = (usage.open_count as f32).ln_1p();
                (path.clone(), recency * count)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Keep top MAX_ENTRIES
        let to_remove: Vec<String> = scored
            .into_iter()
            .skip(MAX_ENTRIES)
            .map(|(path, _)| path)
            .collect();

        for path in to_remove {
            self.entries.remove(&path);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
