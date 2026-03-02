use std::path::PathBuf;
use std::time::Instant;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, h_flex,
    input::{Input, InputState},
    table::{Table, TableState},
    v_flex,
};

use crate::actions::{
    AddToBookmarks, CopyFiles, CopyPath, CopyToOtherPane, CutFiles, DuplicateFiles,
    FindDuplicatesHere, FindInBrowser, MoveToOtherPane, MoveToTrash, NewFolder, NewTodoFile,
    OpenSelected, QuickLook, Refresh, Rename, RevealInFinder, SelectAll, SelectNext, SelectPrev,
};
use crate::services::SearchService;
use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, brand_color};
use crate::ui::{ConfirmDialog, EmptyState};

use super::delegate::FileBrowserDelegate;
use super::search_bar::DisplayMode;
use super::state;

/// Inline editing state for rename or new folder.
pub(super) enum InlineEdit {
    Rename { idx: usize },
    NewFolder,
}

pub struct FileBrowserView {
    pub(super) path: PathBuf,
    pub(super) table_state: Entity<TableState<FileBrowserDelegate>>,
    #[allow(dead_code)]
    pub(super) search: Entity<SearchService>,
    pub(super) load_time_ms: f64,
    #[allow(dead_code)]
    pub(super) loading: bool,
    #[allow(dead_code)]
    pub(super) error: Option<String>,
    pub(super) inline_edit: Option<InlineEdit>,
    pub(super) inline_input: Option<Entity<InputState>>,
    pub(super) pending_trash: Option<PathBuf>,
    // Search
    pub(super) search_active: bool,
    pub(super) search_input: Option<Entity<InputState>>,
    pub(super) display_mode: Option<DisplayMode>,
    pub focus_handle: FocusHandle,
}

impl FileBrowserView {
    pub fn new(
        path: PathBuf,
        search: Entity<SearchService>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Start with empty entries, load in background
        let delegate = FileBrowserDelegate::new(Vec::new());

        let table_state = cx.new(|cx| TableState::new(delegate, window, cx));

        let load_path = path.clone();
        let start = Instant::now();

        let view = Self {
            path,
            table_state,
            search,
            load_time_ms: 0.0,
            loading: true,
            error: None,
            inline_edit: None,
            inline_input: None,
            pending_trash: None,
            search_active: false,
            search_input: None,
            display_mode: None,
            focus_handle: cx.focus_handle(),
        };

        // Load directory entries in background
        cx.spawn(async move |this, cx| {
            let entries = cx
                .background_executor()
                .spawn(async move { state::load_directory(&load_path) })
                .await;

            let elapsed = start.elapsed().as_secs_f64() * 1000.0;

            this.update(cx, |view, cx| {
                view.loading = false;
                view.load_time_ms = elapsed;
                view.table_state.update(cx, |state, cx| {
                    state.delegate_mut().entries = entries;
                    cx.notify();
                });
                cx.notify();
            })
            .ok();
        })
        .detach();

        view
    }

    #[allow(dead_code)]
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    fn render_summary_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let delegate = self.table_state.read(cx).delegate();
        let entries = &delegate.entries;

        let file_count = entries.iter().filter(|e| !e.is_dir).count();
        let folder_count = entries.iter().filter(|e| e.is_dir).count();
        let total_size: u64 = entries.iter().map(|e| e.size).sum();

        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;

        // Search results mode shows a different summary
        if let Some(super::search_bar::DisplayMode::SearchResults { ref query }) = self.display_mode
        {
            let total = file_count + folder_count;
            return h_flex()
                .w_full()
                .px_3()
                .py_1()
                .items_center()
                .justify_between()
                .child(
                    h_flex()
                        .gap(px(4.0))
                        .items_center()
                        .text_size(FONT_SIZE_CAPTION)
                        .child(div().text_color(brand_color()).child("\u{1F50D}"))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child(SharedString::from(total.to_string())),
                        )
                        .child(
                            div()
                                .text_color(muted)
                                .child(SharedString::from(format!("results for \"{}\"", query))),
                        ),
                )
                .child(div().text_size(FONT_SIZE_CAPTION).text_color(muted).child(
                    SharedString::from(state::format_size(total_size).to_string()),
                ));
        }

        let path_str = self.path.to_string_lossy().to_string();
        let time_str = format_load_time(self.load_time_ms);

        h_flex()
            .w_full()
            .px_3()
            .py_1()
            .items_center()
            .justify_between()
            .child(
                h_flex()
                    .gap(px(4.0))
                    .items_center()
                    .text_size(FONT_SIZE_CAPTION)
                    // Zap icon
                    .child(div().text_color(brand_color()).child("\u{26A1}"))
                    // Stats with mixed weights
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child(SharedString::from(file_count.to_string())),
                    )
                    .child(div().text_color(muted).child(SharedString::from(format!(
                        "files ({}),",
                        state::format_size(total_size),
                    ))))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child(SharedString::from(folder_count.to_string())),
                    )
                    .child(div().text_color(muted).child("folders in"))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child(SharedString::from(time_str)),
                    ),
            )
            .child(
                div()
                    .text_size(FONT_SIZE_CAPTION)
                    .text_color(muted)
                    .child(SharedString::from(path_str)),
            )
    }
}

