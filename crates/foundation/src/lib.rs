/// The version of the zero crate, for use by dependent crates like zero-ui.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod dirs;
pub mod logging;
pub mod output;
pub mod util;
