//! Service mode for XPC daemon integration
//!
//! Provides a long-running JSON-RPC service that:
//! - Maintains persistent file and USB watchers
//! - Keeps search index in memory for fast queries
//! - Handles automation execution and recovery
//! - Communicates via stdin/stdout JSON-RPC 2.0

mod handler;
mod logging;
mod protocol;
mod runner;

pub use handler::ServiceHandler;
pub use logging::{ServiceLogger, setup_service_logging};
pub use protocol::{JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
pub use runner::run_service;
