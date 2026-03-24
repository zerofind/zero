//! Sanitization stages - pattern generators for disk wiping
//!
//! Each stage represents a single pass over the disk with a specific pattern:
//! - Zero fill (0x00)
//! - One fill (0xFF)
//! - Constant fill (any byte)
//! - Random fill (cryptographic random data)
//!
//! Adapted from the lethe crate (Apache 2.0 license).

use rand::RngCore;
use rand::SeedableRng;
use std::fmt::{Display, Formatter};

use super::buffer::AlignedBuffer;

/// Seed size for the random generator (`ChaCha8`)
const RANDOM_SEED_SIZE: usize = 32;

/// We use `ChaCha8` for fast cryptographic random generation
/// It's ~3x faster than reading from /dev/urandom while still being secure
type RandomGenerator = rand_chacha::ChaCha8Rng;

/// A single sanitization stage (one pass over the disk)
#[derive(Debug, Clone)]
pub enum Stage {
    /// Fill with a constant byte value
    Fill { value: u8 },

    /// Fill with cryptographic random data
    Random { seed: [u8; RANDOM_SEED_SIZE] },
}

impl Display for Stage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Stage::Fill { value } => write!(f, "fill with {value:#04X}"),
            Stage::Random { .. } => write!(f, "random fill"),
        }
    }
}

impl Stage {
    /// Create a stage that fills with a constant byte value
    pub fn constant(value: u8) -> Self {
        Stage::Fill { value }
    }

    /// Create a stage that fills with zeros (0x00)
    pub fn zero() -> Self {
        Self::constant(0x00)
    }

    /// Create a stage that fills with ones (0xFF)
    pub fn one() -> Self {
        Self::constant(0xFF)
    }

    /// Create a stage that fills with random data using a specific seed
    ///
    /// Using a fixed seed allows verification: we can regenerate the same
    /// random sequence to verify the write succeeded.
    pub fn random_with_seed(seed: [u8; RANDOM_SEED_SIZE]) -> Self {
        Stage::Random { seed }
    }

    /// Create a stage that fills with random data using a fresh random seed
    pub fn random() -> Self {
        let mut seed = [0u8; RANDOM_SEED_SIZE];
        rand::rng().fill_bytes(&mut seed);
        Self::random_with_seed(seed)
    }

    /// Create a streaming iterator that generates data for this stage
    ///
    /// # Arguments
    ///
    /// * `total_size` - Total number of bytes to generate
    /// * `block_size` - Size of each block (buffer size)
    /// * `start_from` - Starting position (for resume support)
    pub fn stream(&self, total_size: u64, block_size: usize, start_from: u64) -> StageStream {
        let mut buf = AlignedBuffer::new(block_size, block_size);

        let kind = match self {
            Stage::Fill { value } => {
                buf.fill(*value);
                StreamKind::Fill
            }
            Stage::Random { seed } => {
                let mut rng = RandomGenerator::from_seed(*seed);
                // Advance the RNG to the correct position for resume support
                // ChaCha uses 4-byte words, so divide position by 4
                rng.set_word_pos(u128::from(start_from / 4));
                StreamKind::Random { rng: Box::new(rng) }
            }
        };

        StageStream {
            kind,
            total_size,
            block_size,
            position: start_from,
            buf,
            current_chunk_size: 0,
            finished: false,
        }
    }

    /// Check if this is a random fill stage
    pub fn is_random(&self) -> bool {
        matches!(self, Stage::Random { .. })
    }

    /// Get a description of this stage
    pub fn description(&self) -> &'static str {
        match self {
            Stage::Fill { value: 0x00 } => "zero fill",
            Stage::Fill { value: 0xFF } => "ones fill",
            Stage::Fill { .. } => "pattern fill",
            Stage::Random { .. } => "random fill",
        }
    }
}

/// Internal stream state
enum StreamKind {
    /// Constant fill (buffer already filled, just reuse it)
    Fill,
    /// Random fill (need to generate new data for each block)
    Random { rng: Box<RandomGenerator> },
}

/// Streaming iterator that generates sanitization data block by block
///
/// This is memory-efficient: it only allocates one block-sized buffer
/// and reuses it for the entire operation.
pub struct StageStream {
    kind: StreamKind,
    total_size: u64,
    block_size: usize,
    position: u64,
    buf: AlignedBuffer,
    current_chunk_size: usize,
    finished: bool,
}

impl StageStream {
    /// Advance to the next block
    ///
    /// Returns `true` if there's more data, `false` if finished.
    pub fn advance(&mut self) -> bool {
        if self.finished || self.position >= self.total_size {
            self.finished = true;
            return false;
        }

        // Calculate chunk size (may be smaller for last block)
        let remaining = self.total_size - self.position;
        let chunk_size = std::cmp::min(self.block_size as u64, remaining) as usize;

        // Generate data for random fills
        if let StreamKind::Random { ref mut rng } = self.kind
            && let Some(slice) = self.buf.as_mut_slice().get_mut(..chunk_size)
        {
            rng.fill_bytes(slice);
        }

        self.current_chunk_size = chunk_size;
        self.position += chunk_size as u64;

        true
    }

    /// Get the current chunk of data
    ///
    /// Returns `None` if `advance()` hasn't been called or returned `false`.
    pub fn get(&self) -> Option<&[u8]> {
        if self.finished || self.current_chunk_size == 0 {
            None
        } else {
            self.buf.as_slice().get(..self.current_chunk_size)
        }
    }

    /// Get current position in the stream
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Get total size of the stream
    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    /// Check if the stream is finished
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Get progress as a fraction (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        if self.total_size == 0 {
            1.0
        } else {
            self.position as f64 / self.total_size as f64
        }
    }

    /// Reset the stream to a new position
    ///
    /// Useful for verification passes that need to regenerate the same data.
    pub fn reset_to(&mut self, position: u64) {
        self.position = position;
        self.finished = false;
        self.current_chunk_size = 0;

        // Reset RNG position for random streams
        if let StreamKind::Random { ref mut rng } = self.kind {
            rng.set_word_pos(u128::from(position / 4));
        }
    }
}

/// Iterator adapter for `StageStream`
impl Iterator for StageStream {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.advance() {
            self.get().map(<[u8]>::to_vec)
        } else {
            None
        }
    }
}

#[cfg(test)]
#[path = "stage_test.rs"]
mod stage_test;
