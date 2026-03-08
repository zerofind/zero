pub mod apps;
pub mod llm;
pub mod mcp;
pub mod search;

pub use apps::AppService;
pub use llm::{LlmEvent, LlmService};
pub use mcp::McpService;
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
    pub mcp: Entity<McpService>,
    pub llm: Entity<LlmService>,
}

impl ServiceHub {
    pub fn new(cx: &mut App) -> Self {
        let search = cx.new(SearchService::new);
        let apps = cx.new(AppService::new);
        let mcp = cx.new(McpService::new);
        let llm = cx.new(LlmService::new);

        Self {
            search,
            apps,
            mcp,
            llm,
        }
    }

    /// Create services without triggering any file system access.
    /// Call `init()` later once Full Disk Access is confirmed.
    pub fn new_deferred(cx: &mut App) -> Self {
        let search = cx.new(SearchService::new_deferred);
        let apps = cx.new(AppService::new_deferred);
        let mcp = cx.new(McpService::new);
        let llm = cx.new(LlmService::new);

        Self {
            search,
            apps,
            mcp,
            llm,
        }
    }

    /// Initialize services after FDA is confirmed.
    pub fn init(&self, cx: &mut App) {
        self.search.update(cx, |svc, cx| svc.activate(cx));
        self.apps.update(cx, |svc, cx| svc.load(cx));
    }
}
