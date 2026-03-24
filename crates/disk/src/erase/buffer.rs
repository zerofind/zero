//! Aligned buffer for direct I/O operations
//!
//! When using direct I/O (`O_DIRECT` on Linux, `F_NOCACHE` on macOS),
//! the OS requires buffers to be aligned to the block size.
//! This module provides a safe wrapper around aligned memory allocation.

#![allow(unsafe_code)]

use std::ptr::slice_from_raw_parts_mut;

/// A buffer with memory aligned to a specific boundary
///
/// Required for direct I/O operations that bypass the OS cache.
/// The alignment is typically the device's block size (e.g., 4096 bytes).
pub struct AlignedBuffer {
    ptr: *mut u8,
    layout: std::alloc::Layout,
}

// SAFETY: AlignedBuffer owns its heap allocation exclusively (no aliasing).
// It contains only a raw pointer and a Layout — no thread-local or !Send state.
unsafe impl Send for AlignedBuffer {}

#[allow(dead_code)]
impl AlignedBuffer {
    /// Create a new aligned buffer
    ///
    /// # Arguments
    ///
    /// * `size` - Size of the buffer in bytes
    /// * `align` - Alignment boundary (must be power of 2)
    ///
    /// # Panics
    ///
    /// Panics if allocation fails or if align is not a power of 2.
    pub fn new(size: usize, align: usize) -> Self {
        assert!(align.is_power_of_two(), "Alignment must be power of 2");
        assert!(size > 0, "Size must be greater than 0");

        // SAFETY: size > 0 and align is a power of two (both asserted above),
        // satisfying Layout::from_size_align's preconditions.
        let layout = unsafe { std::alloc::Layout::from_size_align_unchecked(size, align) };

        // SAFETY: layout has non-zero size (asserted above).
        let ptr = unsafe { std::alloc::alloc(layout) };

        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }

        AlignedBuffer { ptr, layout }
    }

    /// Create a new aligned buffer with default alignment (4096 bytes)
    pub fn with_size(size: usize) -> Self {
        Self::new(size, 4096)
    }

    /// Fill the entire buffer with a single byte value
    pub fn fill(&mut self, value: u8) {
        // SAFETY: ptr is valid for layout.size() bytes (allocated in new()),
        // and &mut self guarantees exclusive access.
        unsafe {
            self.ptr.write_bytes(value, self.layout.size());
        }
    }

    /// Fill the entire buffer with zeros
    pub fn zero(&mut self) {
        self.fill(0);
    }

    /// Fill the entire buffer with ones (0xFF)
    pub fn ones(&mut self) {
        self.fill(0xFF);
    }

    /// Get the buffer as a mutable slice
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: ptr is valid for layout.size() bytes (allocated in new()),
        // and &mut self guarantees no aliasing.
        unsafe { &mut *slice_from_raw_parts_mut(self.ptr, self.layout.size()) }
    }

    /// Get the buffer as an immutable slice
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: ptr is valid for layout.size() bytes (allocated in new()).
        unsafe { std::slice::from_raw_parts(self.ptr, self.layout.size()) }
    }

    /// Get the size of the buffer
    pub fn len(&self) -> usize {
        self.layout.size()
    }

    /// Check if the buffer is empty (always false for valid buffers)
    pub fn is_empty(&self) -> bool {
        self.layout.size() == 0
    }

    /// Get the alignment of the buffer
    pub fn alignment(&self) -> usize {
        self.layout.align()
    }

    /// Verify that the pointer is properly aligned
    pub fn is_aligned(&self) -> bool {
        (self.ptr as usize).is_multiple_of(self.layout.align())
    }

    /// Get a raw pointer to the buffer (for FFI)
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    /// Get a mutable raw pointer to the buffer (for FFI)
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }
}

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        // SAFETY: ptr and layout are the same values from alloc() in new().
        // Drop runs exactly once.
        unsafe {
            std::alloc::dealloc(self.ptr, self.layout);
        }
    }
}

impl AsRef<[u8]> for AlignedBuffer {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for AlignedBuffer {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

#[cfg(test)]
#[path = "buffer_test.rs"]
mod buffer_test;
