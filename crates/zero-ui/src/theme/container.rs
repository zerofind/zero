use gpui::{px, Pixels};

use super::{content_bg, popover_bg, sidebar_bg, RADIUS_LG};

/// Container depth levels — matches Swift ContainerLevel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ContainerLevel {
    /// Base window layer, radius 0
    Window,
    /// Sidebar panel, radius 0
    Sidebar,
    /// Main content area, radius 12
    Content,
    /// Floating overlay (popover, dialog), radius 12
    Popover,
}

/// Corner radius for a given container level.
#[allow(dead_code)]
pub fn container_radius(level: ContainerLevel) -> Pixels {
    match level {
        ContainerLevel::Window | ContainerLevel::Sidebar => px(0.0),
        ContainerLevel::Content | ContainerLevel::Popover => RADIUS_LG,
    }
}

/// Background color for a given container level.
#[allow(dead_code)]
pub fn container_bg(level: ContainerLevel, cx: &gpui::App) -> gpui::Hsla {
    match level {
        ContainerLevel::Window => content_bg(cx),
        ContainerLevel::Sidebar => sidebar_bg(cx),
        ContainerLevel::Content => content_bg(cx),
        ContainerLevel::Popover => popover_bg(cx),
    }
}
