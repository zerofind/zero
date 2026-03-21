use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName, InteractiveElementExt as _, Sizable as _, h_flex,
    input::{Input, InputState},
    table::{Column, ColumnSort, TableDelegate, TableState},
};

use crate::actions::ShowColumnMenu;
use crate::models::{SortDirection, SortField};
use crate::theme::{FONT_SIZE_BODY, FONT_SIZE_CAPTION, ICON_XS};
use crate::ui::FileIcon;

use super::columns::FileColumn;
use super::git::{GitFileStatus, GitInfo};
use super::state::{self, BrowserEntry, SymlinkInfo};

/// The configurable file browser table delegate.
pub struct FileBrowserDelegate {
    pub entries: Vec<BrowserEntry>,
    pub sort_field: SortField,
    pub sort_direction: SortDirection,
    pub selected: Vec<usize>,
    pub split_active: bool,
    pub visible_columns: Vec<FileColumn>,
    pub git_info: Option<GitInfo>,
    columns: Vec<Column>,
    /// Row index currently being inline-renamed.
    pub editing_row: Option<usize>,
    /// Input state for inline rename.
    pub editing_input: Option<Entity<InputState>>,
}

impl FileBrowserDelegate {
    pub fn new(entries: Vec<BrowserEntry>, visible_columns: Vec<FileColumn>) -> Self {
        let mut delegate = Self {
            entries,
            sort_field: SortField::Name,
            sort_direction: SortDirection::Ascending,
            selected: Vec::new(),
            split_active: false,
            visible_columns,
            git_info: None,
            columns: Vec::new(),
            editing_row: None,
            editing_input: None,
        };
        delegate.rebuild_columns();
        delegate
    }

    fn sort_field_for_column(col: FileColumn) -> SortField {
        match col {
            FileColumn::Name => SortField::Name,
            FileColumn::Location => SortField::Location,
            FileColumn::DateModified => SortField::DateModified,
            FileColumn::Size => SortField::Size,
            FileColumn::Kind => SortField::Kind,
            FileColumn::Permissions => SortField::Permissions,
            FileColumn::Owner => SortField::Owner,
            // Attributes is not sortable
            FileColumn::Attributes => SortField::Name,
        }
    }

    pub(super) fn rebuild_columns(&mut self) {
        self.columns = self
            .visible_columns
            .iter()
            .map(|fc| {
                let label = fc.label().to_string();
                let mut col = Column::new(label.clone(), label).width(fc.default_width());

                if fc.sortable() {
                    col = col.sortable();
                }
                if fc.right_aligned() {
                    col = col.text_right();
                }

                let field = Self::sort_field_for_column(*fc);
                if fc.sortable() && field == self.sort_field {
                    col = match self.sort_direction {
                        SortDirection::Ascending => col.ascending(),
                        SortDirection::Descending => col.descending(),
                    };
                }
                col
            })
            .collect();
    }

    /// Update visible columns and rebuild.
    pub fn set_visible_columns(&mut self, cols: Vec<FileColumn>) {
        self.visible_columns = cols;
        self.rebuild_columns();
    }
}

impl TableDelegate for FileBrowserDelegate {
    fn render_header(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        div()
            .id("header")
            .on_mouse_down(MouseButton::Right, move |_ev, window, cx| {
                window.dispatch_action(Box::new(ShowColumnMenu), cx);
            })
    }

    fn move_column(
        &mut self,
        col_ix: usize,
        to_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) {
        if col_ix == to_ix || col_ix >= self.visible_columns.len() {
            return;
        }
        let col = self.visible_columns.remove(col_ix);
        let insert_at = to_ix.min(self.visible_columns.len());
        self.visible_columns.insert(insert_at, col);
        self.rebuild_columns();

        // Persist new order
        let names = super::columns::columns_to_names(&self.visible_columns);
        let mut settings = crate::session::Settings::load();
        settings.visible_columns = names;
        settings.save();
    }

