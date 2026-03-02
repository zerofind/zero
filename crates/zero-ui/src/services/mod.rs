pub mod apps;
pub mod search;

pub use apps::AppService;
pub use search::{SearchEvent, SearchService};

use gpui::*;

/// Central hub holding all service entities.
///
/// Created once in `ZeroApp::new()`, services are then passed to views
/// via their constructors. Views call methods on `Entity<SearchService>` etc.
/// instead of holding `Arc<RwLock<IndexManager>>` directly.
pub struct ServiceHub {
    pub search: Entity<SearchService>,
    pub apps: Entity<AppService>,
}

impl ServiceHub {
    pub fn new(cx: &mut App) -> Self {
        let search = cx.new(SearchService::new);
        let apps = cx.new(AppService::new);

        Self { search, apps }
    }
}
