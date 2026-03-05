use tracing_subscriber::EnvFilter;

/// Debug filter for all zero-ecosystem crates.
/// Update this list when adding new crates.
pub const VERBOSE: &str = "zero=debug,zero_ui=debug,zero_watcher=debug,info";

/// Build a tracing [`EnvFilter`].
/// Checks `RUST_LOG` env first; falls back to the provided default.
pub fn env_filter(fallback: &str) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(fallback))
}
