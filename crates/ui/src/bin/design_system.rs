use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext, Application, Bounds, Context, Div, Entity, FocusHandle, Focusable,
    FontWeight, Hsla, InteractiveElement, IntoElement, KeyBinding, ParentElement, Render,
    SharedString, StatefulInteractiveElement, StyleRefinement, Styled, Window, WindowBounds,
    WindowOptions, actions, div, hsla, point, px, size,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Root, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    switch::Switch,
    tag::Tag,
    theme::{Theme, ThemeMode},
    v_flex,
};
use ui::theme::{
    self, FONT_SIZE_BODY, FONT_SIZE_CALLOUT, FONT_SIZE_CAPTION, FONT_SIZE_TITLE, ICON_SM, RADIUS,
    RADIUS_LG, SIDEBAR_WIDTH, SPACE_XS, brand_color, card_border, content_bg, sidebar_bg,
    surface_hover,
};
use ui::ui::{
    BannerData, BannerKind, Breadcrumb, EmptyState, FileIcon, ProgressBanner, SectionHeader,
    SidebarRow, StatusPill,
};

actions!(design_system, [Quit]);

// -- Sections -----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShowcaseSection {
    FileBrowser,
    Navigation,
    Settings,
}

fn section_info(section: ShowcaseSection) -> (&'static str, &'static str) {
    match section {
        ShowcaseSection::FileBrowser => (
            "File Browser",
            "File icons, status pills, breadcrumbs, and empty states.",
        ),
        ShowcaseSection::Navigation => (
            "Navigation",
            "Sidebar rows, section headers, and progress banners.",
        ),
        ShowcaseSection::Settings => (
            "Settings",
            "Form controls, toggles, and interactive inputs.",
        ),
    }
}

// -- Main ---------------------------------------------------------------------

fn main() {
    let app = Application::new().with_assets(gpui_component_assets::Assets);

    app.run(move |cx| {
        gpui_component::init(cx);
        theme::init_zero_theme(cx);
        gpui_component::theme::Theme::change(gpui_component::theme::ThemeMode::Dark, None, cx);

        cx.bind_keys([
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-q", Quit, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("alt-f4", Quit, None),
        ]);

        cx.on_action(|_: &Quit, cx| cx.quit());

        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    titlebar: Some(TitleBar::title_bar_options()),
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(100.0), px(100.0)),
                        size: size(px(820.0), px(600.0)),
                    })),
                    window_min_size: Some(size(px(720.0), px(500.0))),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| DesignSystem::new(window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}

// -- DesignSystem view --------------------------------------------------------

struct DesignSystem {
    section: ShowcaseSection,
    focus_handle: FocusHandle,
    // Settings state
    search_input: Entity<InputState>,
    notifications: bool,
}

impl DesignSystem {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input =
            cx.new(|cx| InputState::new(window, cx).default_value("/Users/demo/Documents"));

