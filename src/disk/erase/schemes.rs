//! Erasure schemes - predefined security levels for disk wiping
//!
//! This module provides three user-friendly security levels that map to
//! well-known sanitization standards:
//!
//! - **Quick**: Single zero pass - fast but recoverable with forensic tools
//! - **Normal**: Double random pass - good balance of speed and security
//! - **Strong**: VSITR 7-pass - alternating zeros/ones plus random, maximum security
//!
//! For most users, `Normal` is recommended. `Strong` takes significantly longer
//! but provides additional assurance against sophisticated recovery attempts.

use clap::ValueEnum;

use super::stage::Stage;
use std::fmt::{Display, Formatter};

/// Security level for disk erasure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum EraseLevel {
    /// Quick erase - single zero pass
    ///
    /// - Speed: Fastest
    /// - Security: Low (data recoverable with forensic tools)
    /// - Use case: Non-sensitive data, quick reformats
    #[value(alias = "fast", alias = "0")]
    Quick,

    /// Normal erase - double random pass
    ///
    /// - Speed: Moderate
    /// - Security: Good (blocks casual and most forensic recovery)
    /// - Use case: Personal data, general-purpose secure deletion
    #[default]
    #[value(alias = "default", alias = "1")]
    Normal,

    /// Strong erase - VSITR 7-pass (zero/one alternating + random)
    ///
    /// - Speed: Slow (7x longer than Quick)
    /// - Security: Maximum (meets government standards)
    /// - Use case: Highly sensitive data, compliance requirements
    #[value(alias = "secure", alias = "max", alias = "2")]
    Strong,
}

impl EraseLevel {
    /// Get the sanitization scheme for this security level
    pub fn scheme(&self) -> Scheme {
        match self {
            EraseLevel::Quick => Scheme::zero(),
            EraseLevel::Normal => Scheme::random2x(),
            EraseLevel::Strong => Scheme::vsitr(),
        }
    }

    /// Get a human-readable description of this level
    pub fn description(&self) -> &'static str {
        match self {
            EraseLevel::Quick => "Quick (single zero pass) - fast but not secure",
            EraseLevel::Normal => "Normal (double random pass) - recommended for most uses",
            EraseLevel::Strong => "Strong (VSITR 7-pass) - maximum security, takes longer",
        }
    }

    /// Get a short name for this level
    pub fn name(&self) -> &'static str {
        match self {
            EraseLevel::Quick => "quick",
            EraseLevel::Normal => "normal",
            EraseLevel::Strong => "strong",
        }
    }

    /// Get the number of passes for this level
    pub fn passes(&self) -> usize {
        self.scheme().stages.len()
    }

    /// Get estimated time multiplier relative to a single pass
    ///
    /// This accounts for verification passes as well.
    pub fn time_multiplier(&self) -> f64 {
        match self {
            EraseLevel::Quick => 1.0,
            EraseLevel::Normal => 2.0,
            EraseLevel::Strong => 7.0,
        }
    }

    /// Parse from string
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "quick" | "fast" | "0" => Some(EraseLevel::Quick),
            "normal" | "default" | "1" => Some(EraseLevel::Normal),
            "strong" | "secure" | "max" | "2" => Some(EraseLevel::Strong),
            _ => None,
        }
    }

    /// Get all available levels
    pub fn all() -> &'static [EraseLevel] {
        &[EraseLevel::Quick, EraseLevel::Normal, EraseLevel::Strong]
    }
}

impl Display for EraseLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// A sanitization scheme consisting of one or more stages
#[derive(Debug, Clone)]
pub struct Scheme {
    /// Human-readable name of the scheme
    pub name: String,

    /// Description of what this scheme does
    pub description: String,

    /// The stages (passes) to execute
    pub stages: Vec<Stage>,
}

