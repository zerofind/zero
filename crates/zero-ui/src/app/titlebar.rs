use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, Icon, IconName, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _},
};

use crate::actions::{
    GoBack, GoForward, OpenCommandPalette, ToggleAsk, ToggleSidebar, ToggleTerminal,
};
use crate::theme::{self, ICON_XS, RADIUS_SM, SPACE_XS};

use crate::models::ActiveView;

use super::ZeroApp;

impl ZeroApp {
    /// Title and icon for the centered toolbar label, based on active view.
    fn toolbar_title(&self, cx: &App) -> (String, IconName, Option<String>) {
        match &self.active_view {
            ActiveView::Cleanup => ("Cleanup".to_string(), IconName::Delete, None),
            ActiveView::CleanupDetail(idx) => {
                let group_name = self
                    .cleanup
                    .as_ref()
                    .map(|v| v.read(cx).group_name(*idx))
                    .unwrap_or("Detail");
                (
                    "Cleanup".to_string(),
                    IconName::Delete,
                    Some(group_name.to_string()),
                )
            }
            ActiveView::Settings => ("Settings".to_string(), IconName::Settings, None),
            ActiveView::Dedup => ("Duplicates".to_string(), IconName::Copy, None),
            ActiveView::Todo => ("Tasks".to_string(), IconName::Check, None),
            ActiveView::SecureErase => ("Secure Erase".to_string(), IconName::Delete, None),
            ActiveView::Automations => ("Automations".to_string(), IconName::Settings2, None),
            ActiveView::Editor(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Editor".to_string());
                (name, IconName::File, None)
            }
            ActiveView::DataTable(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Data".to_string());
                (name, IconName::LayoutDashboard, None)
            }
            ActiveView::FileBrowser(_) => {
                // Check if we're showing search results
                if let Some(query) = self
                    .file_browser
                    .as_ref()
                    .and_then(|b| b.read(cx).search_results_query().map(String::from))
                {
                    let label = format!("\u{201c}{query}\u{201d}");
                    return (label, IconName::Search, None);
                }

                let folder_name = self
                    .current_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Macintosh HD".to_string());
                let git_branch = self.file_browser.as_ref().and_then(|browser| {
                    let delegate = browser.read(cx).table_state.read(cx).delegate();
                    delegate
                        .git_info
                        .as_ref()
                        .and_then(|gi| gi.branch().map(String::from))
                });
                (folder_name, IconName::Folder, git_branch)
            }
        }
    }

    pub fn render_titlebar(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let can_back = self.can_go_back();
        let can_forward = self.can_go_forward();
        let sidebar_open = self.sidebar_open;

        let (title, title_icon, git_branch) = self.toolbar_title(cx);

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
                    // Centered title button (opens search)
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
                                Icon::new(title_icon)
                                    .with_size(ICON_XS)
                                    .text_color(muted.opacity(0.6)),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(muted)
                                    .child(SharedString::from(title)),
                            )
                            .when_some(git_branch, |el, branch| {
                                el.child(
                                    div()
                                        .text_color(muted.opacity(0.5))
                                        .child(SharedString::from(format!("({branch})"))),
                                )
                            })
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
                            .tooltip_with_action("Search", &OpenCommandPalette, None)
                            .icon(IconName::Search)
                            .text_color(muted)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_command_palette(window, cx);
                            })),
                    )
                    // Ask toggle
                    .child(
                        Button::new("ask-toggle")
                            .ghost()
                            .small()
                            .tooltip_with_action("Ask", &ToggleAsk, None)
                            .icon(IconName::Bot)
                            .text_color(if self.ask_open {
                                cx.theme().foreground
                            } else {
                                muted
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.ask_open = !this.ask_open;
                                if this.ask_open {
                                    this.ensure_ask(window, cx);
                                }
                                cx.notify();
                            })),
                    )
                    // Terminal toggle
                    .child(
                        Button::new("terminal-toggle")
                            .ghost()
                            .small()
                            .mr(px(4.0))
                            .tooltip_with_action("Terminal", &ToggleTerminal, None)
                            .icon(IconName::SquareTerminal)
                            .text_color(if self.terminal_open {
                                cx.theme().foreground
                            } else {
                                muted
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.terminal_open = !this.terminal_open;
                                if this.terminal_open {
                                    this.ensure_terminal(window, cx);
                                    if let Some(tv) = &this.terminal {
                                        tv.read(cx).focus_handle.focus(window);
                                    }
                                } else {
                                    this.focus_content(window, cx);
                                }
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}
