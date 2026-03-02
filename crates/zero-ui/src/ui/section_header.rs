use gpui::*;
use gpui_component::ActiveTheme;

use crate::theme::FONT_SIZE_CAPTION;

/// Muted uppercase section label used in the sidebar.
#[derive(IntoElement)]
pub struct SectionHeader {
    label: SharedString,
}

impl SectionHeader {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl RenderOnce for SectionHeader {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .text_size(FONT_SIZE_CAPTION)
            .text_color(cx.theme().muted_foreground)
            .child(self.label)
    }
}
