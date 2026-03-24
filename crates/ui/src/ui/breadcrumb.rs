use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName, Sizable as _};

use crate::theme::{self, FONT_SIZE_BODY, ICON_XS};

type PathHandler = Rc<dyn Fn(&PathBuf, &ClickEvent, &mut Window, &mut App)>;

/// Clickable path segments: folder > folder > folder
#[derive(IntoElement)]
pub struct Breadcrumb {
    path: PathBuf,
    on_navigate: Option<PathHandler>,
}

impl Breadcrumb {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            on_navigate: None,
        }
    }

    pub fn on_navigate(
        mut self,
        handler: impl Fn(&PathBuf, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_navigate = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Breadcrumb {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        let mut segments: Vec<(String, PathBuf)> = Vec::new();
        let mut current = self.path.as_path();

        loop {
            let name = if current == Path::new("/") {
                "/".to_string()
            } else {
                current.file_name().map_or_else(
                    || current.to_string_lossy().to_string(),
                    |n| n.to_string_lossy().to_string(),
                )
            };
            segments.push((name, current.to_path_buf()));
            match current.parent() {
                Some(parent) if parent != current => current = parent,
                _ => break,
            }
        }
        segments.reverse();

        // Only show last 3 segments to avoid overflow
        if segments.len() > 3 {
            segments = segments.split_off(segments.len() - 3);
        }

        let handler = self.on_navigate;
        let last_idx = segments.len().saturating_sub(1);

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .text_size(FONT_SIZE_BODY)
            .children(
                segments
                    .into_iter()
                    .enumerate()
                    .map(move |(i, (name, full_path))| {
                        let is_last = i == last_idx;
                        let handler = handler.clone();
                        let text_color = if is_last {
                            cx.theme().foreground
                        } else {
                            muted
                        };
                        let brand = theme::brand_color(cx);

                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .when(i > 0, |el| {
                                el.child(
                                    Icon::new(IconName::ChevronRight)
                                        .with_size(ICON_XS)
                                        .text_color(muted),
                                )
                            })
                            .child(
                                div()
                                    .id(SharedString::from(format!("crumb-{i}")))
                                    .cursor_pointer()
                                    .text_color(text_color)
                                    .when(is_last, |el| el.font_weight(FontWeight::BOLD))
                                    .hover(move |s| s.text_color(brand))
                                    .when_some(handler, |el, h| {
                                        let path = full_path.clone();
                                        el.on_click(move |event, window, cx| {
                                            h(&path, event, window, cx);
                                        })
                                    })
                                    .child(SharedString::from(name)),
                            )
                    }),
            )
    }
}
