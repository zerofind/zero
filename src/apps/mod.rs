//! App & package inventory for macOS.
//!
//! Provides a unified view of installed software across multiple sources:
//! macOS .app bundles, Homebrew formulae/casks, and more in the future.
//!
//! Phase 1: Discovery + metadata (name, version, path, timestamps)
//! Phase 2: Integrity checking (codesign, notarization)
//! Phase 3: Version tracking & vulnerability detection

mod inventory;

pub use inventory::{PackageInfo, PackageSource};