        Self {
            section: ShowcaseSection::FileBrowser,
            focus_handle: cx.focus_handle(),
            search_input,
            notifications: true,
        }
    }

    // -- Sidebar --------------------------------------------------------------

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = cx.theme().sidebar_foreground;
        let muted = cx.theme().muted_foreground;
        let active_section = self.section;

        v_flex()
            .h_full()
            .w(SIDEBAR_WIDTH)
            .flex_shrink_0()
            .bg(sidebar_bg(cx))
            .text_color(fg)
            // Traffic-light spacer
            .child(div().h(px(34.0)))
            // Title
            .child(
                div()
                    .px_4()
                    .pb_4()
                    .child(
                        div()
                            .text_size(FONT_SIZE_BODY)
                            .font_weight(FontWeight::BOLD)
                            .child("Zero Design System"),
                    )
                    .child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .child("Component showcase"),
                    ),
            )
            // Nav items
            .child(
                v_flex()
                    .px_3()
                    .gap_0p5()
                    .child(nav_item(
                        "nav-files",
                        "File Browser",
                        IconName::Folder,
                        active_section == ShowcaseSection::FileBrowser,
                        ShowcaseSection::FileBrowser,
                        cx,
                    ))
                    .child(nav_item(
                        "nav-navigation",
                        "Navigation",
                        IconName::Menu,
                        active_section == ShowcaseSection::Navigation,
                        ShowcaseSection::Navigation,
                        cx,
                    ))
                    .child(nav_item(
                        "nav-settings",
                        "Settings",
                        IconName::Settings,
                        active_section == ShowcaseSection::Settings,
                        ShowcaseSection::Settings,
                        cx,
                    )),
            )
    }

    // -- Content --------------------------------------------------------------

    fn render_content(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let (title, subtitle) = section_info(self.section);

        v_flex()
            .id("ds-content")
            .size_full()
            .bg(content_bg(cx))
            .rounded_tl(px(5.0))
            .overflow_hidden()
            // Sticky header
            .child(
                div()
                    .flex_shrink_0()
                    .w_full()
                    .flex()
                    .justify_center()
                    .pt(px(34.0))
                    .pb_4()
                    .child(
                        v_flex()
                            .w_full()
                            .max_w(px(680.0))
                            .px_6()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(FONT_SIZE_TITLE)
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(fg)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(FONT_SIZE_BODY)
                                    .text_color(muted)
                                    .child(subtitle),
                            ),
                    ),
            )
            // Scrollable body
            .child(
                v_flex().id("ds-scroll").flex_1().overflow_y_scroll().child(
                    div().w_full().flex().justify_center().child(
                        v_flex()
                            .w_full()
                            .max_w(px(680.0))
                            .px_6()
                            .pt_2()
                            .pb_12()
                            .gap_6()
                            .child(self.render_section(cx)),
                    ),
                ),
            )
    }

    fn render_section(&mut self, cx: &mut Context<Self>) -> AnyElement {
        match self.section {
            ShowcaseSection::FileBrowser => self.render_file_browser(cx).into_any_element(),
            ShowcaseSection::Navigation => self.render_navigation(cx).into_any_element(),
            ShowcaseSection::Settings => self.render_settings(cx).into_any_element(),
        }
    }

    // -- File Browser section -------------------------------------------------

    #[allow(clippy::unused_self)]
    fn render_file_browser(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;

        v_flex()
            .gap_6()
            // File icons
            .child(
                v_flex().gap_3().child(group_label("File Icons", fg)).child(
                    h_flex().gap_4().flex_wrap().children(
                        [
                            (Some("rs"), false, "main.rs"),
                            (Some("py"), false, "script.py"),
                            (Some("jpg"), false, "photo.jpg"),
                            (Some("mp4"), false, "video.mp4"),
                            (Some("pdf"), false, "report.pdf"),
                            (Some("zip"), false, "archive.zip"),
                            (None, true, "Documents"),
                        ]
                        .into_iter()
                        .map(|(ext, is_dir, name)| {
                            h_flex()
                                .gap_2()
                                .items_center()
                                .py_1()
                                .px_2()
                                .rounded(RADIUS)
                                .bg(surface_hover(cx))
                                .child(FileIcon::new(ext, is_dir))
                                .child(div().text_size(FONT_SIZE_BODY).text_color(fg).child(name))
                        }),
                    ),
                ),
            )
            // Status pills
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Status Pills", fg))
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .child(StatusPill::new("Synced", theme::success_color(cx)))
                            .child(StatusPill::new("Pending", hsla(0.130, 0.80, 0.50, 1.0)))
                            .child(StatusPill::new("Error", hsla(0.000, 0.70, 0.55, 1.0)))
                            .child(StatusPill::new("2 copies", brand_color(cx)))
                            .child(StatusPill::new("Duplicate", hsla(0.830, 0.60, 0.55, 1.0))),
                    ),
            )
            // Breadcrumb
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Breadcrumb", fg))
                    .child(Breadcrumb::new("/Users/demo/Documents/Projects/zero")),
            )
            // Empty state
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Empty State", fg))
                    .child(
                        div()
                            .h(px(160.0))
                            .rounded(RADIUS_LG)
                            .border_1()
                            .border_color(card_border(cx))
                            .child(
                                EmptyState::new(IconName::Folder, "No files found")
                                    .subtitle("Try a different search or location"),
                            ),
                    ),
            )
            // Tags (from gpui-component)
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Tags", fg))
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .child(Tag::primary().child("Primary"))
                            .child(Tag::success().child("Synced"))
                            .child(Tag::warning().child("Pending"))
                            .child(Tag::danger().child("Error"))
                            .child(Tag::info().child("Indexed"))
                            .child(Tag::secondary().child("Archived")),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .text_color(muted)
                            .text_size(FONT_SIZE_BODY)
                            .child("Outline:")
                            .child(Tag::primary().outline().small().child("v0.1"))
                            .child(Tag::danger().outline().small().child("Breaking"))
                            .child(Tag::success().outline().small().child("Stable")),
                    ),
            )
    }

    // -- Navigation section ---------------------------------------------------

    #[allow(clippy::unused_self)]
    fn render_navigation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let fg = cx.theme().foreground;

        v_flex()
            .gap_6()
            // Sidebar rows
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Sidebar Rows", fg))
                    .child(
                        v_flex()
                            .gap_0p5()
                            .w(px(260.0))
                            .child(
                                SidebarRow::new("ds-fav", "Favorites", IconName::Star).active(true),
                            )
                            .child(SidebarRow::new("ds-docs", "Documents", IconName::File))
                            .child(SidebarRow::new("ds-dl", "Downloads", IconName::ArrowDown))
                            .child(SidebarRow::new(
                                "ds-desktop",
                                "Desktop",
                                IconName::LayoutDashboard,
                            )),
                    ),
            )
            // Section headers
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Section Headers", fg))
                    .child(
                        v_flex()
                            .gap_1()
                            .w(px(260.0))
                            .child(SectionHeader::new("FAVORITES"))
                            .child(
                                SidebarRow::new("ds-h-home", "Home", IconName::Globe).active(true),
                            )
                            .child(SidebarRow::new("ds-h-recent", "Recents", IconName::Redo))
                            .child(SectionHeader::new("DEVICES"))
                            .child(SidebarRow::new(
                                "ds-h-ssd",
                                "Macintosh HD",
                                IconName::Folder,
                            ))
                            .child(SidebarRow::new(
                                "ds-h-usb",
                                "USB Drive",
                                IconName::ExternalLink,
                            )),
                    ),
            )
            // Progress banner
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Progress Banner", fg))
                    .child(
                        v_flex()
                            .gap_2()
                            .child(ProgressBanner::new(BannerData {
                                kind: BannerKind::Sync,
                                message: "Syncing Documents folder...".to_string(),
                                bytes_done: 1_536_000_000,
                                bytes_total: 4_200_000_000,
                                files_done: 12_400,
                                files_total: 34_000,
                                phase: None,
                                indeterminate: false,
                                on_cancel: None,
                            }))
                            .child(ProgressBanner::new(BannerData {
                                kind: BannerKind::Index,
                                message: "Building search index...".to_string(),
                                bytes_done: 0,
                                bytes_total: 0,
                                files_done: 1_700_000,
                                files_total: 0,
                                phase: Some("Scanning... 1,700,000 files".to_string()),
                                indeterminate: true,
                                on_cancel: None,
                            })),
                    ),
            )
            // Buttons (from gpui-component)
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Buttons", fg))
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .child(
                                Button::new("ds-btn-primary")
                                    .primary()
                                    .rounded(RADIUS)
                                    .label("Sync Now"),
                            )
                            .child(
                                Button::new("ds-btn-secondary")
                                    .rounded(RADIUS)
                                    .label("Browse"),
                            )
                            .child(
                                Button::new("ds-btn-danger")
                                    .danger()
                                    .rounded(RADIUS)
                                    .label("Delete"),
                            )
                            .child(
                                Button::new("ds-btn-ghost")
                                    .ghost()
                                    .rounded(RADIUS)
                                    .label("Cancel"),
                            )
                            .child(
                                Button::new("ds-btn-outline")
                                    .outline()
                                    .rounded(RADIUS)
                                    .label("Export"),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(cx.theme().muted_foreground)
                            .child("Small:")
                            .child(
                                Button::new("ds-btn-sm-1")
                                    .primary()
                                    .small()
                                    .rounded(RADIUS)
                                    .label("Add"),
                            )
                            .child(
                                Button::new("ds-btn-sm-2")
                                    .small()
                                    .rounded(RADIUS)
                                    .label("Edit"),
                            )
                            .child(
                                Button::new("ds-btn-sm-3")
                                    .danger()
                                    .small()
                                    .rounded(RADIUS)
                                    .label("Remove"),
                            ),
                    ),
            )
            // Color swatches
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Brand Colors", fg))
                    .child(
                        h_flex()
                            .gap_3()
                            .child(color_swatch("Brand", brand_color(cx), cx))
                            .child(color_swatch("Success", theme::success_color(cx), cx))
                            .child(color_swatch("Selection", theme::selection_color(cx), cx)),
                    ),
            )
    }

    // -- Settings section -----------------------------------------------------

    fn render_settings(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = cx.theme().is_dark();
        let border = card_border(cx);
        let label_color = cx.theme().muted_foreground;
        let value_color = cx.theme().foreground;
        let notifications = self.notifications;

        v_flex()
            .gap_6()
            // Appearance
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Appearance", value_color))
                    .child(
                        v_flex()
                            .rounded(RADIUS_LG)
                            .border_1()
                            .border_color(border)
                            .overflow_hidden()
                            .child(setting_row(
                                "Dark mode",
                                "Toggle between light and dark theme.",
                                Switch::new("ds-dark-mode")
                                    .checked(is_dark)
                                    .on_click(cx.listener(|_, checked: &bool, window, cx| {
                                        let mode = if *checked {
                                            ThemeMode::Dark
                                        } else {
                                            ThemeMode::Light
                                        };
                                        Theme::change(mode, Some(window), cx);
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                                label_color,
                                value_color,
                                border,
                                true,
                            ))
                            .child(setting_row(
                                "Notifications",
                                "Show desktop notifications for sync events.",
                                Checkbox::new("ds-notifs")
                                    .checked(notifications)
                                    .on_click(cx.listener(|this, checked: &bool, _window, _cx| {
                                        this.notifications = *checked;
                                    }))
                                    .into_any_element(),
                                label_color,
                                value_color,
                                border,
                                false,
                            )),
                    ),
            )
            // Sync
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Sync", value_color))
                    .child(
                        v_flex()
                            .rounded(RADIUS_LG)
                            .border_1()
                            .border_color(border)
                            .overflow_hidden()
                            .child(setting_row(
                                "Default source",
                                "Path to scan when no source is specified.",
                                Input::new(&self.search_input)
                                    .w(px(220.0))
                                    .into_any_element(),
                                label_color,
                                value_color,
                                border,
                                true,
                            ))
                            .child(setting_row(
                                "Verify copies",
                                "Hash-verify all transferred files.",
                                div()
                                    .text_size(FONT_SIZE_BODY)
                                    .text_color(value_color)
                                    .child("Enabled")
                                    .into_any_element(),
                                label_color,
                                value_color,
                                border,
                                false,
                            )),
                    ),
            )
            // Danger zone
            .child(
                v_flex()
                    .gap_3()
                    .child(group_label("Danger Zone", value_color))
                    .child(
                        v_flex()
                            .rounded(RADIUS_LG)
                            .border_1()
                            .border_color(border)
                            .overflow_hidden()
                            .child(setting_row(
                                "Clear cache",
                                "Remove all cached checksums and indexes.",
                                Button::new("ds-clear-cache")
                                    .danger()
                                    .small()
                                    .rounded(RADIUS)
                                    .label("Clear")
                                    .into_any_element(),
                                label_color,
                                value_color,
                                border,
                                true,
                            ))
                            .child(setting_row(
                                "Reset settings",
                                "Restore all settings to their defaults.",
                                Button::new("ds-reset")
                                    .danger()
                                    .small()
                                    .rounded(RADIUS)
                                    .label("Reset")
                                    .into_any_element(),
                                label_color,
                                value_color,
                                border,
                                false,
                            )),
                    ),
            )
    }
}