impl FileBrowserView {
    pub fn start_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry(cx) else {
            return;
        };
        let name = entry
            .path
            .file_name()
            .map(|n: &std::ffi::OsStr| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let idx = self
            .table_state
            .read(cx)
            .delegate()
            .selected
            .first()
            .copied()
            .unwrap_or(0);
        let input = cx.new(|cx| InputState::new(window, cx).default_value(&name));
        self.inline_edit = Some(InlineEdit::Rename { idx });
        self.inline_input = Some(input);
        cx.notify();
    }

    pub fn start_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Folder name..."));
        self.inline_edit = Some(InlineEdit::NewFolder);
        self.inline_input = Some(input);
        cx.notify();
    }

    fn confirm_inline_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = &self.inline_input else {
            return;
        };
        let value = input.read(cx).value().to_string();
        let value = value.trim();

        if value.is_empty() {
            self.cancel_inline_edit(cx);
            return;
        }

        match &self.inline_edit {
            Some(InlineEdit::Rename { idx }) => {
                let entries = &self.table_state.read(cx).delegate().entries;
                if let Some(entry) = entries.get(*idx) {
                    let old_path = &entry.path;
                    let new_path = old_path.with_file_name(value);
                    if new_path != *old_path
                        && let Err(e) = std::fs::rename(old_path, &new_path)
                    {
                        eprintln!("[zero-ui] rename error: {}", e);
                    }
                }
            }
            Some(InlineEdit::NewFolder) => {
                let new_path = self.path.join(value);
                if let Err(e) = std::fs::create_dir(&new_path) {
                    eprintln!("[zero-ui] create folder error: {}", e);
                }
            }
            None => {}
        }

        self.inline_edit = None;
        self.inline_input = None;
        self.reload(cx);
        self.focus_handle.focus(window);
    }

    fn cancel_inline_edit(&mut self, cx: &mut Context<Self>) {
        self.inline_edit = None;
        self.inline_input = None;
        cx.notify();
    }

    fn render_inline_edit(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let edit = self.inline_edit.as_ref()?;
        let input = self.inline_input.as_ref()?;

        let label = match edit {
            InlineEdit::Rename { .. } => "Rename:",
            InlineEdit::NewFolder => "New folder:",
        };

        Some(
            h_flex()
                .w_full()
                .px_3()
                .py_1()
                .gap_2()
                .items_center()
                .border_b_1()
                .border_color(cx.theme().border)
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    if ev.keystroke.key == "enter" {
                        this.confirm_inline_edit(window, cx);
                    } else if ev.keystroke.key == "escape" {
                        this.cancel_inline_edit(cx);
                    }
                }))
                .child(
                    div()
                        .text_size(FONT_SIZE_BODY)
                        .text_color(cx.theme().muted_foreground)
                        .child(label),
                )
                .child(div().flex_1().child(Input::new(input)))
                .into_any_element(),
        )
    }
}

