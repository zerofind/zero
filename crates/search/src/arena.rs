//! Contiguous path storage arena
//!
//! Replaces millions of individual `String` heap allocations with a single
//! `Vec<u8>` buffer. Each path is stored as UTF-8 bytes and referenced by
//! `(offset: u32, len: u16)` — 6 bytes instead of 24 bytes per String.
//!
//! For 2.5M files at ~60 bytes average path length, this uses ~150MB of
//! contiguous memory vs ~400MB with individual String allocations
//! (150MB data + 250MB allocator fragmentation).

/// Maximum path length in bytes (u16::MAX = 65535)
pub const MAX_PATH_LEN: usize = u16::MAX as usize;

/// Maximum arena capacity in bytes (u32::MAX = ~4GB)
const MAX_ARENA_BYTES: usize = u32::MAX as usize;

/// Contiguous UTF-8 path storage
#[derive(Debug, Clone)]
pub struct PathArena {
    data: Vec<u8>,
    free_list: Vec<(u32, u16)>,
}

impl PathArena {
    /// Create a new empty arena
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            free_list: Vec::new(),
        }
    }

    /// Create an arena with pre-allocated capacity
    pub fn with_capacity(bytes: usize) -> Self {
        Self {
            data: Vec::with_capacity(bytes),
            free_list: Vec::new(),
        }
    }

    /// Push a path into the arena, returning (offset, len).
    ///
    /// Returns `None` if the path exceeds `MAX_PATH_LEN` (65535 bytes)
    /// or the arena would exceed its 4GB capacity.
    ///
    /// Reuses freed slots when possible (best-fit).
    pub fn push(&mut self, path: &str) -> Option<(u32, u16)> {
        let bytes = path.as_bytes();
        if bytes.len() > MAX_PATH_LEN {
            return None;
        }
        let len = bytes.len() as u16;

        // Try to reuse a free slot (best-fit: smallest slot that fits)
        if let Some(best) = self
            .free_list
            .iter()
            .enumerate()
            .filter(|(_, (_, free_len))| *free_len >= len)
            .min_by_key(|(_, (_, free_len))| *free_len)
            .map(|(i, _)| i)
        {
            let (offset, free_len) = self.free_list.swap_remove(best);
            // Write into the reused slot
            // Bounds guaranteed: best-fit search above ensures free_len >= len,
            // and offset + free_len is within self.data from the original push.
            #[allow(clippy::indexing_slicing)]
            self.data[offset as usize..offset as usize + len as usize].copy_from_slice(bytes);
            // If the slot was larger, put the remainder back on the free list,
            // but only if the remainder is large enough to be useful (>=4 bytes).
            // This bounds free-list growth under churn.
            let remainder = free_len - len;
            if remainder >= 4 {
                let remainder_offset = offset + u32::from(len);
                self.free_list.push((remainder_offset, remainder));
            }
            return Some((offset, len));
        }

        // Check u32 offset capacity before appending
        if self.data.len() + bytes.len() > MAX_ARENA_BYTES {
            return None;
        }

        let offset = self.data.len() as u32;
        self.data.extend_from_slice(bytes);
        Some((offset, len))
    }

    /// Get a path string from the arena (zero-copy)
    ///
    /// Callers must only pass `(offset, len)` values returned by `push()`.
    /// The data at those coordinates is always valid UTF-8 because `push()` only
    /// accepts `&str`. The slice bounds are valid because `push()` extends
    /// `self.data` to cover `offset..offset+len` or reuses a free slot of at
    /// least `len` bytes.
    #[inline]
    #[allow(unsafe_code)]
    #[allow(clippy::indexing_slicing)]
    pub fn get(&self, offset: u32, len: u16) -> &str {
        let start = offset as usize;
        let end = start + len as usize;
        // SAFETY: all input to push() is &str (valid UTF-8), and (offset, len)
        // pairs are only produced by push() which guarantees data[start..end]
        // is within bounds and contains the original UTF-8 bytes.
        unsafe { std::str::from_utf8_unchecked(&self.data[start..end]) }
    }

    /// Mark bytes as free for reuse.
    ///
    /// Adjacent free regions are coalesced to reduce fragmentation.
    pub fn remove(&mut self, offset: u32, len: u16) {
        if len == 0 {
            return;
        }

        let new_start = offset;
        let new_end = offset + u32::from(len);
        let mut merged_start = new_start;
        let mut merged_end = new_end;

        // Coalesce with any adjacent free regions
        let mut i = 0;
        while i < self.free_list.len() {
            // Index is valid: loop condition guarantees i < self.free_list.len()
            #[allow(clippy::indexing_slicing)]
            let (f_off, f_len) = self.free_list[i];
            let f_end = f_off + u32::from(f_len);

            if f_end == merged_start {
                // This free region is immediately before us
                merged_start = f_off;
                self.free_list.swap_remove(i);
                // Don't increment — swapped element needs checking
            } else if f_off == merged_end {
                // This free region is immediately after us
                merged_end = f_end;
                self.free_list.swap_remove(i);
            } else {
                i += 1;
            }
        }

        let merged_len = merged_end - merged_start;
        // Only track if it fits in u16 (always true unless arena has a bug)
        if let Ok(merged_u16) = u16::try_from(merged_len) {
            self.free_list.push((merged_start, merged_u16));
        }
    }

    /// Total bytes allocated in the data buffer
    #[allow(dead_code)]
    pub fn total_bytes(&self) -> usize {
        self.data.len()
    }

    /// Number of entries in the free list
    #[allow(dead_code)]
    pub fn free_list_len(&self) -> usize {
        self.free_list.len()
    }

    /// Shrink the data buffer to fit
    pub fn shrink_to_fit(&mut self) {
        self.data.shrink_to_fit();
        self.free_list.shrink_to_fit();
    }
}

impl Default for PathArena {
    fn default() -> Self {
        Self::new()
    }
}