    fn columns_count(&self, _cx: &App) -> usize {
        self.visible_columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.entries.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> &Column {
        &self.columns[col_ix]
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        _sort: ColumnSort,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let Some(fc) = self.visible_columns.get(col_ix).copied() else {
            return;
        };
        if !fc.sortable() {
            return;
        }

        let field = Self::sort_field_for_column(fc);
        if field == self.sort_field {
            self.sort_direction = self.sort_direction.toggle();
        } else {
            self.sort_field = field;
            self.sort_direction = SortDirection::Ascending;
        }

        state::sort_entries(&mut self.entries, self.sort_field, self.sort_direction);
        self.rebuild_columns();
        cx.notify();
    }

    #[allow(refining_impl_trait)]
    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> AnyElement {
        let Some(entry) = self.entries.get(row_ix) else {
            return div().into_any_element();
        };

        let Some(fc) = self.visible_columns.get(col_ix).copied() else {
            return div().into_any_element();
        };

        let muted = cx.theme().muted_foreground;

        let git_status = self
            .git_info
            .as_ref()
            .and_then(|gi| gi.status(&entry.path))
            .copied();

        match fc {
            FileColumn::Name => {
                if self.editing_row == Some(row_ix)
                    && let Some(ref input) = self.editing_input
                {
                    return render_name_editing(entry, input, cx);
                }
                render_name_column(entry, row_ix, git_status, cx)
            }
            FileColumn::Location => render_location_column(entry, cx),
            FileColumn::DateModified => div()
                .h_full()
                .flex()
                .items_center()
                .text_size(FONT_SIZE_CAPTION)
                .text_color(muted)
                .child(SharedString::from(state::format_date(entry.mtime)))
                .into_any_element(),
            FileColumn::Size => div()
                .h_full()
                .flex()
                .items_center()
                .justify_end()
                .text_size(FONT_SIZE_CAPTION)
                .text_color(muted)
                .child(SharedString::from(state::format_size(entry.size)))
                .into_any_element(),
            FileColumn::Kind => {
                let kind = SharedString::from(entry.kind_label().to_string());
                div()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_size(FONT_SIZE_CAPTION)
                    .text_color(muted)
                    .child(kind)
                    .into_any_element()
            }
            FileColumn::Permissions => render_permissions_column(entry, cx),
            FileColumn::Owner => render_owner_column(entry, cx),
            FileColumn::Attributes => render_flags_column(entry, cx),
        }
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let is_selected = self.selected.contains(&row_ix);

        let mut row = div()
            .id(("row", row_ix))
            .on_double_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                window.dispatch_action(Box::new(crate::actions::OpenSelected), cx);
            })
            .on_click({
                let table = cx.entity().clone();
                move |event: &ClickEvent, _, cx| {
                    table.update(cx, |state, cx| {
                        let mods = event.modifiers();
                        let delegate = state.delegate_mut();
                        if mods.platform {
                            delegate.toggle_select(row_ix);
                        } else if mods.shift {
                            delegate.range_select(row_ix);
                        } else {
                            delegate.select(row_ix);
                        }
                        cx.notify();
                    });
                }
            });

        if is_selected {
            row = row.bg(crate::theme::surface_active(cx));
        }

        row
    }

    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: gpui_component::menu::PopupMenu,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> gpui_component::menu::PopupMenu {
        if !self.selected.contains(&row_ix) {
            self.selected = vec![row_ix];
        }

        super::context_menu::build_context_menu(
            &self.entries,
            row_ix,
            self.split_active,
            menu,
            window,
            cx,
        )
    }
}

// -- Column renderers ---------------------------------------------------------

