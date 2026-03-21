use std::collections::HashSet;
use std::path::PathBuf;

use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{ContextMenuExt as _, PopupMenuItem},
    v_flex,
};

use crate::models::ActiveView;
use crate::platform::traffic_lights::TrafficLights;
use crate::services::GitService;
use crate::session::Settings;
use crate::theme::{
    self, BOOKMARK_TILE_HEIGHT, ICON_MD, ICON_XS, PADDING_LG, PADDING_MD, RADIUS, RADIUS_SM,
    SIDEBAR_TOP_INSET, SIDEBAR_WIDTH, SPACE_XS,
};
use crate::ui::{SectionHeader, SidebarRow};

/// Events emitted by the sidebar to the app root.
pub enum SidebarEvent {
    OpenPath(PathBuf),
    RemoveBookmark(PathBuf),
    UnpinBookmark(PathBuf),
    PinBookmark(PathBuf),
    EjectDrive(PathBuf),
    FindDuplicates(PathBuf),
    IndexLocation(PathBuf),
    GoBack,
    GoForward,
    ToggleSidebar,
    OpenSearch,
    SwitchWorkspace(usize),
    CreateWorkspace,
    #[allow(dead_code)]
    RenameWorkspace(usize, String),
    DeleteWorkspace(usize),
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
    git: Entity<GitService>,
    /// When false, nav controls are shown in the sidebar header.
    toolbar_visible: bool,
    can_go_back: bool,
    can_go_forward: bool,
    /// Workspace names for the dropdown.
    workspace_names: Vec<String>,
    /// Currently active workspace index.
    active_workspace_idx: usize,
    #[allow(dead_code)]
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}

struct DriveEntry {
    name: String,
    path: PathBuf,
    is_external: bool,
    #[allow(dead_code)]
    fstype: String,
}

