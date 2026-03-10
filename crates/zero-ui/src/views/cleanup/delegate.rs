use std::collections::HashSet;
use std::path::PathBuf;

use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    table::{Column, ColumnSort, TableDelegate, TableState},
};

use crate::theme::{FONT_SIZE_BODY, RADIUS};
use crate::ui::format_size;

/// Pre-computed row for the cleanup detail table.
pub(super) struct CleanupRow {
    pub display_name: String,
    pub subtitle: String,
    pub size: u64,
    pub path: PathBuf,
}

pub(super) struct CleanupDetailDelegate {
    pub rows: Vec<CleanupRow>,
    pub selected: HashSet<usize>,
    /// Last clicked index — used as anchor for shift-click range select.
    anchor: Option<usize>,
    columns: Vec<Column>,
}

impl CleanupDetailDelegate {
    pub fn new(rows: Vec<CleanupRow>) -> Self {
        let columns = vec![
            Column::new("name", "Name").width(px(400.0)).sortable(),
            Column::new("type", "Type").width(px(160.0)).sortable(),
            Column::new("size", "Size")
                .width(px(100.0))
                .text_right()
                .sortable(),
        ];
        Self {
            rows,
            selected: HashSet::new(),
            anchor: None,
            columns,
        }
    }

    /// Plain click: clear selection, select only this row.
    pub fn select(&mut self, idx: usize) {
        self.selected.clear();
        if idx < self.rows.len() {
            self.selected.insert(idx);
        }
        self.anchor = Some(idx);
    }

    /// Cmd+click: toggle individual row without clearing others.
    pub fn toggle(&mut self, idx: usize) {
        if self.selected.contains(&idx) {
            self.selected.remove(&idx);
        } else {
            self.selected.insert(idx);
        }
        self.anchor = Some(idx);
    }

    /// Shift+click: range select from anchor to clicked row.
    pub fn range_select(&mut self, idx: usize) {
        let anchor = self.anchor.unwrap_or(0);
        let (start, end) = if idx > anchor {
            (anchor, idx)
        } else {
            (idx, anchor)
        };
        let end = end.min(self.rows.len().saturating_sub(1));
        for i in start..=end {
            self.selected.insert(i);
        }
    }

    pub fn toggle_all(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        if self.selected.len() == self.rows.len() {
            self.selected.clear();
        } else {
            self.selected = (0..self.rows.len()).collect();
        }
    }

    pub fn selected_bytes(&self) -> u64 {
        self.selected
            .iter()
            .filter_map(|&i| self.rows.get(i))
            .map(|r| r.size)
            .sum()
    }

    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected
            .iter()
            .filter_map(|&i| self.rows.get(i).map(|r| r.path.clone()))
            .collect()
    }

    pub fn focus_prev(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let idx = match self.anchor {
            Some(0) | None => 0,
            Some(i) => i - 1,
        };
        self.select(idx);
    }

    pub fn focus_next(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let max = self.rows.len().saturating_sub(1);
        let idx = match self.anchor {
            None => 0,
            Some(i) if i >= max => max,
            Some(i) => i + 1,
        };
        self.select(idx);
    }

    pub fn toggle_focused(&mut self) {
        if let Some(idx) = self.anchor {
            self.toggle(idx);
        }
    }

    pub fn open_focused(&self) {
        let Some(idx) = self.anchor else { return };
        let Some(_row) = self.rows.get(idx) else {
            return;
        };
        #[cfg(target_os = "macos")]
        crate::platform::open::open_path(&_row.path);
    }

    pub fn reveal_item(&self, idx: usize) {
        let Some(_row) = self.rows.get(idx) else {
            return;
        };
        #[cfg(target_os = "macos")]
        crate::platform::open::reveal_in_finder(&_row.path);
    }

    /// Remove rows whose paths are in `deleted`, returning bytes removed.
    pub fn remove_deleted(&mut self, deleted: &HashSet<PathBuf>) -> u64 {
        let mut bytes_removed = 0u64;
        self.rows.retain(|row| {
            if deleted.contains(&row.path) {
                bytes_removed += row.size;
                false
            } else {
                true
            }
        });
        self.selected.clear();
        self.anchor = None;
        bytes_removed
    }
}

impl TableDelegate for CleanupDetailDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> &Column {
        &self.columns[col_ix]
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let desc = matches!(sort, ColumnSort::Descending);
        match col_ix {
            0 => self.rows.sort_by(|a, b| {
                let cmp = a.display_name.cmp(&b.display_name);
                if desc { cmp.reverse() } else { cmp }
            }),
            1 => self.rows.sort_by(|a, b| {
                let cmp = a.subtitle.cmp(&b.subtitle);
                if desc { cmp.reverse() } else { cmp }
            }),
            2 => self.rows.sort_by(|a, b| {
                let cmp = a.size.cmp(&b.size);
                if desc { cmp.reverse() } else { cmp }
            }),
            _ => {}
        }
        self.selected.clear();
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
        let Some(row) = self.rows.get(row_ix) else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;

        match col_ix {
            // Name column: checkbox + path + reveal button
            0 => {
                let checked = self.selected.contains(&row_ix);
                let table = cx.entity().clone();
                let ix = row_ix;

                gpui_component::h_flex()
                    .h_full()
                    .items_center()
                    .gap_2()
                    .child(
                        Checkbox::new(SharedString::from(format!("chk-{row_ix}"))).checked(checked),
                    )
                    .child(
                        div().flex_1().min_w_0().overflow_hidden().child(
                            div()
                                .text_size(FONT_SIZE_BODY)
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(SharedString::from(row.display_name.clone())),
                        ),
                    )
                    .child(
                        div()
                            .invisible()
                            .group_hover("table-row", |s| s.visible())
                            .child(
                                Button::new(SharedString::from(format!("reveal-{row_ix}")))
                                    .icon(IconName::ExternalLink)
                                    .ghost()
                                    .compact()
                                    .xsmall()
                                    .tooltip("Reveal in Finder")
                                    .on_click(move |_, _, cx| {
                                        table.update(cx, |state, _| {
                                            state.delegate().reveal_item(ix);
                                        });
                                    }),
                            ),
                    )
                    .into_any_element()
            }
            // Type column
            1 => div()
                .h_full()
                .flex()
                .items_center()
                .text_size(FONT_SIZE_BODY)
                .text_color(muted)
                .text_ellipsis()
                .whitespace_nowrap()
                .child(SharedString::from(row.subtitle.clone()))
                .into_any_element(),
            // Size column
            2 => div()
                .h_full()
                .flex()
                .items_center()
                .justify_end()
                .text_size(FONT_SIZE_BODY)
                .child(SharedString::from(format_size(row.size)))
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let is_selected = self.selected.contains(&row_ix);
        let is_anchor = self.anchor == Some(row_ix);
        let table = cx.entity().clone();

        let mut row = div()
            .id(("cleanup-row", row_ix))
            .group("table-row")
            .rounded(RADIUS)
            .cursor_pointer()
            .on_click(move |event: &ClickEvent, _, cx| {
                table.update(cx, |state, cx| {
                    let mods = event.modifiers();
                    let d = state.delegate_mut();
                    if mods.platform {
                        d.toggle(row_ix);
                    } else if mods.shift {
                        d.range_select(row_ix);
                    } else {
                        d.select(row_ix);
                    }
                    cx.notify();
                });
            });

        if is_selected || is_anchor {
            row = row.bg(crate::theme::surface_hover(cx));
        }

        row
    }
}
