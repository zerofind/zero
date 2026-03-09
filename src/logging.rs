use tracing_subscriber::EnvFilter;

/// Debug filter for all zero-ecosystem crates.
/// Update this list when adding new crates.
pub const VERBOSE: &str = "zero=debug,zero_ui=debug,zero_llm=debug,zero_watcher=debug,alacritty_terminal=warn,vte=warn,info";

/// Build a tracing [`EnvFilter`].
/// Checks `RUST_LOG` env first; falls back to the provided default.
pub fn env_filter(fallback: &str) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(fallback))
}

/// Initialize the global tracing subscriber.
///
/// Safe to call multiple times — uses `try_init` so the first caller wins.
/// The `tracing-log` feature bridges the `log` crate (used by vte/alacritty)
/// through our tracing filter, so `vte=warn` actually silences vte's DEBUG logs.
pub fn init(fallback_filter: &str) {
    let filter = env_filter(fallback_filter);
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
}
