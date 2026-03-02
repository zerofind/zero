use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    ActiveTheme, Disableable as _, IconName, Sizable as _, TitleBar,
};

use crate::actions::{GoBack, GoForward, OpenCommandPalette, ToggleSidebar};
use crate::theme::{self, FONT_SIZE_CALLOUT};
use crate::ui::Breadcrumb;

use super::ZeroApp;

impl ZeroApp {
    pub fn render_titlebar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let can_back = self.can_go_back();
        let can_forward = self.can_go_forward();
        let sidebar_open = self.sidebar_open;

        let breadcrumb_path = self.current_path.clone();

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
                            .gap(px(2.0))
                            .child(
                                Button::new("nav-back")
                                    .ghost()
                                    .small()
                                    .tooltip_with_action("Back", &GoBack, None)
                                    .icon(IconName::ChevronLeft)
                                    .text_color(if can_back { muted } else { disabled_color })
                                    .disabled(!can_back)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.go_back(cx);
                                    })),
                            )
                            .child(
                                Button::new("nav-forward")
                                    .ghost()
                                    .small()
                                    .tooltip_with_action("Forward", &GoForward, None)
                                    .icon(IconName::ChevronRight)
                                    .text_color(
                                        if can_forward { muted } else { disabled_color },
                                    )
                                    .disabled(!can_forward)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.go_forward(cx);
                                    })),
                            ),
                    )
                    // Left spacer for centering
                    .child(div().flex_1())
                    // Centered breadcrumb navigation
                    .child({
                        let app = cx.entity().clone();
                        div()
                            .text_size(FONT_SIZE_CALLOUT)
                            .font_weight(FontWeight::MEDIUM)
                            .child(
                                Breadcrumb::new(breadcrumb_path)
                                    .on_navigate(move |path, _ev, _window, cx| {
                                        let path = path.clone();
                                        app.update(cx, |this, cx| {
                                            this.navigate_to(path, cx);
                                        });
                                    }),
                            )
                    })
                    // Right spacer for centering
                    .child(div().flex_1())
                    // Drives popover toggle
                    .child(
                        Button::new("drives-toggle")
                            .ghost()
                            .small()
                            .tooltip("Drives")
                            .icon(IconName::ExternalLink)
                            .text_color(muted)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.drives_popover_open = !this.drives_popover_open;
                                if this.drives_popover_open {
                                    this.ensure_drives_popover(window, cx);
                                }
                                cx.notify();
                            })),
                    )
                    // Search button
                    .child(
                        Button::new("search")
                            .ghost()
                            .small()
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
