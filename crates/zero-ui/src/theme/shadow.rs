/// Shadow levels — documents the design intent, maps to GPUI presets.
///
/// GPUI only supports `.shadow_md()` and `.shadow_lg()` (no custom shadow params).
/// Values match Swift DS.Shadow for reference:
///   panel:   blur 12, y-offset 4, opacity 0.25
///   popover: blur 30, y-offset 10, opacity 0.3
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ShadowLevel {
    /// Toast notifications, alerts — maps to `.shadow_md()`
    Panel,
    /// Modals, command palette, popovers — maps to `.shadow_lg()`
    Popover,
}
