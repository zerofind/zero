use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{ActiveTheme, IconName, h_flex};

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS};
use crate::ui::FileIcon;

// -- Data types shared with view.rs ------------------------------------------

/// Action entry for the palette's default section.
pub(super) struct PaletteAction {
    pub name: &'static str,
    pub icon: fn() -> IconName,
    pub path: &'static str,
    pub shortcut: Option<&'static str>,
    /// Whether this action appears in the empty-state default view.
    /// Actions with `false` are still searchable when the user types.
    pub default_visible: bool,
}

pub(super) const DEFAULT_ACTIONS: &[PaletteAction] = &[
    // -- Browse / Search -------------------------------------------------
    PaletteAction {
        name: "Applications",
        icon: || IconName::LayoutDashboard,
        path: "apps://",
        shortcut: None,
        default_visible: true,
    },
    PaletteAction {
        name: "Todos",
        icon: || IconName::Check,
        path: "action://todo",
        shortcut: None,
        default_visible: true,
    },
    // Media — use File icon; color comes from the render layer
    PaletteAction {
        name: "Images",
        icon: || IconName::File,
        path: "type://images",
        shortcut: None,
        default_visible: true,
    },
    PaletteAction {
        name: "Videos",
        icon: || IconName::File,
        path: "type://videos",
        shortcut: None,
        default_visible: true,
    },
    PaletteAction {
        name: "Audio",
        icon: || IconName::File,
        path: "type://audio",
        shortcut: None,
        default_visible: true,
    },
    // Files
    PaletteAction {
        name: "Documents",
        icon: || IconName::File,
        path: "type://documents",
        shortcut: None,
        default_visible: true,
    },
    PaletteAction {
        name: "Archives",
        icon: || IconName::File,
        path: "type://archives",
        shortcut: None,
        default_visible: true,
    },
    PaletteAction {
        name: "Folders",
        icon: || IconName::Folder,
        path: "type://directories",
        shortcut: None,
        default_visible: true,
    },
    // Code — parent category visible; individual languages searchable only
    PaletteAction {
        name: "Code",
        icon: || IconName::File,
        path: "type://code",
        shortcut: None,
        default_visible: true,
    },
    PaletteAction {
        name: "Rust",
        icon: || IconName::File,
        path: "type://rust",
        shortcut: None,
        default_visible: false,
    },
    PaletteAction {
        name: "Swift",
        icon: || IconName::File,
        path: "type://swift",
        shortcut: None,
        default_visible: false,
    },
    PaletteAction {
        name: "TypeScript",
        icon: || IconName::File,
        path: "type://typescript",
        shortcut: None,
        default_visible: false,
    },
    PaletteAction {
        name: "JavaScript",
        icon: || IconName::File,
        path: "type://javascript",
        shortcut: None,
        default_visible: false,
    },
    PaletteAction {
        name: "Python",
        icon: || IconName::File,
        path: "type://python",
        shortcut: None,
        default_visible: false,
    },
    PaletteAction {
        name: "Go",
        icon: || IconName::File,
        path: "type://go",
        shortcut: None,
        default_visible: false,
    },
    // -- Tools (accessible via sidebar; searchable but not in defaults) --
    PaletteAction {
        name: "Find Duplicates",
        icon: || IconName::File,
        path: "action://dedup",
        shortcut: None,
        default_visible: false,
    },
    PaletteAction {
        name: "Cleanup",
        icon: || IconName::Delete,
        path: "action://cleanup",
        shortcut: None,
        default_visible: false,
    },
    // -- View ------------------------------------------------------------
    PaletteAction {
        name: "Toggle Split View",
        icon: || IconName::LayoutDashboard,
        path: "action://toggle_split_view",
        shortcut: Some("\u{2318}T"),
        default_visible: true,
    },
    PaletteAction {
        name: "Toggle Sidebar",
        icon: || IconName::PanelLeft,
        path: "action://toggle_sidebar",
        shortcut: Some("\u{2318}B"),
        default_visible: true,
    },
    PaletteAction {
        name: "New Folder",
        icon: || IconName::Folder,
        path: "action://new_folder",
        shortcut: Some("\u{21e7}\u{2318}N"),
        default_visible: true,
    },
    // -- Settings --------------------------------------------------------
    PaletteAction {
        name: "Settings",
        icon: || IconName::Settings,
        path: "action://settings",
        shortcut: Some("\u{2318},"),
        default_visible: true,
    },
    PaletteAction {
        name: "Automations",
        icon: || IconName::Settings,
        path: "action://automations",
        shortcut: None,
        default_visible: true,
    },
];

pub struct StorageEntry {
    pub name: String,
    pub mount_point: PathBuf,
    pub is_external: bool,
}

impl StorageEntry {
    pub fn discover() -> Vec<StorageEntry> {
        let mut storages = Vec::new();
        // Internal disk — label it "Macintosh HD" like Finder does
        if let Some(home) = dirs::home_dir()
            && let Ok(_vol) = zero::disk::VolumeInfo::for_path(&home)
        {
            storages.push(StorageEntry {
                name: "Macintosh HD".to_string(),
                mount_point: home,
                is_external: false,
            });
        }
        // Real external volumes — only those mounted under /Volumes/
        if let Ok(all) = zero::disk::VolumeInfo::all() {
            for vol in all {
                if !vol.is_external() {
                    continue;
                }
                if !vol.mount_point.starts_with("/Volumes/") {
                    continue;
                }
                storages.push(StorageEntry {
                    name: vol.name.clone(),
                    mount_point: PathBuf::from(&vol.mount_point),
                    is_external: true,
                });
            }
        }
        storages
    }
}

// -- PaletteItem rendering element -------------------------------------------

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
                            h_flex()
                                .gap_1()
                                .items_center()
                                .px_1p5()
                                .py(px(2.))
                                .bg(cx.theme().muted)
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .child("Run"),
                        )
                    })
                    .when(
                        self.selected && !self.is_drillable && self.action_label.is_some(),
                        |el| {
                            el.child(
                                h_flex()
                                    .gap_1()
                                    .items_center()
                                    .px_1p5()
                                    .py(px(2.))
                                    .bg(cx.theme().muted)
                                    .text_size(FONT_SIZE_CAPTION)
                                    .text_color(muted)
                                    .child(self.action_label.unwrap()),
                            )
                        },
                    ),
            )
    }
}
