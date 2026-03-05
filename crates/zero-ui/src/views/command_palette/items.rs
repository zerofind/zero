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
    pub category: &'static str,
    pub shortcut: Option<&'static str>,
}

pub(super) const DEFAULT_ACTIONS: &[PaletteAction] = &[
    // -- Browse / Search -------------------------------------------------
    PaletteAction {
        name: "Applications",
        icon: || IconName::LayoutDashboard,
        path: "apps://",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "Todos",
        icon: || IconName::Check,
        path: "action://todo",
        category: "View",
        shortcut: None,
    },
    // Media
    PaletteAction {
        name: "Images",
        icon: || IconName::Eye,
        path: "type://images",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "Videos",
        icon: || IconName::Eye,
        path: "type://videos",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "Audio",
        icon: || IconName::Search,
        path: "type://audio",
        category: "Search",
        shortcut: None,
    },
    // Files
    PaletteAction {
        name: "Documents",
        icon: || IconName::File,
        path: "type://documents",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "Archives",
        icon: || IconName::File,
        path: "type://archives",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "Folders",
        icon: || IconName::Folder,
        path: "type://directories",
        category: "Search",
        shortcut: None,
    },
    // Code languages
    PaletteAction {
        name: "Code",
        icon: || IconName::Search,
        path: "type://code",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "Rust",
        icon: || IconName::Search,
        path: "type://rust",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "Swift",
        icon: || IconName::Search,
        path: "type://swift",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "TypeScript",
        icon: || IconName::Search,
        path: "type://typescript",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "JavaScript",
        icon: || IconName::Search,
        path: "type://javascript",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "Python",
        icon: || IconName::Search,
        path: "type://python",
        category: "Search",
        shortcut: None,
    },
    PaletteAction {
        name: "Go",
        icon: || IconName::Search,
        path: "type://go",
        category: "Search",
        shortcut: None,
    },
    // -- Tools -----------------------------------------------------------
    PaletteAction {
        name: "Find Duplicates",
        icon: || IconName::File,
        path: "action://dedup",
        category: "View",
        shortcut: None,
    },
    PaletteAction {
        name: "System Cleanup",
        icon: || IconName::Delete,
        path: "action://cleanup",
        category: "View",
        shortcut: None,
    },
    // -- View ------------------------------------------------------------
    PaletteAction {
        name: "Toggle Split View",
        icon: || IconName::LayoutDashboard,
        path: "action://toggle_split_view",
        category: "View",
        shortcut: Some("\u{2318}T"),
    },
    PaletteAction {
        name: "Toggle Sidebar",
        icon: || IconName::PanelLeft,
        path: "action://toggle_sidebar",
        category: "View",
        shortcut: Some("\u{2318}B"),
    },
    PaletteAction {
        name: "New Folder",
        icon: || IconName::Folder,
        path: "action://new_folder",
        category: "Files",
        shortcut: Some("\u{21e7}\u{2318}N"),
    },
    // -- Settings --------------------------------------------------------
    PaletteAction {
        name: "Settings",
        icon: || IconName::Settings,
        path: "action://settings",
        category: "View",
        shortcut: Some("\u{2318},"),
    },
    PaletteAction {
        name: "Automations",
        icon: || IconName::Settings,
        path: "action://automations",
        category: "View",
        shortcut: None,
    },
];

pub(super) struct StorageEntry {
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
