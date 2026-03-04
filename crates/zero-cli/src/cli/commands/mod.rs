//! Command handlers for CLI operations

pub mod automation;
pub mod cloud;
pub mod delete;
pub mod diff;
pub mod disk;
pub mod dupes;
pub mod erase;
pub mod index;
pub mod scan;
pub mod search;
pub mod sync;
pub mod templates;
pub mod todo;
pub mod update;
pub mod verify;
pub mod watch;

pub use automation::*;
pub use cloud::*;
pub use delete::*;
pub use diff::*;
pub use disk::*;
pub use dupes::*;
pub use erase::*;
pub use index::*;
pub use scan::*;
pub use search::*;
pub use sync::*;
pub use templates::*;
pub use todo::*;
pub use update::*;
pub use watch::*;
