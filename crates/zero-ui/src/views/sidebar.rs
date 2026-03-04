use std::path::PathBuf;

use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _, h_flex,
    menu::{ContextMenuExt as _, PopupMenuItem},
    v_flex,
};

use crate::models::ActiveView;
use crate::session::Settings;
use crate::theme::{
    self, BOOKMARK_TILE_HEIGHT, ICON_MD, PADDING_LG, PADDING_MD, RADIUS, SIDEBAR_TOP_INSET,
    SIDEBAR_WIDTH,
};
use crate::ui::{SectionHeader, SidebarRow};

/// Events emitted by the sidebar to the app root.
pub enum SidebarEvent {
    Navigate(ActiveView),
    OpenPath(PathBuf),
    RemoveBookmark(PathBuf),
    UnpinBookmark(PathBuf),
    PinBookmark(PathBuf),
    EjectDrive(PathBuf),
    FindDuplicates(PathBuf),
    IndexLocation(PathBuf),
}

impl EventEmitter<SidebarEvent> for AppSidebar {}

pub struct AppSidebar {
    active_view: ActiveView,
    current_path: PathBuf,
    /// Pinned bookmarks shown as icon tiles.
    pinned_bookmarks: Vec<PathBuf>,
    /// Regular bookmarks shown as rows in the BOOKMARKS section.
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
        pinned_bookmarks: Vec<PathBuf>,
        bookmarks: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let drives = Self::discover_drives();

        Self {
            active_view,
            current_path,
            pinned_bookmarks,
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
        if !self.bookmarks.contains(&path) && !self.pinned_bookmarks.contains(&path) {
            self.bookmarks.push(path);
            cx.notify();
        }
    }

    pub fn remove_bookmark(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        self.pinned_bookmarks.retain(|b| b != path);
        self.bookmarks.retain(|b| b != path);
        cx.notify();
    }

    pub fn unpin_bookmark(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        if let Some(pos) = self.pinned_bookmarks.iter().position(|b| b == path) {
            let removed = self.pinned_bookmarks.remove(pos);
            if !self.bookmarks.contains(&removed) {
                self.bookmarks.push(removed);
            }
            cx.notify();
        }
    }

