use gpui::*;

use crate::theme::{FONT_SIZE_CAPTION, RADIUS_SM};

/// Small colored badge — e.g. "2 copies" or "synced".
#[derive(IntoElement)]
pub struct StatusPill {
    label: SharedString,
    color: Hsla,
}

impl StatusPill {
    pub fn new(label: impl Into<SharedString>, color: Hsla) -> Self {
        Self {
            label: label.into(),
            color,
        }
    }
}

impl RenderOnce for StatusPill {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .px_2()
            .py(px(1.0))
            .rounded(RADIUS_SM)
            .bg(self.color.opacity(0.15))
            .text_size(FONT_SIZE_CAPTION)
            .text_color(self.color)
            .child(self.label)
    }
}
