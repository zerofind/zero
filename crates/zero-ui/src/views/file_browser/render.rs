use std::path::PathBuf;
use std::time::Instant;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, h_flex,
    input::{Enter as InputEnter, Escape as InputEscape, InputState},
    table::{Table, TableEvent, TableState},
    v_flex,
};

use crate::actions::{
    AddToBookmarks, CopyFiles, CopyPath, CopyToOtherPane, CutFiles, DuplicateFiles,
    FindDuplicatesHere, FindInBrowser, MoveToOtherPane, MoveToTrash, NewFolder, NewTodoFile,
    OpenSelected, QuickLook, Refresh, Rename, RevealInFinder, SelectAll, ShowColumnMenu,
};
use crate::services::SearchService;
use crate::theme::{self, FONT_SIZE_BODY, RADIUS};
use crate::ui::{EmptyState, StatusBar, StatusBarMode};

use super::delegate::FileBrowserDelegate;
use super::git::GitInfo;
use super::search_bar::DisplayMode;
use super::state::{self, BrowserEntry};

pub struct FileBrowserView {
    pub(super) path: PathBuf,
    pub table_state: Entity<TableState<FileBrowserDelegate>>,
    #[allow(dead_code)]
    pub(super) search: Entity<SearchService>,
    pub(super) load_time_ms: f64,
    #[allow(dead_code)]
    pub(super) loading: bool,
    #[allow(dead_code)]
    pub(super) error: Option<String>,
    // Search
    pub(super) search_active: bool,
    pub(super) search_input: Option<Entity<InputState>>,
    pub(super) display_mode: Option<DisplayMode>,
    pub(super) column_menu_open: bool,
    pub focus_handle: FocusHandle,
    // Type-ahead navigation
    pub(super) typeahead_buffer: String,
    pub(super) typeahead_last_key: Option<Instant>,
    // Subscriptions kept alive during inline editing (blur handler, etc.)
    editing_subs: Vec<Subscription>,
}