impl AppSidebar {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        active_view: ActiveView,
        current_path: PathBuf,
        pinned_bookmarks: Vec<PathBuf>,
        bookmarks: Vec<PathBuf>,
        workspace_names: Vec<String>,
        active_workspace_idx: usize,
        git: Entity<GitService>,
        cx: &mut Context<Self>,
    ) -> Self {
        let drives = Self::discover_drives();

        // Re-render sidebar when git status changes.
        let git_sub = cx.observe(&git, |_this, _git, cx| {
            cx.notify();
        });

        Self {
            active_view,
            current_path,
            pinned_bookmarks,
            bookmarks,
            drives,
            git,
            toolbar_visible: false,
            can_go_back: false,
            can_go_forward: false,
            workspace_names,
            active_workspace_idx,
            focus_handle: cx.focus_handle(),
            _subs: vec![git_sub],
        }
    }

    pub fn set_active_view(&mut self, view: ActiveView, cx: &mut Context<Self>) {
        if self.active_view != view {
            self.active_view = view;
            cx.notify();
        }
    }

    pub fn set_current_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.current_path != path {
            self.current_path = path;
            cx.notify();
        }
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

    pub fn set_toolbar_state(
        &mut self,
        visible: bool,
        can_back: bool,
        can_forward: bool,
        cx: &mut Context<Self>,
    ) {
        if self.toolbar_visible != visible
            || self.can_go_back != can_back
            || self.can_go_forward != can_forward
        {
            self.toolbar_visible = visible;
            self.can_go_back = can_back;
            self.can_go_forward = can_forward;
            cx.notify();
        }
    }

    /// Full workspace switch: update names, active index, and bookmarks.
    pub fn set_workspace(
        &mut self,
        names: Vec<String>,
        active_idx: usize,
        pinned: Vec<PathBuf>,
        regular: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.workspace_names = names;
        self.active_workspace_idx = active_idx;
        self.pinned_bookmarks = pinned;
        self.bookmarks = regular;
        cx.notify();
    }

    /// Update workspace names only (for rename).
    pub fn set_workspace_names(&mut self, names: Vec<String>, cx: &mut Context<Self>) {
        self.workspace_names = names;
        cx.notify();
    }

    /// Swap bookmarks (used on workspace switch from app).
    #[allow(dead_code)]
    pub fn set_bookmarks(
        &mut self,
        pinned: Vec<PathBuf>,
        regular: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.pinned_bookmarks = pinned;
        self.bookmarks = regular;
        cx.notify();
    }

    /// Build unique short labels for workspace icons.
    /// Uses first char uppercase; appends index on collision.
    fn workspace_labels(names: &[String]) -> Vec<String> {
        let firsts: Vec<String> = names
            .iter()
            .map(|n| n.chars().next().unwrap_or('?').to_uppercase().collect())
            .collect();

        // Count how many times each letter appears
        let mut counts = std::collections::HashMap::<&str, usize>::new();
        for f in &firsts {
            *counts.entry(f.as_str()).or_default() += 1;
        }

        // For duplicates, append a running number
        let mut seen = std::collections::HashMap::<String, usize>::new();
        firsts
            .iter()
            .map(|f| {
                if counts.get(f.as_str()).copied().unwrap_or(0) > 1 {
                    let n = seen.entry(f.clone()).or_insert(0);
                    *n += 1;
                    format!("{f}{n}")
                } else {
                    f.clone()
                }
            })
            .collect()
    }

    /// Arc-style bottom workspace bar: small letter icons, active highlight, + button.
    fn render_workspace_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let fg = cx.theme().foreground;
        let active_idx = self.active_workspace_idx;
        let workspace_count = self.workspace_names.len();
        let active_bg = theme::surface_active(cx);
        let labels = Self::workspace_labels(&self.workspace_names);

        let items: Vec<_> = self
            .workspace_names
            .iter()
            .enumerate()
            .map(|(i, _name)| {
                let is_active = i == active_idx;
                let label = labels[i].clone();

                let (text_col, bg) = if is_active {
                    (fg, active_bg)
                } else {
                    (muted.opacity(0.5), gpui::transparent_black())
                };

                let sidebar = cx.entity().clone();
                let ws_count = workspace_count;

                div()
                    .id(SharedString::from(format!("ws-{i}")))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(26.0))
                    .rounded(RADIUS_SM)
                    .bg(bg)
                    .text_size(px(10.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(text_col)
                    .hover(|s| s.bg(active_bg))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        if !is_active {
                            cx.emit(SidebarEvent::SwitchWorkspace(i));
                        }
                    }))
                    .child(SharedString::from(label))
                    .context_menu({
                        let sidebar = sidebar.clone();
                        move |menu, _window, _cx| {
                            let mut m = menu;
                            if ws_count > 1 {
                                let sb = sidebar.clone();
                                m = m.item(PopupMenuItem::new("Delete Workspace").on_click(
                                    move |_, _, cx| {
                                        sb.update(cx, |_, cx| {
                                            cx.emit(SidebarEvent::DeleteWorkspace(i));
                                        });
                                    },
                                ));
                            }
                            m
                        }
                    })
            })
            .collect();

        h_flex()
            .id("workspace-bar")
            .w_full()
            .items_center()
            .gap(px(4.0))
            .px(PADDING_MD)
            .pt(px(6.0))
            .pb(px(8.0))
            .border_t_1()
            .border_color(cx.theme().border.opacity(0.15))
            .children(items)
            .child(div().flex_1())
            .child(
                div()
                    .id("ws-add")
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(26.0))
                    .rounded(RADIUS_SM)
                    .text_color(muted.opacity(0.35))
                    .hover(|s| s.bg(active_bg).text_color(muted))
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.emit(SidebarEvent::CreateWorkspace);
                    }))
                    .child(Icon::new(IconName::Plus).with_size(ICON_XS)),
            )
    }

    fn discover_drives() -> Vec<DriveEntry> {
        use crate::views::file_browser::mount::fstype_for_path;
        use std::path::Path;

        let mut drives = Vec::new();

        let root_fstype = fstype_for_path(Path::new("/"))
            .unwrap_or("apfs")
            .to_uppercase();

        drives.push(DriveEntry {
            name: "Macintosh HD".into(),
            path: PathBuf::from("/"),
            is_external: false,
            fstype: root_fstype,
        });

        if let Ok(entries) = std::fs::read_dir("/Volumes") {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name == "Macintosh HD" {
                    continue;
                }
                let fstype = fstype_for_path(&path).unwrap_or("").to_uppercase();
                drives.push(DriveEntry {
                    name,
                    path,
                    is_external: true,
                    fstype,
                });
            }
        }

        drives
    }

    /// Nav controls shown in the sidebar header when toolbar is hidden.
    fn render_nav_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let disabled_color = muted.opacity(0.35);
        let can_back = self.can_go_back;
        let can_forward = self.can_go_forward;

        h_flex()
            .w_full()
            .items_center()
            .gap(px(4.0))
            .pl(px(70.0)) // after traffic lights
            .pr(PADDING_MD)
            .h(SIDEBAR_TOP_INSET)
            // Sidebar toggle
            .child(
                Button::new("sb-sidebar-toggle")
                    .ghost()
                    .small()
                    .icon(IconName::PanelLeftClose)
                    .text_color(muted)
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.emit(SidebarEvent::ToggleSidebar);
                    })),
            )
            // Back / Forward
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(SPACE_XS)
                    .child(
                        Button::new("sb-nav-back")
                            .ghost()
                            .small()
                            .icon(IconName::ChevronLeft)
                            .text_color(if can_back { muted } else { disabled_color })
                            .disabled(!can_back)
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(SidebarEvent::GoBack);
                            })),
                    )
                    .child(
                        Button::new("sb-nav-forward")
                            .ghost()
                            .small()
                            .icon(IconName::ChevronRight)
                            .text_color(if can_forward { muted } else { disabled_color })
                            .disabled(!can_forward)
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(SidebarEvent::GoForward);
                            })),
                    ),
            )
            // Spacer
            .child(div().flex_1())
            // Search button
            .child(
                Button::new("sb-search")
                    .ghost()
                    .small()
                    .icon(IconName::Search)
                    .text_color(muted)
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.emit(SidebarEvent::OpenSearch);
                    })),
            )
    }

    /// Path bar shown below nav header when toolbar is hidden.
    fn render_path_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let folder_name = self
            .current_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Macintosh HD".to_string());

        div()
            .id("sidebar-path-bar")
            .cursor_pointer()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .px(PADDING_MD)
            .py(px(4.0))
            .mb_2()
            .rounded(RADIUS_SM)
            .bg(cx.theme().background)
            .hover(|s| s.bg(cx.theme().muted.opacity(0.5)))
            .on_click(cx.listener(|_, _, _, cx| {
                cx.emit(SidebarEvent::OpenSearch);
            }))
            .child(
                Icon::new(IconName::Folder)
                    .with_size(ICON_XS)
                    .text_color(muted.opacity(0.6)),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(muted)
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .min_w_0()
                    .child(SharedString::from(folder_name)),
            )
            .child(
                Icon::new(IconName::ChevronDown)
                    .with_size(px(9.0))
                    .text_color(muted.opacity(0.4)),
            )
    }

    /// Collect the set of bookmark paths that are in dirty git repos.
    fn dirty_bookmarks(&self, cx: &App) -> HashSet<PathBuf> {
        let git = self.git.read(cx);
        self.pinned_bookmarks
            .iter()
            .chain(self.bookmarks.iter())
            .filter(|p| git.status(p).is_some_and(|s| s.dirty))
            .cloned()
            .collect()
    }

    fn render_pinned_bookmarks(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pinned_bookmarks.is_empty() {
            return div();
        }

        let muted = cx.theme().muted_foreground;
        let warning = cx.theme().warning;
        let sidebar = cx.entity().clone();
        let active_bm = self.active_bookmark_path();
        let dirty = self.dirty_bookmarks(cx);

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

                let is_active = active_bm == Some(path);
                let is_dirty = dirty.contains(path);
                let is_system = Settings::is_system_bookmark(path);
                let p = path.clone();

                let icon_color = if is_dirty {
                    warning
                } else if is_active {
                    cx.theme().foreground
                } else {
                    muted
                };

                let bg = if is_active {
                    theme::selection_active_bg(cx)
                } else {
                    cx.theme().background
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
                            .item(PopupMenuItem::new("Open").on_click({
                                let path = ctx_open.clone();
                                let sb = sb.clone();
                                move |_, _, cx| {
                                    sb.update(cx, |_, cx| {
                                        cx.emit(SidebarEvent::OpenPath(path.clone()));
                                    });
                                }
                            }))
                            .separator()
                            .item(PopupMenuItem::new("Unpin from Quick Access").on_click({
                                let path = ctx_unpin.clone();
                                let sb = sb.clone();
                                move |_, _, cx| {
                                    sb.update(cx, |_, cx| {
                                        cx.emit(SidebarEvent::UnpinBookmark(path.clone()));
                                    });
                                }
                            }));

                        if !is_system {
                            m = m
                                .separator()
                                .item(PopupMenuItem::new("Remove Bookmark").on_click({
                                    let path = ctx_remove.clone();
                                    let sb = sb;
                                    move |_, _, cx| {
                                        sb.update(cx, |_, cx| {
                                            cx.emit(SidebarEvent::RemoveBookmark(path.clone()));
                                        });
                                    }
                                }));
                        }

                        m
                    })
            })
            .collect();

        div().flex().flex_row().gap_2().children(items)
    }

    fn render_regular_bookmarks(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.bookmarks.is_empty() {
            return div();
        }

        let warning = cx.theme().warning;
        let sidebar = cx.entity().clone();
        let active_bm = self.active_bookmark_path();
        let dirty = self.dirty_bookmarks(cx);

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
                let is_active = active_bm == Some(path);
                let is_dirty = dirty.contains(path);
                let is_system = Settings::is_system_bookmark(path);
                let p = path.clone();
                let ctx_open = path.clone();
                let ctx_pin = path.clone();
                let ctx_remove = path.clone();
                let sidebar_ref = sidebar.clone();

                let mut row = SidebarRow::new(
                    SharedString::from(format!("reg-bm-row-{i}")),
                    SharedString::from(name),
                    icon,
                )
                .active(is_active)
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(SidebarEvent::OpenPath(p.clone()));
                }));

                if is_dirty {
                    row = row.color_override(warning);
                }

                div()
                    .id(SharedString::from(format!("reg-bm-{i}")))
                    .child(row)
                    .context_menu(move |menu, _window, _cx| {
                        let sb = sidebar_ref.clone();

                        let mut m = menu
                            .item(PopupMenuItem::new("Open").on_click({
                                let path = ctx_open.clone();
                                let sb = sb.clone();
                                move |_, _, cx| {
                                    sb.update(cx, |_, cx| {
                                        cx.emit(SidebarEvent::OpenPath(path.clone()));
                                    });
                                }
                            }))
                            .separator()
                            .item(PopupMenuItem::new("Pin to Quick Access").on_click({
                                let path = ctx_pin.clone();
                                let sb = sb.clone();
                                move |_, _, cx| {
                                    sb.update(cx, |_, cx| {
                                        cx.emit(SidebarEvent::PinBookmark(path.clone()));
                                    });
                                }
                            }));

                        if !is_system {
                            m = m
                                .separator()
                                .item(PopupMenuItem::new("Remove Bookmark").on_click({
                                    let path = ctx_remove.clone();
                                    let sb = sb;
                                    move |_, _, cx| {
                                        sb.update(cx, |_, cx| {
                                            cx.emit(SidebarEvent::RemoveBookmark(path.clone()));
                                        });
                                    }
                                }));
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

    /// The single most-specific bookmark whose path is a prefix of `current_path`.
    /// Only returns a match when the active view is a file browser — tools, settings,
    /// etc. should never co-highlight a bookmark.
    fn active_bookmark_path(&self) -> Option<&PathBuf> {
        if !matches!(self.active_view, ActiveView::FileBrowser(_)) {
            return None;
        }
        self.pinned_bookmarks
            .iter()
            .chain(self.bookmarks.iter())
            .filter(|b| self.current_path.starts_with(b))
            .max_by_key(|b| b.as_os_str().len())
    }

    fn render_drives(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // Only highlight a drive when browsing files and no bookmark already
        // claims the path. Non-browser views (tools, settings) never highlight.
        let active_drive_idx = if !matches!(self.active_view, ActiveView::FileBrowser(_))
            || self.active_bookmark_path().is_some()
        {
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
                            .item(PopupMenuItem::new("Find Duplicates").on_click({
                                let path = ctx_dedup.clone();
                                let sb = sb.clone();
                                move |_, _, cx| {
                                    sb.update(cx, |_, cx| {
                                        cx.emit(SidebarEvent::FindDuplicates(path.clone()));
                                    });
                                }
                            }))
                            .separator()
                            .item(PopupMenuItem::new("Index for Search").on_click({
                                let path = ctx_index.clone();
                                let sb = sb.clone();
                                move |_, _, cx| {
                                    sb.update(cx, |_, cx| {
                                        cx.emit(SidebarEvent::IndexLocation(path.clone()));
                                    });
                                }
                            }));

                        if is_external {
                            m = m.separator().item(PopupMenuItem::new("Eject").on_click({
                                let path = ctx_eject.clone();
                                let sb = sb;
                                move |_, _, cx| {
                                    sb.update(cx, |_, cx| {
                                        cx.emit(SidebarEvent::EjectDrive(path.clone()));
                                    });
                                }
                            }));
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
        let toolbar_hidden = !self.toolbar_visible;

        let top_padding = if toolbar_hidden {
            px(0.0)
        } else {
            SIDEBAR_TOP_INSET
        };

        let mut root = v_flex()
            .id("sidebar")
            .relative()
            .w(SIDEBAR_WIDTH)
            .h_full()
            .flex_shrink_0()
            .bg(theme::sidebar_bg(cx))
            .pt(top_padding)
            .px(PADDING_MD)
            // Custom traffic lights — positioned where native ones used to be
            .child(
                div()
                    .absolute()
                    .top(px(11.0))
                    .left(px(11.0))
                    .child(TrafficLights::new()),
            );

        if toolbar_hidden {
            root = root
                .child(self.render_nav_header(cx))
                .child(self.render_path_bar(cx));
        }

        root = root.child(
            v_flex()
                .id("sidebar-scroll")
                .flex_1()
                .min_h_0()
                .gap_4()
                .overflow_y_scroll()
                .pb(PADDING_LG)
                .child(self.render_pinned_bookmarks(cx))
                .child(self.render_regular_bookmarks(cx))
                .child(self.render_drives(cx)),
        );

        // Bottom workspace bar (Arc-style)
        root.child(self.render_workspace_bar(cx))
    }
}
