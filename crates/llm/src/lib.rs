pub mod agent;
pub mod config;
mod error;
mod prompt;
mod provider;
mod tools;

use search::IndexManager;
use std::sync::{Arc, RwLock};

/// Shared, lazily-populated index reference.
///
/// Starts as `None`. When the search index finishes loading, the UI layer
/// calls `set_index()` which populates it. Tools that need the index
/// gracefully degrade when it's `None`.
pub type SharedIndex = Arc<RwLock<Option<IndexManager>>>;

pub use agent::{StreamEvent, ZeroAgent};
pub use config::{LlmConfig, ModelInfo};
pub use error::{LlmError, friendly_error};
