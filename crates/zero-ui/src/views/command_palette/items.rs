use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, h_flex};

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS};
use crate::ui::FileIcon;

/// A single result row in the command palette.
#[derive(IntoElement)]
pub struct PaletteItem {
    id: ElementId,
    name: SharedString,
    path: SharedString,
    extension: Option<String>,
    is_dir: bool,
    selected: bool,
}

impl PaletteItem {
    pub fn new(
        id: impl Into<ElementId>,
        name: impl Into<SharedString>,
        path: impl Into<SharedString>,
        extension: Option<String>,
        is_dir: bool,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            path: path.into(),
            extension,
            is_dir,
            selected: false,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl RenderOnce for PaletteItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        h_flex()
            .id(self.id)
            .w_full()
            .px_3()
            .py_1p5()
            .gap_2p5()
            .items_center()
            .rounded(RADIUS)
            .cursor_pointer()
            .when(self.selected, |el| el.bg(theme::surface_active(cx)))
            .hover(|s| s.bg(theme::surface_hover(cx)))
            .child(FileIcon::new(self.extension.as_deref(), self.is_dir))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(FONT_SIZE_BODY)
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(self.name),
                    )
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(self.path),
                    ),
            )
    }
}