    pub fn pin_bookmark(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        if let Some(pos) = self.bookmarks.iter().position(|b| b == path) {
            let removed = self.bookmarks.remove(pos);
            if !self.pinned_bookmarks.contains(&removed) {
                self.pinned_bookmarks.push(removed);
            }
            cx.notify();
        }
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

    fn render_settings_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_active = self.active_view == ActiveView::Settings;

        SidebarRow::new("nav-settings", "Settings", IconName::Settings)
            .active(is_active)
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(SidebarEvent::Navigate(ActiveView::Settings));
            }))
    }

    fn render_pinned_bookmarks(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let sidebar = cx.entity().clone();

        let items: Vec<_> = self
            .pinned_bookmarks
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

                let icon_color = if is_active {
                    theme::selection_color(cx)
                } else {
                    muted
                };

                let bg = if is_active {
                    theme::selection_active_bg(cx)
                } else {
                    gpui::transparent_black()
                };

                let ctx_open = path.clone();
                let ctx_unpin = path.clone();
                let ctx_remove = path.clone();
                let sidebar_ref = sidebar.clone();

                div()
                    .id(SharedString::from(format!("pinned-{i}")))
                    .flex_1()
                    .h(BOOKMARK_TILE_HEIGHT)
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
                    .child(Icon::new(icon).with_size(ICON_MD).text_color(icon_color))
                    .context_menu(move |menu, _window, _cx| {
                        let sb = sidebar_ref.clone();

                        let mut m = menu
                            .item(
                                PopupMenuItem::new("Open")
                                    .icon(IconName::FolderOpen)
                                    .on_click({
                                        let path = ctx_open.clone();
                                        let sb = sb.clone();
                                        move |_, _, cx| {
                                            sb.update(cx, |_, cx| {
                                                cx.emit(SidebarEvent::OpenPath(path.clone()));
                                            });
                                        }
                                    }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new("Unpin from Quick Access")
                                    .icon(IconName::StarOff)
                                    .on_click({
                                        let path = ctx_unpin.clone();
                                        let sb = sb.clone();
                                        move |_, _, cx| {
                                            sb.update(cx, |_, cx| {
                                                cx.emit(SidebarEvent::UnpinBookmark(path.clone()));
                                            });
                                        }
                                    }),
                            );

                        if !is_system {
                            m = m.separator().item(
                                PopupMenuItem::new("Remove Bookmark")
                                    .icon(IconName::Delete)
                                    .on_click({
                                        let path = ctx_remove.clone();
                                        let sb = sb;
                                        move |_, _, cx| {
                                            sb.update(cx, |_, cx| {
                                                cx.emit(SidebarEvent::RemoveBookmark(path.clone()));
                                            });
                                        }
                                    }),
                            );
                        }

                        m
                    })
            })
            .collect();

        h_flex().gap_2().children(items)
    }

    fn render_regular_bookmarks(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.bookmarks.is_empty() {
            return div();
        }

        let sidebar = cx.entity().clone();

        let items: Vec<_> = self
            .bookmarks
            .iter()
            .enumerate()
            .map(|(i, path)| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());

                let icon = bookmark_icon(&name);
                let is_active = self.current_path.starts_with(path);
                let is_system = Settings::is_system_bookmark(path);
                let p = path.clone();
                let ctx_open = path.clone();
                let ctx_pin = path.clone();
                let ctx_remove = path.clone();
                let sidebar_ref = sidebar.clone();

                div()
                    .id(SharedString::from(format!("reg-bm-{i}")))
                    .child(
                        SidebarRow::new(
                            SharedString::from(format!("reg-bm-row-{i}")),
                            SharedString::from(name),
                            icon,
                        )
                        .active(is_active)
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(SidebarEvent::OpenPath(p.clone()));
                        })),
                    )
                    .context_menu(move |menu, _window, _cx| {
                        let sb = sidebar_ref.clone();

                        let mut m = menu
                            .item(
                                PopupMenuItem::new("Open")
                                    .icon(IconName::FolderOpen)
                                    .on_click({
                                        let path = ctx_open.clone();
                                        let sb = sb.clone();
                                        move |_, _, cx| {
                                            sb.update(cx, |_, cx| {
                                                cx.emit(SidebarEvent::OpenPath(path.clone()));
                                            });
                                        }
                                    }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new("Pin to Quick Access")
                                    .icon(IconName::Star)
                                    .on_click({
                                        let path = ctx_pin.clone();
                                        let sb = sb.clone();
                                        move |_, _, cx| {
                                            sb.update(cx, |_, cx| {
                                                cx.emit(SidebarEvent::PinBookmark(path.clone()));
                                            });
                                        }
                                    }),
                            );

                        if !is_system {
                            m = m.separator().item(
                                PopupMenuItem::new("Remove Bookmark")
                                    .icon(IconName::Delete)
                                    .on_click({
                                        let path = ctx_remove.clone();
                                        let sb = sb;
                                        move |_, _, cx| {
                                            sb.update(cx, |_, cx| {
                                                cx.emit(SidebarEvent::RemoveBookmark(path.clone()));
                                            });
                                        }
                                    }),
                            );
                        }

                        m
                    })
            })
            .collect();

        v_flex()
            .gap_0p5()
            .child(SectionHeader::new("BOOKMARKS"))
            .children(items)
    }

    fn any_bookmark_matches(&self) -> bool {
        self.pinned_bookmarks
            .iter()
            .chain(self.bookmarks.iter())
            .any(|b| self.current_path.starts_with(b))
    }

    fn render_drives(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // Only highlight a drive if no bookmark already claims the current path.
        // This prevents Macintosh HD (/) from always appearing active.
        let active_drive_idx = if self.any_bookmark_matches() {
            None
        } else {
            self.drives
                .iter()
                .enumerate()
                .filter(|(_, d)| self.current_path.starts_with(&d.path))
                .max_by_key(|(_, d)| d.path.as_os_str().len())
                .map(|(i, _)| i)
        };

        let sidebar = cx.entity().clone();

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
                let is_external = drive.is_external;
                let is_active = active_drive_idx == Some(i);

                let ctx_dedup = drive.path.clone();
                let ctx_index = drive.path.clone();
                let ctx_eject = drive.path.clone();
                let sidebar_ref = sidebar.clone();

                div()
                    .id(SharedString::from(format!("drive-wrap-{i}")))
                    .child(
                        SidebarRow::new(
                            SharedString::from(format!("drive-{i}")),
                            SharedString::from(drive.name.clone()),
                            icon,
                        )
                        .active(is_active)
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(SidebarEvent::OpenPath(p.clone()));
                        })),
                    )
                    .context_menu(move |menu, _window, _cx| {
                        let sb = sidebar_ref.clone();

                        let mut m = menu
                            .item(
                                PopupMenuItem::new("Find Duplicates")
                                    .icon(IconName::Copy)
                                    .on_click({
                                        let path = ctx_dedup.clone();
                                        let sb = sb.clone();
                                        move |_, _, cx| {
                                            sb.update(cx, |_, cx| {
                                                cx.emit(SidebarEvent::FindDuplicates(path.clone()));
                                            });
                                        }
                                    }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new("Index for Search")
                                    .icon(IconName::Search)
                                    .on_click({
                                        let path = ctx_index.clone();
                                        let sb = sb.clone();
                                        move |_, _, cx| {
                                            sb.update(cx, |_, cx| {
                                                cx.emit(SidebarEvent::IndexLocation(path.clone()));
                                            });
                                        }
                                    }),
                            );

                        if is_external {
                            m = m.separator().item(
                                PopupMenuItem::new("Eject").icon(IconName::Minus).on_click({
                                    let path = ctx_eject.clone();
                                    let sb = sb;
                                    move |_, _, cx| {
                                        sb.update(cx, |_, cx| {
                                            cx.emit(SidebarEvent::EjectDrive(path.clone()));
                                        });
                                    }
                                }),
                            );
                        }

                        m
                    })
            })
            .collect();

        v_flex()
            .gap_0p5()
            .child(SectionHeader::new("STORAGES"))
            .children(items)
    }
}

fn bookmark_icon(name: &str) -> IconName {
    match name {
        "Desktop" => IconName::LayoutDashboard,
        "Documents" => IconName::File,
        "Downloads" => IconName::ArrowDown,
        _ => IconName::Folder,
    }
}

impl Render for AppSidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("sidebar")
            .w(SIDEBAR_WIDTH)
            .h_full()
            .flex_shrink_0()
            .bg(theme::sidebar_bg(cx))
            .pt(SIDEBAR_TOP_INSET) // below traffic lights
            .px(PADDING_MD)
            .pb(PADDING_LG)
            .child(
                v_flex()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h_0()
                    .gap_4()
                    .overflow_y_scroll()
                    .child(self.render_pinned_bookmarks(cx))
                    .child(self.render_regular_bookmarks(cx))
                    .child(self.render_drives(cx)),
            )
            // Settings pinned at bottom
            .child(self.render_settings_row(cx))
    }
}
