//! Contiguous path storage arena
//!
//! Replaces millions of individual `String` heap allocations with a single
//! `Vec<u8>` buffer. Each path is stored as UTF-8 bytes and referenced by
//! `(offset: u32, len: u16)` — 6 bytes instead of 24 bytes per String.
//!
//! For 2.5M files at ~60 bytes average path length, this uses ~150MB of
//! contiguous memory vs ~400MB with individual String allocations
//! (150MB data + 250MB allocator fragmentation).

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

    /// Push a path into the arena, returning (offset, len)
    ///
    /// Reuses freed slots when possible (best-fit).
    pub fn push(&mut self, path: &str) -> (u32, u16) {
        let bytes = path.as_bytes();
        let len = bytes.len().min(u16::MAX as usize) as u16;

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
            self.data[offset as usize..offset as usize + len as usize]
                .copy_from_slice(&bytes[..len as usize]);
            // If the slot was larger, put the remainder back
            if free_len > len {
                let remainder_offset = offset + len as u32;
                let remainder_len = free_len - len;
                self.free_list.push((remainder_offset, remainder_len));
            }
            return (offset, len);
        }

        // Append to the end
        let offset = self.data.len() as u32;
        self.data.extend_from_slice(&bytes[..len as usize]);
        (offset, len)
    }

    /// Get a path string from the arena (zero-copy)
    ///
    /// # Safety
    /// All data pushed via `push()` is valid UTF-8, so `from_utf8_unchecked` is safe.
    #[inline]
    pub fn get(&self, offset: u32, len: u16) -> &str {
        let start = offset as usize;
        let end = start + len as usize;
        // SAFETY: all input to push() is &str (valid UTF-8)
        unsafe { std::str::from_utf8_unchecked(&self.data[start..end]) }
    }

    /// Mark bytes as free for reuse
    pub fn remove(&mut self, offset: u32, len: u16) {
        if len > 0 {
            self.free_list.push((offset, len));
        }
    }

    /// Total bytes allocated in the data buffer
    #[allow(dead_code)]
    pub fn total_bytes(&self) -> usize {
        self.data.len()
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