impl Scheme {
    /// Create a new scheme with the given stages
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        stages: Vec<Stage>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            stages,
        }
    }

    /// Single zero fill (Quick)
    pub fn zero() -> Self {
        Self::new(
            "Zero Fill",
            "Single pass filling all bytes with zeros (0x00)",
            vec![Stage::zero()],
        )
    }

    /// Single random fill
    pub fn random() -> Self {
        Self::new(
            "Random Fill",
            "Single pass filling all bytes with cryptographic random data",
            vec![Stage::random()],
        )
    }

    /// Double random fill (Normal)
    pub fn random2x() -> Self {
        Self::new(
            "Double Random",
            "Two passes of cryptographic random data",
            vec![Stage::random(), Stage::random()],
        )
    }

    /// DoD 5220.22-M (3-pass)
    ///
    /// US Department of Defense standard:
    /// 1. Zero fill
    /// 2. One fill
    /// 3. Random fill
    pub fn dod() -> Self {
        Self::new(
            "DoD 5220.22-M",
            "US Department of Defense 3-pass standard: zeros, ones, random",
            vec![Stage::zero(), Stage::one(), Stage::random()],
        )
    }

    /// VSITR (7-pass) - German government standard (Strong)
    ///
    /// Also known as RCMP TSSIT OPS-II:
    /// 1. Zero fill
    /// 2. One fill
    /// 3. Zero fill
    /// 4. One fill
    /// 5. Zero fill
    /// 6. One fill
    /// 7. Random fill
    pub fn vsitr() -> Self {
        Self::new(
            "VSITR 7-Pass",
            "German VSITR standard: alternating zeros/ones (6 passes) plus random",
            vec![
                Stage::zero(),
                Stage::one(),
                Stage::zero(),
                Stage::one(),
                Stage::zero(),
                Stage::one(),
                Stage::random(),
            ],
        )
    }

    /// Gutmann 35-pass (overkill for modern drives)
    ///
    /// This is mainly of historical interest. For modern drives (especially SSDs),
    /// it provides no additional security over VSITR and takes 5x longer.
    pub fn gutmann() -> Self {
        let mut stages = Vec::with_capacity(35);

        // First 4 passes: random
        for _ in 0..4 {
            stages.push(Stage::random());
        }

        // Passes 5-31: specific patterns (simplified)
        let patterns: [u8; 27] = [
            0x55, 0xAA, 0x92, 0x49, 0x24, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
            0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x92, 0x49, 0x24, 0x6D, 0xB6, 0xDB,
        ];
        for pattern in patterns {
            stages.push(Stage::constant(pattern));
        }

        // Last 4 passes: random
        for _ in 0..4 {
            stages.push(Stage::random());
        }

        Self::new(
            "Gutmann 35-Pass",
            "Peter Gutmann's 35-pass method (historical, overkill for modern drives)",
            stages,
        )
    }

    /// Get the number of stages in this scheme
    pub fn pass_count(&self) -> usize {
        self.stages.len()
    }

    /// Estimate time to complete based on disk size and write speed
    ///
    /// Returns estimated seconds.
    pub fn estimate_time(&self, size_bytes: u64, write_speed_mbps: u64) -> u64 {
        if write_speed_mbps == 0 {
            return 0;
        }

        let bytes_per_second = write_speed_mbps * 1_000_000;
        let seconds_per_pass = size_bytes / bytes_per_second;

        seconds_per_pass * self.stages.len() as u64
    }

    /// Format estimated time as human-readable string
    pub fn estimate_time_display(&self, size_bytes: u64, write_speed_mbps: u64) -> String {
        let seconds = self.estimate_time(size_bytes, write_speed_mbps);

        if seconds < 60 {
            format!("{} seconds", seconds)
        } else if seconds < 3600 {
            format!("{} minutes", seconds / 60)
        } else {
            let hours = seconds / 3600;
            let minutes = (seconds % 3600) / 60;
            if minutes > 0 {
                format!("{} hours {} minutes", hours, minutes)
            } else {
                format!("{} hours", hours)
            }
        }
    }
}

impl Display for Scheme {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({} passes)", self.name, self.stages.len())
    }
}

#[cfg(test)]
#[path = "schemes_test.rs"]
mod schemes_test;
