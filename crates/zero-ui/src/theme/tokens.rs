use gpui::{Pixels, px};

// -- Icon Sizes ---------------------------------------------------------------
// InterfaceIcon scale (matches Swift DS)

/// Compact: close/remove buttons, indicators (12px)
pub const ICON_XS: Pixels = px(12.0);
/// Default: standalone icons in UI (14px)
pub const ICON_SM: Pixels = px(14.0);
/// Sidebar rows, bookmarks, navigation (16px)
pub const ICON_MD: Pixels = px(16.0);
/// Emphasized, prominent icons (18px)
pub const ICON_LG: Pixels = px(18.0);

// AppIcon scale (file type icons, hero/display)

pub const APP_ICON_SM: Pixels = px(16.0);
/// Empty state hero icons (24px)
pub const APP_ICON_MD: Pixels = px(24.0);
pub const APP_ICON_LG: Pixels = px(32.0);

// -- Typography ---------------------------------------------------------------

/// Tag labels, tiny text (10px)
pub const FONT_SIZE_MICRO: Pixels = px(10.0);
/// Section headers, secondary (11px)
pub const FONT_SIZE_CAPTION: Pixels = px(11.0);
/// Default body text (13px)
pub const FONT_SIZE_BODY: Pixels = px(13.0);
/// Breadcrumbs, callouts (14px)
pub const FONT_SIZE_CALLOUT: Pixels = px(14.0);
/// Section titles (16px)
pub const FONT_SIZE_TITLE: Pixels = px(16.0);
/// Modal titles (18px)
pub const FONT_SIZE_HEADING: Pixels = px(18.0);
/// Onboarding, hero headings (20px)
pub const FONT_SIZE_DISPLAY: Pixels = px(20.0);

// -- Corner Radii -------------------------------------------------------------

/// Tags, tiny badges (3px)
pub const RADIUS_XS: Pixels = px(3.0);
/// Small cards, pills (6px)
pub const RADIUS_SM: Pixels = px(6.0);
/// Default: buttons, rows, inputs (8px)
pub const RADIUS: Pixels = px(8.0);
/// Medium containers (10px)
pub const RADIUS_MD: Pixels = px(10.0);
/// Modals, content panels (12px)
pub const RADIUS_LG: Pixels = px(12.0);
/// Fully rounded pills (9999px)
pub const RADIUS_FULL: Pixels = px(9999.0);

// -- Spacing ------------------------------------------------------------------

/// Tight: button groups, dividers (2px)
pub const SPACE_XS: Pixels = px(2.0);
/// Small gaps, compact padding (4px)
pub const SPACE_SM: Pixels = px(4.0);
/// Default padding (8px)
pub const SPACE_MD: Pixels = px(8.0);
/// Section padding (12px)
pub const SPACE_LG: Pixels = px(12.0);
/// Large section gaps (16px)
pub const SPACE_XL: Pixels = px(16.0);

// Legacy aliases (keep existing imports working)
pub const PADDING_SM: Pixels = SPACE_SM;
pub const PADDING_MD: Pixels = SPACE_MD;
pub const PADDING_LG: Pixels = SPACE_LG;

// -- Sidebar ------------------------------------------------------------------

pub const SIDEBAR_WIDTH: Pixels = px(220.0);
/// macOS traffic light clearance
pub const SIDEBAR_TOP_INSET: Pixels = px(38.0);
pub const SIDEBAR_ROW_HEIGHT: Pixels = px(32.0);
pub const BOOKMARK_TILE_HEIGHT: Pixels = px(46.0);

// -- Titlebar -----------------------------------------------------------------

pub const TITLEBAR_HEIGHT: Pixels = px(34.0);
pub const TOOLBAR_BUTTON_SIZE: Pixels = px(26.0);

// -- Content ------------------------------------------------------------------

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

// -- Grid View ----------------------------------------------------------------

pub const GRID_CELL_WIDTH: Pixels = px(80.0);
pub const GRID_CELL_HEIGHT: Pixels = px(100.0);
pub const GRID_ICON_SIZE: Pixels = px(48.0);

// -- Progress -----------------------------------------------------------------

pub const PROGRESS_BAR_HEIGHT: Pixels = px(3.0);
pub const PROGRESS_BAR_RADIUS: Pixels = px(2.0);