fn render_name_column(
    entry: &BrowserEntry,
    row_ix: usize,
    git_status: Option<GitFileStatus>,
    cx: &mut Context<TableState<FileBrowserDelegate>>,
) -> AnyElement {
    let indent = entry.depth as f32 * 20.0;
    let is_dir = entry.is_dir;
    let expanded = entry.expanded;
    let table = cx.entity().clone();
    let muted = cx.theme().muted_foreground;
    let is_broken_symlink = matches!(&entry.symlink_target, Some(SymlinkInfo::Broken(_)));

    let name_color = if is_broken_symlink {
        cx.theme().danger
    } else if let Some(status) = git_status {
        match status {
            GitFileStatus::Modified => cx.theme().warning,
            GitFileStatus::New => cx.theme().success,
            GitFileStatus::Deleted => cx.theme().danger,
            GitFileStatus::Ignored => {
                let mut c = cx.theme().muted_foreground;
                c.a *= 0.5;
                c
            }
        }
    } else {
        cx.theme().foreground
    };

    let mut el = h_flex()
        .h_full()
        .overflow_hidden()
        .items_center()
        .gap_1p5()
        .pl(px(indent))
        // Disclosure chevron for directories
        .when(is_dir, |el| {
            let icon = if expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            };
            let table = table.clone();
            el.child(
                div()
                    .flex_shrink_0()
                    .id(("chevron", row_ix))
                    .cursor_pointer()
                    .child(Icon::new(icon).with_size(ICON_XS).text_color(muted))
                    .on_click(move |_, _, cx| {
                        table.update(cx, |state, cx| {
                            let delegate = state.delegate_mut();
                            let was_expanded = delegate.entries[row_ix].expanded;
                            let count = state::toggle_expand(&mut delegate.entries, row_ix);

                            if was_expanded {
                                delegate
                                    .selected
                                    .retain(|&idx| idx <= row_ix || idx > row_ix + count);
                                for idx in delegate.selected.iter_mut() {
                                    if *idx > row_ix + count {
                                        *idx -= count;
                                    }
                                }
                            } else {
                                for idx in delegate.selected.iter_mut() {
                                    if *idx > row_ix {
                                        *idx += count;
                                    }
                                }
                            }

                            cx.notify();
                        });
                    }),
            )
        })
        .when(!is_dir, |el| el.child(div().flex_shrink_0().w(px(10.0))))
        .child(
            div()
                .flex_shrink_0()
                .child(FileIcon::new(entry.extension.as_deref(), entry.is_dir)),
        )
        .child(
            div()
                .flex_1()
                .text_size(FONT_SIZE_BODY)
                .text_color(name_color)
                .text_ellipsis()
                .whitespace_nowrap()
                .min_w_0()
                .child(SharedString::from(entry.name.clone())),
        );

    // Append symlink target after the name
    if let Some(ref target) = entry.symlink_target {
        let (arrow_text, arrow_color) = match target {
            SymlinkInfo::Valid(t) => (format!(" \u{2192} {t}"), muted),
            SymlinkInfo::Broken(t) => (format!(" \u{2192} {t}"), cx.theme().danger),
        };
        el = el.child(
            div()
                .text_size(FONT_SIZE_CAPTION)
                .text_color(arrow_color)
                .text_ellipsis()
                .whitespace_nowrap()
                .min_w_0()
                .flex_shrink()
                .child(SharedString::from(arrow_text)),
        );
    }

    el.into_any_element()
}

fn render_location_column(
    entry: &BrowserEntry,
    cx: &mut Context<TableState<FileBrowserDelegate>>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;

    let location = entry
        .path
        .parent()
        .map(|p| {
            // Abbreviate home dir to ~
            if let Some(home) = dirs::home_dir()
                && let Ok(suffix) = p.strip_prefix(&home)
            {
                return format!("~/{}", suffix.display());
            }
            p.to_string_lossy().to_string()
        })
        .unwrap_or_default();

    div()
        .h_full()
        .flex()
        .items_center()
        .text_size(FONT_SIZE_CAPTION)
        .text_color(muted)
        .text_ellipsis()
        .whitespace_nowrap()
        .min_w_0()
        .child(SharedString::from(location))
        .into_any_element()
}

