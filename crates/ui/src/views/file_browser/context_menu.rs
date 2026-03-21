use gpui::*;
use gpui_component::{
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
    m = m.action_row("Open", Box::new(OpenSelected));

    // New → submenu
    let new_sub = PopupMenu::build(window, cx, |menu, _w, _cx| {
        menu.action_row("New Folder", Box::new(NewFolder))
            .action_row("New Todo", Box::new(NewTodoFile))
    });
    m = m.item(PopupMenuItem::submenu("New", new_sub)).separator();

    // Clipboard
    m = m
        .action_row("Cut", Box::new(CutFiles))
        .action_row("Copy", Box::new(CopyFiles))
        .action_row("Copy Path", Box::new(CopyPath))
        .action_row("Paste", Box::new(PasteFiles))
        .separator();

    // Edit
    m = m.action_row("Rename", Box::new(Rename));

    // Pane actions (split view only)
    if split_active {
        m = m
            .action_row("Move to Other Pane", Box::new(MoveToOtherPane))
            .action_row("Copy to Other Pane", Box::new(CopyToOtherPane));
    }

    // Directory actions
    if is_dir {
        m = m.separator();
        m = m
            .action_row("Add to Bookmarks", Box::new(AddToBookmarks))
            .action_row("Find Duplicates", Box::new(FindDuplicatesHere));
    }

    // Destructive — always last
    m = m.separator().action_row("Trash", Box::new(MoveToTrash));

    m
}