impl Focusable for DesignSystem {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for DesignSystem {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("design-system")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(self.render_sidebar(cx))
            .child(self.render_content(cx))
    }
}

// -- Helpers ------------------------------------------------------------------

fn nav_item(
    id: &str,
    label: &str,
    icon: IconName,
    active: bool,
    section: ShowcaseSection,
    cx: &mut Context<DesignSystem>,
) -> impl IntoElement {
    let hover_bg = surface_hover(cx);
    let active_bg = if active {
        Some(theme::brand_active_bg(cx))
    } else {
        None
    };
    let text_color = if active {
        brand_color(cx)
    } else {
        cx.theme().sidebar_foreground
    };

    div()
        .id(SharedString::from(id.to_string()))
        .flex()
        .flex_row()
        .gap_2p5()
        .items_center()
        .px_2()
        .py_1()
        .rounded(RADIUS)
        .text_size(FONT_SIZE_BODY)
        .text_color(text_color)
        .cursor_pointer()
        .when_some(active_bg, gpui::Styled::bg)
        .hover(move |s: StyleRefinement| s.bg(hover_bg))
        .child(Icon::new(icon).with_size(ICON_SM))
        .child(label.to_string())
        .on_click(cx.listener(move |this, _, _, cx| {
            this.section = section;
            cx.notify();
        }))
}

fn setting_row(
    title: &str,
    description: &str,
    control: AnyElement,
    label_color: Hsla,
    value_color: Hsla,
    border_color: Hsla,
    has_border_bottom: bool,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_5()
        .py_4()
        .gap_6()
        .when(has_border_bottom, |el| {
            el.border_b_1().border_color(border_color)
        })
        .child(
            v_flex()
                .gap(SPACE_XS)
                .flex_1()
                .child(
                    div()
                        .text_size(FONT_SIZE_BODY)
                        .text_color(value_color)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(FONT_SIZE_CAPTION)
                        .text_color(label_color)
                        .child(description.to_string()),
                ),
        )
        .child(div().flex_shrink_0().child(control))
}

fn group_label(text: &str, color: Hsla) -> Div {
    div()
        .text_size(FONT_SIZE_CALLOUT)
        .font_weight(FontWeight::BOLD)
        .text_color(color)
        .child(text.to_string())
}

fn color_swatch(name: &str, color: Hsla, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1()
        .items_center()
        .child(div().size(px(40.0)).rounded(RADIUS).bg(color))
        .child(
            div()
                .text_size(FONT_SIZE_CAPTION)
                .text_color(cx.theme().muted_foreground)
                .child(name.to_string()),
        )
}
