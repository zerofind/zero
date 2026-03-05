use gpui::*;
use gpui_component::{
    IconName,
    menu::{PopupMenu, PopupMenuItem},
    table::TableState,
};

use crate::actions::{
    AddToBookmarks, CopyFiles, CopyPath, CopyToOtherPane, CutFiles, FindDuplicatesHere,
    MoveToOtherPane, MoveToTrash, NewFolder, NewTodoFile, OpenSelected, PasteFiles, Rename,
};
use crate::ui::ZeroMenuExt;

use super::delegate::FileBrowserDelegate;
use super::state::BrowserEntry;

/// Build the right-click context menu for a file browser row.
pub fn build_context_menu(
    entries: &[BrowserEntry],
    row_ix: usize,
    split_active: bool,
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<TableState<FileBrowserDelegate>>,
) -> PopupMenu {
    let Some(entry) = entries.get(row_ix) else {
        return menu;
    };

    let is_dir = entry.is_dir;
    let mut m = menu;

    // Primary action
    m = m
        .action_row("Open", IconName::FolderOpen, Box::new(OpenSelected))
        .separator();

    // New → submenu
    let new_sub = PopupMenu::build(window, cx, |menu, _w, _cx| {
        menu.action_row("New Folder", IconName::Folder, Box::new(NewFolder))
            .action_row("New Todo", IconName::File, Box::new(NewTodoFile))
    });
    m = m
        .item(PopupMenuItem::submenu("New", new_sub).icon(IconName::Plus))
        .separator();

    // Clipboard — copy actions, then move actions
    m = m
        .action_row("Copy", IconName::Copy, Box::new(CopyFiles))
        .action_row("Copy Path", IconName::Copy, Box::new(CopyPath))
        .action_row("Cut", IconName::Minus, Box::new(CutFiles))
        .action_row("Paste", IconName::Plus, Box::new(PasteFiles))
        .separator();

    // Edit
    m = m.action_row("Rename", IconName::Replace, Box::new(Rename));

    // Pane submenu (split view only)
    if split_active {
        m = m.separator();
        let pane_sub = PopupMenu::build(window, cx, |menu, _w, _cx| {
            menu.action_row(
                "Move to Other Pane",
                IconName::ArrowRight,
                Box::new(MoveToOtherPane),
            )
            .action_row(
                "Copy to Other Pane",
                IconName::Copy,
                Box::new(CopyToOtherPane),
            )
        });
        m = m.item(PopupMenuItem::submenu("Pane", pane_sub).icon(IconName::PanelRight));
    }

    // Actions submenu (dirs only)
    if is_dir {
        m = m.separator();
        let actions_sub = PopupMenu::build(window, cx, |menu, _w, _cx| {
            menu.action_row("Add to Bookmarks", IconName::Star, Box::new(AddToBookmarks))
                .action_row(
                    "Find Duplicates",
                    IconName::Search,
                    Box::new(FindDuplicatesHere),
                )
        });
        m = m.item(PopupMenuItem::submenu("Actions", actions_sub).icon(IconName::Settings));
    }

    // Destructive — always last
    m = m
        .separator()
        .action_row("Trash", IconName::Delete, Box::new(MoveToTrash));

    m
}