impl FileBrowserView {
    pub fn new(
        path: PathBuf,
        search: Entity<SearchService>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Start with empty entries, load in background
        let settings = crate::session::Settings::load();
        let visible_columns = super::columns::columns_from_names(&settings.visible_columns);
        let delegate = FileBrowserDelegate::new(Vec::new(), visible_columns);

        let table_state = cx.new(|cx| TableState::new(delegate, window, cx).col_selectable(false));

        // Sync Table's keyboard selection → delegate.selected.
        // Guard: if click handler already built a multi-selection containing
        // this row, skip — otherwise keyboard nav resets to single select.
        cx.subscribe(&table_state, |_this, table, event: &TableEvent, cx| {
            if let TableEvent::SelectRow(row_ix) = event {
                table.update(cx, |state, _cx| {
                    let delegate = state.delegate_mut();
                    if delegate.selected.len() <= 1 || !delegate.selected.contains(row_ix) {
                        delegate.select(*row_ix);
                    }
                });
                cx.notify();
            }
        })
        .detach();

        let load_path = path.clone();
        let start = Instant::now();

        // Share the Table's focus handle so the wrapping v_flex and the Table
        // always agree on who is focused. This prevents clicks on the status-bar
        // or other non-Table areas from stealing focus to a competing handle.
        let focus_handle = table_state.focus_handle(cx);

        let view = Self {
            path,
            table_state,
            search,
            load_time_ms: 0.0,
            loading: true,
            error: None,
            search_active: false,
            search_input: None,
            display_mode: None,
            column_menu_open: false,
            focus_handle,
            typeahead_buffer: String::new(),
            typeahead_last_key: None,
            editing_subs: Vec::new(),
        };

        // Load directory entries and git info in background
        cx.spawn(async move |this, cx| {
            let git_path = load_path.clone();
            let entries = cx
                .background_executor()
                .spawn(async move { state::load_directory(&load_path) })
                .await;
            let git_info = cx
                .background_executor()
                .spawn(async move { GitInfo::discover(&git_path) })
                .await;

            let elapsed = start.elapsed().as_secs_f64() * 1000.0;

            this.update(cx, |view, cx| {
                view.loading = false;
                view.load_time_ms = elapsed;
                // Don't overwrite entries if a search/type view was applied
                // before the background directory load finished.
                if view.display_mode.is_none() {
                    let has_git = git_info.is_some();
                    view.table_state.update(cx, |state, cx| {
                        let delegate = state.delegate_mut();
                        delegate.entries = entries;
                        delegate.git_info = git_info;
                        cx.notify();
                    });
                    if has_git {
                        cx.emit(super::actions::FileBrowserEvent::GitStateChanged);
                    }
                }
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

    /// Returns the search query if currently showing global search results.
    pub fn search_results_query(&self) -> Option<&str> {
        match &self.display_mode {
            Some(DisplayMode::SearchResults { query }) => Some(query),
            _ => None,
        }
    }

    fn render_summary_bar(&self, cx: &mut Context<Self>) -> StatusBar {
        let delegate = self.table_state.read(cx).delegate();
        let entries = &delegate.entries;

        let file_count = entries.iter().filter(|e| !e.is_dir).count();
        let folder_count = entries.iter().filter(|e| e.is_dir).count();
        let total_size: u64 = entries.iter().map(|e| e.size).sum();

        let selected_count = delegate.selected.len();
        let selected_size: u64 = delegate
            .selected
            .iter()
            .filter_map(|&idx| entries.get(idx))
            .map(|e| e.size)
            .sum();

        if let Some(super::search_bar::DisplayMode::SearchResults { ref query }) = self.display_mode
        {
            return StatusBar::new(StatusBarMode::SearchResults {
                total: file_count + folder_count,
                query: query.clone(),
                total_size,
            });
        }

        StatusBar::new(StatusBarMode::Directory {
            file_count,
            folder_count,
            total_size,
            selected_count,
            selected_size,
            load_time: format_load_time(self.load_time_ms),
            path: self.path.to_string_lossy().to_string(),
        })
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

        tracing::debug!(path = %entry.path.display(), "browser: start rename");

        let input = cx.new(|cx| InputState::new(window, cx).default_value(&name));
        let focus = input.focus_handle(cx);
        focus.focus(window);

        // Confirm rename when input loses focus (click outside)
        let blur_sub = cx.on_blur(&focus, window, |this, window, cx| {
            if this.is_editing(cx) {
                this.confirm_rename(window, cx);
            }
        });
        self.editing_subs = vec![blur_sub];

        self.table_state.update(cx, |state, _cx| {
            let delegate = state.delegate_mut();
            delegate.editing_row = Some(idx);
            delegate.editing_input = Some(input);
        });
        cx.notify();
    }

    /// Create "untitled folder" on disk, insert it into the list, and start
    /// inline rename — Finder-style.
    pub fn start_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_path = super::actions::unique_path(&self.path.join("untitled folder"));

        tracing::debug!(path = %new_path.display(), "browser: new folder");
        if let Err(e) = std::fs::create_dir(&new_path) {
            tracing::error!(error = %e, "create folder failed");
            return;
        }

        let Some(entry) = BrowserEntry::from_fs(&new_path, 0) else {
            self.reload(cx);
            return;
        };

        let name = entry.name.clone();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(&name));
        let focus = input.focus_handle(cx);
        focus.focus(window);

        // Confirm rename when input loses focus (click outside)
        let blur_sub = cx.on_blur(&focus, window, |this, window, cx| {
            if this.is_editing(cx) {
                this.confirm_rename(window, cx);
            }
        });
        self.editing_subs = vec![blur_sub];

        self.table_state.update(cx, |state, _cx| {
            let delegate = state.delegate_mut();

            // Insert at correct sorted position (dirs first, natural sort)
            let insert_idx = delegate
                .entries
                .iter()
                .position(|e| {
                    if e.is_dir {
                        natord::compare_ignore_case(&name, &e.name) == std::cmp::Ordering::Less
                    } else {
                        true // dirs go before all files
                    }
                })
                .unwrap_or(delegate.entries.len());

            delegate.entries.insert(insert_idx, entry);
            delegate.selected = vec![insert_idx];
            delegate.editing_row = Some(insert_idx);
            delegate.editing_input = Some(input);
        });
        cx.notify();
    }

    fn confirm_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (editing_row, value) = {
            let delegate = self.table_state.read(cx).delegate();
            let Some(row) = delegate.editing_row else {
                return;
            };
            let Some(ref input) = delegate.editing_input else {
                return;
            };
            (row, input.read(cx).value().to_string())
        };

        let value = value.trim();
        if value.is_empty() {
            self.cancel_rename(window, cx);
            return;
        }

        let entries = &self.table_state.read(cx).delegate().entries;
        if let Some(entry) = entries.get(editing_row) {
            let old_path = &entry.path;
            let new_path = old_path.with_file_name(value);
            if new_path != *old_path {
                tracing::debug!(from = %old_path.display(), to = %new_path.display(), "browser: rename");
                if let Err(e) = std::fs::rename(old_path, &new_path) {
                    tracing::error!(error = %e, "rename failed");
                }
            }
        }

        self.clear_editing(window, cx);
        self.reload(cx);
    }

    fn cancel_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_editing(window, cx);
        cx.notify();
    }

    fn clear_editing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing_subs.clear();
        self.table_state.update(cx, |state, _cx| {
            let delegate = state.delegate_mut();
            delegate.editing_row = None;
            delegate.editing_input = None;
        });
        self.table_state.focus_handle(cx).focus(window);
    }

    pub(super) fn is_editing(&self, cx: &Context<Self>) -> bool {
        self.table_state.read(cx).delegate().editing_row.is_some()
    }