impl Render for FileBrowserView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_empty = self.table_state.read(cx).delegate().entries.is_empty();

        // Build confirm dialog overlay if pending
        let trash_dialog = self.pending_trash.as_ref().map(|path| {
            let name = path
                .file_name()
                .map(|n: &std::ffi::OsStr| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());

            let confirm_entity = cx.entity().clone();
            let cancel_entity = cx.entity().clone();

            ConfirmDialog::new(
                "Move to Trash",
                format!("Are you sure you want to move \"{}\" to the Trash?", name),
                move |_window, cx| {
                    confirm_entity.update(cx, |this, cx| this.confirm_trash(cx));
                },
                move |_window, cx| {
                    cancel_entity.update(cx, |this, cx| this.cancel_trash(cx));
                },
            )
            .confirm_label("Move to Trash")
            .destructive()
            .render_element(window, cx)
        });

        div()
            .relative()
            .size_full()
            .child(
                v_flex()
                    .track_focus(&self.focus_handle)
                    .size_full()
                    .bg(theme::content_bg(cx))
                    .on_action(cx.listener(|this, _: &OpenSelected, _, cx| {
                        this.open_selected(cx);
                    }))
                    .on_action(cx.listener(|this, _: &RevealInFinder, _, cx| {
                        this.reveal_selected(cx);
                    }))
                    .on_action(cx.listener(|this, _: &QuickLook, _, cx| {
                        this.quick_look_selected(cx);
                    }))
                    .on_action(cx.listener(|this, _: &CopyPath, _, cx| {
                        this.copy_path_selected(cx);
                    }))
                    .on_action(cx.listener(|this, _: &MoveToTrash, _, cx| {
                        this.trash_selected(cx);
                    }))
                    .on_action(cx.listener(|this, _: &Refresh, _, cx| {
                        this.reload(cx);
                    }))
                    .on_action(cx.listener(|this, _: &SelectPrev, _, cx| {
                        this.table_state.update(cx, |state, cx| {
                            state.delegate_mut().select_prev();
                            cx.notify();
                        });
                        cx.notify();
                    }))
                    .on_action(cx.listener(|this, _: &SelectNext, _, cx| {
                        this.table_state.update(cx, |state, cx| {
                            state.delegate_mut().select_next();
                            cx.notify();
                        });
                        cx.notify();
                    }))
                    .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                        this.table_state.update(cx, |state, cx| {
                            state.delegate_mut().select_all();
                            cx.notify();
                        });
                        cx.notify();
                    }))
                    .on_action(cx.listener(|this, _: &Rename, window, cx| {
                        this.start_rename(window, cx);
                    }))
                    .on_action(cx.listener(|this, _: &NewFolder, window, cx| {
                        this.start_new_folder(window, cx);
                    }))
                    .on_action(cx.listener(|this, _: &CopyFiles, _, cx| {
                        this.copy_files(cx);
                    }))
                    .on_action(cx.listener(|this, _: &CutFiles, _, cx| {
                        this.cut_files(cx);
                    }))
                    .on_action(cx.listener(|this, _: &DuplicateFiles, _, cx| {
                        this.duplicate_files(cx);
                    }))
                    .on_action(cx.listener(|this, _: &AddToBookmarks, _, cx| {
                        this.add_to_bookmarks(cx);
                    }))
                    .on_action(cx.listener(|this, _: &NewTodoFile, _, cx| {
                        this.new_todo_file(cx);
                    }))
                    .on_action(cx.listener(|this, _: &FindDuplicatesHere, _, cx| {
                        this.find_duplicates_here(cx);
                    }))
                    .on_action(cx.listener(|this, _: &MoveToOtherPane, _, cx| {
                        this.move_to_other_pane(cx);
                    }))
                    .on_action(cx.listener(|this, _: &CopyToOtherPane, _, cx| {
                        this.copy_to_other_pane(cx);
                    }))
                    .on_action(cx.listener(|this, _: &FindInBrowser, window, cx| {
                        this.toggle_search(window, cx);
                    }))
                    .when(!self.loading, |el| el.child(self.render_summary_bar(cx)))
                    // Search bar
                    .when_some(self.render_search_bar(cx), |el, bar| el.child(bar))
                    // Inline edit bar (rename or new folder)
                    .when_some(self.render_inline_edit(cx), |el, edit_bar| {
                        el.child(edit_bar)
                    })
                    .when(self.loading, |el| {
                        el.child(
                            EmptyState::new(IconName::FolderOpen, "Loading...")
                                .subtitle("Reading directory contents"),
                        )
                    })
                    .when(!self.loading && is_empty, |el| {
                        el.child(
                            EmptyState::new(IconName::FolderOpen, "This folder is empty")
                                .subtitle("No files or folders to display"),
                        )
                    })
                    .when(!self.loading && !is_empty, |el| {
                        el.child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .child(Table::new(&self.table_state)),
                        )
                    }),
            )
            // Confirm dialog overlay
            .when_some(trash_dialog, |el, dialog| el.child(dialog))
    }
}

/// Format load timing: <1ms → "0.54ms", 1-10ms → "5.8ms", 10ms+ → "156ms"
fn format_load_time(ms: f64) -> String {
    if ms < 1.0 {
        format!("{:.2}ms", ms)
    } else if ms < 10.0 {
        format!("{:.1}ms", ms)
    } else {
        format!("{:.0}ms", ms)
    }
}
