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
    category: Option<SharedString>,
    action_label: Option<SharedString>,
    shortcut: Option<SharedString>,
    is_drillable: bool,
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
            category: None,
            action_label: None,
            shortcut: None,
            is_drillable: false,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn category(mut self, category: impl Into<SharedString>) -> Self {
        self.category = Some(category.into());
        self
    }

    pub fn action_label(mut self, label: impl Into<SharedString>) -> Self {
        self.action_label = Some(label.into());
        self
    }

    #[allow(dead_code)]
    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    #[allow(dead_code)]
    pub fn drillable(mut self, drillable: bool) -> Self {
        self.is_drillable = drillable;
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
                    .when(!self.path.is_empty(), |el| {
                        el.child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(self.path),
                        )
                    }),
            )
            // Right side: category + shortcut badge + action label
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_2()
                    .items_center()
                    .when_some(self.category, |el, cat| {
                        el.child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .child(cat),
                        )
                    })
                    .when_some(self.shortcut, |el, sc| {
                        el.child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .px_1p5()
                                .py_0p5()
                                .rounded(RADIUS)
                                .bg(cx.theme().muted)
                                .child(sc),
                        )
                    })
                    .when(self.selected && self.is_drillable, |el| {
                        el.child(
                            div()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .child("Search >"),
                        )
                    })
                    .when(
                        self.selected && !self.is_drillable && self.action_label.is_some(),
                        |el| {
                            el.child(
                                div()
                                    .text_size(FONT_SIZE_CAPTION)
                                    .text_color(muted)
                                    .child(self.action_label.unwrap()),
                            )
                        },
                    ),
            )
    }
}
