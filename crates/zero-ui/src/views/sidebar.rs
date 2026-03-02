use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme, Icon, IconName, Sizable as _,
};

use crate::models::ActiveView;
use crate::session::Settings;
use crate::theme::{self, PADDING_LG, PADDING_MD, RADIUS};
use crate::ui::{SectionHeader, SidebarRow};

/// Events emitted by the sidebar to the app root.
pub enum SidebarEvent {
    Navigate(ActiveView),
    OpenPath(PathBuf),
    RemoveBookmark(PathBuf),
    EjectDrive(PathBuf),
}

impl EventEmitter<SidebarEvent> for AppSidebar {}

pub struct AppSidebar {
    active_view: ActiveView,
    current_path: PathBuf,
    bookmarks: Vec<PathBuf>,
    drives: Vec<DriveEntry>,
    #[allow(dead_code)]
    focus_handle: FocusHandle,
}

struct DriveEntry {
    name: String,
    path: PathBuf,
    is_external: bool,
}

impl AppSidebar {
    pub fn new(
        active_view: ActiveView,
        current_path: PathBuf,
        bookmarks: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let drives = Self::discover_drives();

        Self {
            active_view,
            current_path,
            bookmarks,
            drives,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn set_active_view(&mut self, view: ActiveView, cx: &mut Context<Self>) {
        self.active_view = view;
        cx.notify();
    }

    pub fn set_current_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.current_path = path;
        cx.notify();
    }

    pub fn add_bookmark(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.bookmarks.contains(&path) {
            self.bookmarks.push(path);
            cx.notify();
        }
    }

    pub fn remove_bookmark(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        self.bookmarks.retain(|b| b != path);
        cx.notify();
    }

    pub fn refresh_drives(&mut self, cx: &mut Context<Self>) {
        self.drives = Self::discover_drives();
        cx.notify();
    }

    fn discover_drives() -> Vec<DriveEntry> {
        let mut drives = Vec::new();

        drives.push(DriveEntry {
            name: "Macintosh HD".into(),
            path: PathBuf::from("/"),
            is_external: false,
        });

        if let Ok(entries) = std::fs::read_dir("/Volumes") {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name == "Macintosh HD" {
                    continue;
                }
                drives.push(DriveEntry {
                    name,
                    path,
                    is_external: true,
                });
            }
        }

        drives
    }

