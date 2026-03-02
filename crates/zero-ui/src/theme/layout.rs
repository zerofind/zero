use gpui::{Pixels, px};

// -- Sidebar ------------------------------------------------------------------

#[allow(dead_code)]
pub const SIDEBAR_WIDTH: Pixels = px(220.0);
/// Traffic light clearance on macOS
#[allow(dead_code)]
pub const SIDEBAR_TOP_INSET: Pixels = px(38.0);

// -- Toolbar / Titlebar -------------------------------------------------------

#[allow(dead_code)]
pub const TITLEBAR_HEIGHT: Pixels = px(34.0);
#[allow(dead_code)]
pub const TOOLBAR_BUTTON_SIZE: Pixels = px(26.0);

// -- Content ------------------------------------------------------------------

#[allow(dead_code)]
pub const CONTENT_INSET: Pixels = px(10.0);

// -- Modals -------------------------------------------------------------------

/// Alert, drives popover
pub const MODAL_SM_WIDTH: Pixels = px(320.0);
/// Confirm dialog
pub const MODAL_MD_WIDTH: Pixels = px(400.0);
/// Automations modal
pub const MODAL_LG_WIDTH: Pixels = px(500.0);
/// Command palette
pub const MODAL_PALETTE_WIDTH: Pixels = px(640.0);

// -- Progress -----------------------------------------------------------------

pub const PROGRESS_BAR_HEIGHT: Pixels = px(3.0);
pub const PROGRESS_BAR_RADIUS: Pixels = px(2.0);