    fn toggle_column(&mut self, column: super::columns::FileColumn, cx: &mut Context<Self>) {
        use super::columns::{columns_from_names, columns_to_names};

        let mut settings = crate::session::Settings::load();
        let mut cols = columns_from_names(&settings.visible_columns);

        if let Some(pos) = cols.iter().position(|c| *c == column) {
            cols.remove(pos);
        } else {
            cols.push(column);
        }

        settings.visible_columns = columns_to_names(&cols);
        settings.save();

        self.table_state.update(cx, |state, cx| {
            state.delegate_mut().set_visible_columns(cols);
            state.refresh(cx);
            cx.notify();
        });
    }

    fn render_column_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.column_menu_open {
            return None;
        }

        use super::columns::FileColumn;

        let visible = self.table_state.read(cx).delegate().visible_columns.clone();

        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;

        let items: Vec<_> = FileColumn::TOGGLEABLE
            .iter()
            .map(|col| {
                let is_visible = visible.contains(col);
                let col_value = *col;
                div()
                    .id(SharedString::from(col.label()))
                    .w_full()
                    .px_3()
                    .py(px(5.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(cx.theme().list_active))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_column(col_value, cx);
                    }))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .w(px(16.0))
                                    .text_size(FONT_SIZE_BODY)
                                    .text_color(fg)
                                    .child(if is_visible { "\u{2713}" } else { "" }),
                            )
                            .child(
                                div()
                                    .text_size(FONT_SIZE_BODY)
                                    .text_color(if is_visible { fg } else { muted })
                                    .child(col.label()),
                            ),
                    )
            })
            .collect();

        Some(
            div()
                .id("column-menu-panel")
                .absolute()
                .top(px(32.0))
                .right(px(0.0))
                .min_w(px(180.0))
                .bg(cx.theme().background)
                .border_1()
                .border_color(cx.theme().border)
                .rounded(RADIUS)
                .shadow_md()
                .py_1()
                .children(items)
                .into_any_element(),
        )
    }
}

impl Render for FileBrowserView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_empty = self.table_state.read(cx).delegate().entries.is_empty();

        div()
            .relative()
            .size_full()
            .child(
                v_flex()
                    .track_focus(&self.focus_handle)
                    .size_full()
                    .bg(theme::content_bg(cx))
                    .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                        if this.is_editing(cx) {
                            if ev.keystroke.key == "enter" {
                                this.confirm_rename(window, cx);
                            } else if ev.keystroke.key == "escape" {
                                this.cancel_rename(window, cx);
                            }
                            return;
                        }
                        this.handle_typeahead(ev, cx);
                    }))
                    // Inline editing: Enter confirms, Escape cancels (propagated from Input)
                    .on_action(cx.listener(|this, _: &InputEnter, window, cx| {
                        if this.is_editing(cx) {
                            this.confirm_rename(window, cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
                        if this.is_editing(cx) {
                            this.cancel_rename(window, cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &OpenSelected, _, cx| {
                        if !this.is_editing(cx) {
                            this.open_selected(cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &RevealInFinder, _, cx| {
                        this.reveal_selected(cx);
                    }))
                    .on_action(cx.listener(|this, _: &QuickLook, _, cx| {
                        if !this.is_editing(cx) {
                            this.quick_look_selected(cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &CopyPath, _, cx| {
                        this.copy_path_selected(cx);
                    }))
                    .on_action(cx.listener(|this, _: &MoveToTrash, _, cx| {
                        if !this.is_editing(cx) {
                            this.trash_selected(cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &Refresh, _, cx| {
                        this.reload(cx);
                    }))
                    .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                        this.table_state.update(cx, |state, cx| {
                            state.delegate_mut().select_all();
                            cx.notify();
                        });
                        cx.notify();
                    }))
                    .on_action(cx.listener(|this, _: &Rename, window, cx| {
                        if !this.is_editing(cx) {
                            this.start_rename(window, cx);
                        }
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
                    .on_action(cx.listener(|this, _: &ShowColumnMenu, _, cx| {
                        this.column_menu_open = !this.column_menu_open;
                        cx.notify();
                    }))
                    // Search bar
                    .when_some(self.render_search_bar(cx), |el, bar| el.child(bar))
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
                                .child(Table::new(&self.table_state).bordered(false)),
                        )
                    })
                    // Status bar pinned to bottom
                    .when(!self.loading, |el| el.child(self.render_summary_bar(cx))),
            )
            // Event barrier: blocks all interaction with content while menu is open
            .when(self.column_menu_open, |el| {
                el.child(
                    div()
                        .id("column-menu-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.column_menu_open = false;
                                cx.notify();
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(|this, _, _, cx| {
                                this.column_menu_open = false;
                                cx.notify();
                            }),
                        ),
                )
            })
            // Column menu overlay (rendered after backdrop so it's on top)
            .when_some(self.render_column_menu(cx), |el, menu| el.child(menu))
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