    fn render_nav_section(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let items: &[(ActiveView, IconName, &str)] = &[
            (ActiveView::FileBrowser, IconName::Folder, "Files"),
            (ActiveView::Cleanup, IconName::Delete, "Cleanup"),
            (ActiveView::Dedup, IconName::File, "Duplicates"),
            (ActiveView::SecureErase, IconName::Delete, "Erase"),
            (ActiveView::Automations, IconName::Settings, "Auto"),
            (ActiveView::Todo, IconName::Check, "Tasks"),
            (ActiveView::Settings, IconName::Settings, "Settings"),
        ];

        let muted = cx.theme().muted_foreground;

        h_flex()
            .gap(px(6.0))
            .children(items.iter().map(|(view, icon, label)| {
                let is_active = self.active_view == *view;
                let view = view.clone();

                let mut btn = Button::new(SharedString::from(format!("nav-{label}")))
                    .ghost()
                    .compact()
                    .small()
                    .icon(icon.clone())
                    .tooltip(SharedString::from(*label))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(SidebarEvent::Navigate(view.clone()));
                    }));

                if is_active {
                    btn = btn
                        .text_color(theme::selection_color())
                        .bg(theme::selection_active_bg(cx));
                } else {
                    btn = btn.text_color(muted);
                }

                btn
            }))
    }

    fn render_bookmarks(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;

        let items: Vec<_> = self
            .bookmarks
            .iter()
            .enumerate()
            .map(|(i, path)| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());

                let icon = match name.as_str() {
                    "Desktop" => IconName::LayoutDashboard,
                    "Documents" => IconName::File,
                    "Downloads" => IconName::ArrowDown,
                    _ => IconName::Folder,
                };

                let is_active = self.current_path.starts_with(path);
                let is_system = Settings::is_system_bookmark(path);
                let p = path.clone();
                let remove_path = path.clone();

                let icon_color = if is_active {
                    theme::selection_color()
                } else {
                    muted
                };

                let bg = if is_active {
                    theme::selection_active_bg(cx)
                } else {
                    theme::surface_hover(cx)
                };

                div()
                    .id(SharedString::from(format!("bookmark-{i}")))
                    .group(SharedString::from(format!("bm-group-{i}")))
                    .relative()
                    .flex_1()
                    .h(px(52.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(RADIUS)
                    .cursor_pointer()
                    .bg(bg)
                    .hover(|s| s.bg(theme::surface_active(cx)))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(SidebarEvent::OpenPath(p.clone()));
                    }))
                    .child(
                        Icon::new(icon)
                            .with_size(px(20.0))
                            .text_color(icon_color),
                    )
                    // Remove button (visible on hover, only for non-system bookmarks)
                    .when(!is_system, |el| {
                        el.child(
                            div()
                                .absolute()
                                .top_0()
                                .right_0()
                                .invisible()
                                .group_hover(
                                    SharedString::from(format!("bm-group-{i}")),
                                    |s| s.visible(),
                                )
                                .child(
                                    Button::new(SharedString::from(format!("rm-bm-{i}")))
                                        .ghost()
                                        .compact()
                                        .xsmall()
                                        .icon(IconName::Close)
                                        .tooltip("Remove Bookmark")
                                        .on_click(cx.listener(move |_, _, _, cx| {
                                            cx.emit(SidebarEvent::RemoveBookmark(
                                                remove_path.clone(),
                                            ));
                                        })),
                                ),
                        )
                    })
            })
            .collect();

        v_flex()
            .gap(px(6.0))
            .child(SectionHeader::new("BOOKMARKS"))
            .child(h_flex().gap_2().children(items))
    }

    fn render_drives(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let items: Vec<_> = self
            .drives
            .iter()
            .enumerate()
            .map(|(i, drive)| {
                let icon = if drive.is_external {
                    IconName::ExternalLink
                } else {
                    IconName::Folder
                };
                let p = drive.path.clone();
                let eject_path = drive.path.clone();
                let is_external = drive.is_external;

                div()
                    .id(SharedString::from(format!("drive-wrap-{i}")))
                    .group(SharedString::from(format!("drv-group-{i}")))
                    .relative()
                    .child(
                        SidebarRow::new(
                            SharedString::from(format!("drive-{i}")),
                            SharedString::from(drive.name.clone()),
                            icon,
                        )
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(SidebarEvent::OpenPath(p.clone()));
                        })),
                    )
                    .when(is_external, |el| {
                        el.child(
                            div()
                                .absolute()
                                .top(px(4.0))
                                .right(px(4.0))
                                .invisible()
                                .group_hover(
                                    SharedString::from(format!("drv-group-{i}")),
                                    |s| s.visible(),
                                )
                                .child(
                                    Button::new(SharedString::from(format!("eject-{i}")))
                                        .ghost()
                                        .compact()
                                        .xsmall()
                                        .icon(IconName::Minus)
                                        .tooltip("Eject")
                                        .on_click(cx.listener(move |_, _, _, cx| {
                                            cx.emit(SidebarEvent::EjectDrive(
                                                eject_path.clone(),
                                            ));
                                        })),
                                ),
                        )
                    })
            })
            .collect();

        v_flex()
            .gap_0p5()
            .child(SectionHeader::new("STORAGES"))
            .children(items)
    }
}

impl Render for AppSidebar {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .id("sidebar")
            .w(px(220.0))
            .h_full()
            .flex_shrink_0()
            .bg(theme::sidebar_bg(cx))
            .pt(px(38.0)) // below traffic lights
            .px(PADDING_MD)
            .pb(PADDING_LG)
            .gap_4()
            .overflow_y_scroll()
            .child(self.render_nav_section(cx))
            .child(self.render_bookmarks(cx))
            .child(self.render_drives(cx))
    }
}