fn render_permissions_column(
    entry: &BrowserEntry,
    cx: &mut Context<TableState<FileBrowserDelegate>>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;

    let Some(mode) = entry.mode else {
        return div()
            .h_full()
            .flex()
            .items_center()
            .text_size(FONT_SIZE_CAPTION)
            .text_color(muted)
            .child("-")
            .into_any_element();
    };

    let perm_str = state::format_mode(mode, entry.is_dir, entry.is_symlink);
    let xattr_suffix = if entry.has_xattrs { "@" } else { "" };
    let has_special = mode & 0o7000 != 0;
    let text_color = if has_special {
        cx.theme().warning
    } else {
        muted
    };

    let display = format!("{perm_str}{xattr_suffix}");
    div()
        .h_full()
        .flex()
        .items_center()
        .text_size(FONT_SIZE_CAPTION)
        .text_color(text_color)
        .child(SharedString::from(display))
        .into_any_element()
}

fn render_owner_column(
    entry: &BrowserEntry,
    cx: &mut Context<TableState<FileBrowserDelegate>>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;

    let Some(ref owner) = entry.owner else {
        return div()
            .h_full()
            .flex()
            .items_center()
            .text_size(FONT_SIZE_CAPTION)
            .text_color(muted)
            .child("-")
            .into_any_element();
    };

    // Dim the group when it equals the user (common case, reduces noise)
    let group_color = if owner.group == owner.user {
        let mut c = muted;
        c.a *= 0.6;
        c
    } else {
        muted
    };

    // Dim the whole thing if it's the current user
    let user_color = {
        #[cfg(unix)]
        {
            let current_uid = unsafe { libc::getuid() };
            let is_current =
                entry.mode.is_some() && resolve_uid_matches_name(current_uid, &owner.user);
            if is_current {
                let mut c = muted;
                c.a *= 0.7;
                c
            } else {
                muted
            }
        }
        #[cfg(not(unix))]
        {
            muted
        }
    };

    h_flex()
        .h_full()
        .items_center()
        .text_size(FONT_SIZE_CAPTION)
        .child(
            div()
                .text_color(user_color)
                .child(SharedString::from(owner.user.clone())),
        )
        .child(div().text_color(group_color).child(":"))
        .child(
            div()
                .text_color(group_color)
                .child(SharedString::from(owner.group.clone())),
        )
        .into_any_element()
}

#[cfg(unix)]
fn resolve_uid_matches_name(uid: u32, name: &str) -> bool {
    let pw = unsafe { libc::getpwuid(uid) };
    if pw.is_null() {
        return uid.to_string() == name;
    }
    let pw_name = unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) };
    pw_name.to_string_lossy() == name
}

fn render_name_editing(
    entry: &BrowserEntry,
    input: &Entity<InputState>,
    _cx: &mut Context<TableState<FileBrowserDelegate>>,
) -> AnyElement {
    let indent = entry.depth as f32 * 20.0;
    let is_dir = entry.is_dir;

    h_flex()
        .h_full()
        .overflow_hidden()
        .items_center()
        .gap_1p5()
        .pl(px(indent))
        .when(is_dir, |el| {
            el.child(div().flex_shrink_0().w(px(26.0))) // chevron space
        })
        .when(!is_dir, |el| el.child(div().flex_shrink_0().w(px(10.0))))
        .child(
            div()
                .flex_shrink_0()
                .child(FileIcon::new(entry.extension.as_deref(), entry.is_dir)),
        )
        .child(div().flex_1().child(Input::new(input)))
        .into_any_element()
}

fn render_flags_column(
    entry: &BrowserEntry,
    cx: &mut Context<TableState<FileBrowserDelegate>>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;

    let Some(ref flags) = entry.flags else {
        return div()
            .h_full()
            .flex()
            .items_center()
            .text_size(FONT_SIZE_CAPTION)
            .text_color(muted)
            .child("-")
            .into_any_element();
    };

    // Highlight `uchg` (immutable) in danger color since it blocks operations
    let color = if flags.contains("uchg") {
        cx.theme().danger
    } else {
        muted
    };

    div()
        .h_full()
        .flex()
        .items_center()
        .text_size(FONT_SIZE_CAPTION)
        .text_color(color)
        .child(SharedString::from(flags.clone()))
        .into_any_element()
}
