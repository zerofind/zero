//! Hash algorithm definitions

use clap::ValueEnum;
use std::fmt;

/// Supported hash algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum HashAlgorithm {
    /// Blake3 - cryptographic hash, very fast, use for integrity/tripwire
    Blake3,
    /// XXH3 - non-cryptographic hash, extremely fast, default for sync/dedup/comparison
    #[default]
    Xxh3,
}

impl HashAlgorithm {
    /// Size of the hash output in bytes
    pub fn digest_size(&self) -> usize {
        match self {
            Self::Blake3 => 32, // 256 bits
            Self::Xxh3 => 16,   // 128 bits
        }
    }

    /// Human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Blake3 => "blake3",
            Self::Xxh3 => "xxh3",
        }
    }

    /// Whether this is a cryptographic hash (suitable for integrity verification)
    pub fn is_cryptographic(&self) -> bool {
        match self {
            Self::Blake3 => true,
            Self::Xxh3 => false,
        }
    }
}

impl fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl TryFrom<&str> for HashAlgorithm {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "blake3" => Ok(Self::Blake3),
            "xxh3" => Ok(Self::Xxh3),
            _ => Err(format!(
                "Unknown hash algorithm: '{s}'. Supported: blake3, xxh3"
            )),
        }
    }
}

#[cfg(test)]
#[path = "algorithm_test.rs"]
mod algorithm_test;
