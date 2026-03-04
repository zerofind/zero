use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, Icon, IconName, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _},
};

use crate::actions::{GoBack, GoForward, OpenCommandPalette, ToggleSidebar};
use crate::theme::{self, ICON_XS, RADIUS_SM, SPACE_XS};

use super::ZeroApp;

impl ZeroApp {
    pub fn render_titlebar(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let can_back = self.can_go_back();
        let can_forward = self.can_go_forward();
        let sidebar_open = self.sidebar_open;

        // Current folder name (last path component)
        let folder_name = self
            .current_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Macintosh HD".to_string());

        let toggle_icon = if sidebar_open {
            IconName::PanelLeftClose
        } else {
            IconName::PanelLeftOpen
        };

        // Dim color for disabled nav buttons
        let disabled_color = cx.theme().muted_foreground.opacity(0.35);

        TitleBar::new()
            .bg(theme::content_bg(cx))
            .border_0()
            .when(sidebar_open, |el| el.pl(px(12.0)))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .w_full()
                    .items_center()
                    .gap(px(6.0))
                    // Sidebar toggle (always visible)
                    .child(
                        Button::new("sidebar-toggle")
                            .ghost()
                            .small()
                            .tooltip_with_action("Toggle sidebar", &ToggleSidebar, None)
                            .icon(toggle_icon)
                            .text_color(muted)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sidebar_open = !this.sidebar_open;
                                cx.notify();
                            })),
                    )
                    // Back / Forward — grouped tight
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(SPACE_XS)
                            .child(
                                Button::new("nav-back")
                                    .ghost()
                                    .small()
                                    .tooltip_with_action("Back", &GoBack, None)
                                    .icon(IconName::ChevronLeft)
                                    .text_color(if can_back { muted } else { disabled_color })
                                    .disabled(!can_back)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.go_back(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("nav-forward")
                                    .ghost()
                                    .small()
                                    .tooltip_with_action("Forward", &GoForward, None)
                                    .icon(IconName::ChevronRight)
                                    .text_color(if can_forward { muted } else { disabled_color })
                                    .disabled(!can_forward)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.go_forward(window, cx);
                                    })),
                            ),
                    )
                    // Left spacer for centering
                    .child(div().flex_1())
                    // Centered folder button (opens search)
                    .child(
                        div()
                            .id("folder-button")
                            .cursor_pointer()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.0))
                            .px(px(10.0))
                            .py(px(4.0))
                            .rounded(RADIUS_SM)
                            .hover(|s| s.bg(cx.theme().muted.opacity(0.5)))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_command_palette(window, cx);
                            }))
                            .child(
                                Icon::new(IconName::Folder)
                                    .with_size(ICON_XS)
                                    .text_color(muted.opacity(0.6)),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(muted)
                                    .child(SharedString::from(folder_name)),
                            )
                            .child(
                                Icon::new(IconName::ChevronDown)
                                    .with_size(px(9.0))
                                    .text_color(muted.opacity(0.4)),
                            ),
                    )
                    // Right spacer for centering
                    .child(div().flex_1())
                    // Search button
                    .child(
                        Button::new("search")
                            .ghost()
                            .small()
                            .mr(px(4.0))
                            .tooltip_with_action("Search", &OpenCommandPalette, None)
                            .icon(IconName::Search)
                            .text_color(muted)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_command_palette(window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}
