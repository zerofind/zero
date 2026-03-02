use gpui::*;
use gpui_component::{menu::PopupMenu, table::TableState, IconName};

use crate::actions::{
    AddToBookmarks, CopyFiles, CopyPath, CopyToOtherPane, CutFiles, DuplicateFiles,
    FindDuplicatesHere, MoveToOtherPane, MoveToTrash, NewFolder, NewTodoFile, OpenSelected,
    PasteFiles, QuickLook, Rename, RevealInFinder,
};

use super::delegate::FileBrowserDelegate;
use super::state::BrowserEntry;

/// Build the right-click context menu for a file browser row.
pub fn build_context_menu(
    entries: &[BrowserEntry],
    row_ix: usize,
    split_active: bool,
    menu: PopupMenu,
    _window: &mut Window,
    _cx: &mut Context<TableState<FileBrowserDelegate>>,
) -> PopupMenu {
    let Some(entry) = entries.get(row_ix) else {
        return menu;
    };

    let is_dir = entry.is_dir;
    let _path = entry.path.to_string_lossy().to_string();

    let mut m = menu;

    m = m
        .menu_with_icon("Open", IconName::FolderOpen, Box::new(OpenSelected))
        .separator();

    if !is_dir {
        m = m
            .menu_with_icon("Quick Look", IconName::Eye, Box::new(QuickLook))
            .separator();
    }

    m = m
        .menu_with_icon("Copy", IconName::Copy, Box::new(CopyFiles))
        .menu_with_icon("Cut", IconName::Minus, Box::new(CutFiles))
        .menu_with_icon("Paste", IconName::Plus, Box::new(PasteFiles))
        .menu_with_icon("Duplicate", IconName::Copy, Box::new(DuplicateFiles))
        .separator()
        .menu_with_icon("Copy Path", IconName::Copy, Box::new(CopyPath))
        .menu_with_icon(
            "Reveal in Finder",
            IconName::ExternalLink,
            Box::new(RevealInFinder),
        );

    if is_dir {
        m = m
            .menu_with_icon(
                "Add to Bookmarks",
                IconName::Plus,
                Box::new(AddToBookmarks),
            )
            .menu_with_icon(
                "Find Duplicates",
                IconName::Search,
                Box::new(FindDuplicatesHere),
            );
    }

    m = m
        .separator()
        .menu_with_icon("New Todo", IconName::Plus, Box::new(NewTodoFile));

    if split_active {
        m = m
            .menu_with_icon(
                "Move to Other Pane",
                IconName::ArrowRight,
                Box::new(MoveToOtherPane),
            )
            .menu_with_icon(
                "Copy to Other Pane",
                IconName::Copy,
                Box::new(CopyToOtherPane),
            );
    }

    m = m
        .separator()
        .menu_with_icon("Rename", IconName::Replace, Box::new(Rename))
        .menu_with_icon("New Folder", IconName::Plus, Box::new(NewFolder))
        .separator()
        .menu_with_icon(
            "Move to Trash",
            IconName::Delete,
            Box::new(MoveToTrash),
        );

    m
}
