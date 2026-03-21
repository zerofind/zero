use gpui::{Menu, MenuItem, OsAction};

use crate::actions;

pub fn app_menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "Zero".into(),
            items: vec![
                MenuItem::action("About Zero", actions::About),
                MenuItem::action("Check for Updates", actions::CheckForUpdates),
                MenuItem::separator(),
                MenuItem::action("Settings...", actions::OpenSettings),
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::os_submenu("Services", gpui::SystemMenuType::Services),
                #[cfg(target_os = "macos")]
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::action("Hide Zero", actions::Hide),
                #[cfg(target_os = "macos")]
                MenuItem::action("Hide Others", actions::HideOthers),
                #[cfg(target_os = "macos")]
                MenuItem::action("Show All", actions::ShowAll),
                #[cfg(target_os = "macos")]
                MenuItem::separator(),
                MenuItem::action("Quit Zero", actions::Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New Folder", actions::NewFolder),
                MenuItem::action("New Todo File", actions::NewTodoFile),
                MenuItem::separator(),
                MenuItem::action("Quick Look", actions::QuickLook),
                MenuItem::action("Reveal in Finder", actions::RevealInFinder),
                MenuItem::separator(),
                MenuItem::action("Rename", actions::Rename),
                MenuItem::action("Duplicate", actions::DuplicateFiles),
                MenuItem::action("Move to Trash", actions::MoveToTrash),
                MenuItem::separator(),
                MenuItem::action("Close Window", actions::CloseWindow),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Copy", actions::CopyFiles, OsAction::Copy),
                MenuItem::os_action("Cut", actions::CutFiles, OsAction::Cut),
                MenuItem::os_action("Paste", actions::PasteFiles, OsAction::Paste),
                MenuItem::separator(),
                MenuItem::os_action("Select All", actions::SelectAll, OsAction::SelectAll),
                MenuItem::separator(),
                MenuItem::action("Copy Path", actions::CopyPath),
                MenuItem::action("Find in Browser", actions::FindInBrowser),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Toggle Sidebar", actions::ToggleSidebar),
                MenuItem::action("Toggle Toolbar", actions::ToggleToolbar),
                MenuItem::action("Toggle Split View", actions::ToggleSplitView),
                MenuItem::separator(),
                MenuItem::action("Refresh", actions::Refresh),
            ],
        },
        Menu {
            name: "Go".into(),
            items: vec![
                MenuItem::action("Back", actions::GoBack),
                MenuItem::action("Forward", actions::GoForward),
                MenuItem::action("Enclosing Folder", actions::GoUp),
                MenuItem::separator(),
                MenuItem::action("Home", actions::GoHome),
                MenuItem::action("Desktop", actions::GoDesktop),
                MenuItem::action("Documents", actions::GoDocuments),
                MenuItem::action("Downloads", actions::GoDownloads),
                MenuItem::action("Applications", actions::GoApplications),
                MenuItem::separator(),
                MenuItem::action("Command Palette...", actions::OpenCommandPalette),
            ],
        },
        Menu {
            name: "Tools".into(),
            items: vec![
                MenuItem::action("Find Duplicates", actions::OpenDuplicates),
                MenuItem::action("Cleanup Storage", actions::OpenCleanup),
                MenuItem::action("Secure Erase", actions::OpenSecureErase),
                MenuItem::separator(),
                MenuItem::action("Ask AI", actions::ToggleAsk),
                MenuItem::action("Terminal", actions::ToggleTerminal),
                MenuItem::separator(),
                MenuItem::action("Automations", actions::OpenAutomations),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                #[cfg(target_os = "macos")]
                MenuItem::action("Minimize", actions::Minimize),
                #[cfg(target_os = "macos")]
                MenuItem::action("Zoom", actions::Zoom),
                #[cfg(target_os = "macos")]
                MenuItem::separator(),
                MenuItem::action("Workspace 1", actions::SwitchWorkspace1),
                MenuItem::action("Workspace 2", actions::SwitchWorkspace2),
                MenuItem::action("Workspace 3", actions::SwitchWorkspace3),
                MenuItem::action("Workspace 4", actions::SwitchWorkspace4),
                MenuItem::action("Workspace 5", actions::SwitchWorkspace5),
                MenuItem::action("Workspace 6", actions::SwitchWorkspace6),
                MenuItem::action("Workspace 7", actions::SwitchWorkspace7),
                MenuItem::action("Workspace 8", actions::SwitchWorkspace8),
                MenuItem::action("Workspace 9", actions::SwitchWorkspace9),
            ],
        },
        Menu {
            name: "Help".into(),
            items: vec![MenuItem::action("Zero Help", actions::About)],
        },
    ]
}
