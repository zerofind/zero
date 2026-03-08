use std::collections::HashSet;
use std::path::PathBuf;

use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    table::{Column, ColumnSort, TableDelegate, TableState},
};

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION, RADIUS};
use crate::ui::format_size;

/// A row in the flattened dedup table.
pub(in crate::views) enum DedupRow {
    /// Group header row (collapsed or expanded).
    Group {
        group_ix: usize,
        name: String,
        copies: usize,
        size: u64,
        savings: u64,
        expanded: bool,
    },
    /// Individual file within an expanded group.
    File {
        group_ix: usize,
        path: PathBuf,
        is_keeper: bool,
    },
}

pub(in crate::views) struct DedupDelegate {
    /// The flat row list (rebuilt on expand/collapse/sort).
    pub rows: Vec<DedupRow>,
    /// Source groups data.
    pub groups: Vec<DedupGroupData>,
    /// Paths selected for deletion.
    pub selected: HashSet<PathBuf>,
    /// Currently focused row index.
    focused: Option<usize>,
    columns: Vec<Column>,
}

/// Source data for a duplicate group.
pub(in crate::views) struct DedupGroupData {
    pub size: u64,
    pub files: Vec<PathBuf>,
    pub expanded: bool,
}

impl DedupDelegate {
    pub fn new() -> Self {
        let columns = vec![
            Column::new("name", "Name").width(px(500.0)).sortable(),
            Column::new("copies", "Copies")
                .width(px(80.0))
                .text_right()
                .sortable(),
            Column::new("size", "Size")
                .width(px(100.0))
                .text_right()
                .sortable(),
            Column::new("savings", "Savings")
                .width(px(100.0))
                .text_right()
                .sortable(),
        ];
        Self {
            rows: Vec::new(),
            groups: Vec::new(),
            selected: HashSet::new(),
            focused: None,
            columns,
        }
    }

    /// Add a verified duplicate group. Returns true if added.
    pub fn push_group(&mut self, size: u64, files: Vec<PathBuf>) -> bool {
        if files.len() < 2 {
            return false;
        }
        self.groups.push(DedupGroupData {
            size,
            files,
            expanded: false,
        });
        true
    }

    /// Rebuild the flat row list from groups.
    pub fn rebuild_rows(&mut self) {
        self.rows.clear();
        for (gi, group) in self.groups.iter().enumerate() {
            let name = group
                .files
                .first()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| "Unknown".into());
            let copies = group.files.len();
            let savings = group.size * (copies as u64 - 1);

            self.rows.push(DedupRow::Group {
                group_ix: gi,
                name,
                copies,
                size: group.size,
                savings,
                expanded: group.expanded,
            });

            if group.expanded {
                for (fi, path) in group.files.iter().enumerate() {
                    self.rows.push(DedupRow::File {
                        group_ix: gi,
                        path: path.clone(),
                        is_keeper: fi == 0,
                    });
                }
            }
        }
    }

    pub fn toggle_expand(&mut self, group_ix: usize) {
        if let Some(g) = self.groups.get_mut(group_ix) {
            g.expanded = !g.expanded;
        }
        self.rebuild_rows();
    }

    pub fn toggle_file(&mut self, path: PathBuf) {
        if self.selected.contains(&path) {
            self.selected.remove(&path);
        } else {
            self.selected.insert(path);
        }
    }

    pub fn select_all_duplicates(&mut self) {
        self.selected.clear();
        for group in &self.groups {
            // Skip index 0 (keeper)
            for file in group.files.iter().skip(1) {
                self.selected.insert(file.clone());
            }
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    pub fn selected_bytes(&self) -> u64 {
        let mut total = 0u64;
        for group in &self.groups {
            for file in &group.files {
                if self.selected.contains(file) {
                    total += group.size;
                }
            }
        }
        total
    }

    pub fn total_wasted(&self) -> u64 {
        self.groups
            .iter()
            .map(|g| g.size * (g.files.len() as u64 - 1))
            .sum()
    }

    /// Remove deleted paths from groups, prune empty groups.
    pub fn remove_deleted(&mut self, deleted: &HashSet<PathBuf>) {
        for group in &mut self.groups {
            group.files.retain(|f| !deleted.contains(f));
        }
        self.groups.retain(|g| g.files.len() >= 2);
        self.selected.retain(|p| !deleted.contains(p));
        self.rebuild_rows();
    }

    pub fn sort_by_savings_desc(&mut self) {
        self.groups.sort_by(|a, b| {
            let a_s = a.size * (a.files.len() as u64 - 1);
            let b_s = b.size * (b.files.len() as u64 - 1);
            b_s.cmp(&a_s)
        });
        self.rebuild_rows();
    }

    pub fn reveal_item(&self, row_ix: usize) {
        if let Some(DedupRow::File { path, .. }) = self.rows.get(row_ix) {
            #[cfg(target_os = "macos")]
            crate::platform::open::reveal_in_finder(path);
        }
    }
}

impl TableDelegate for DedupDelegate {
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
            // Name
            0 => self.groups.sort_by(|a, b| {
                let a_name = a
                    .files
                    .first()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
                let b_name = b
                    .files
                    .first()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
                let cmp = a_name.cmp(&b_name);
                if desc { cmp.reverse() } else { cmp }
            }),
            // Copies
            1 => self.groups.sort_by(|a, b| {
                let cmp = a.files.len().cmp(&b.files.len());
                if desc { cmp.reverse() } else { cmp }
            }),
            // Size
            2 => self.groups.sort_by(|a, b| {
                let cmp = a.size.cmp(&b.size);
                if desc { cmp.reverse() } else { cmp }
            }),
            // Savings
            3 => self.groups.sort_by(|a, b| {
                let a_s = a.size * (a.files.len() as u64 - 1);
                let b_s = b.size * (b.files.len() as u64 - 1);
                let cmp = a_s.cmp(&b_s);
                if desc { cmp.reverse() } else { cmp }
            }),
            _ => {}
        }
        self.rebuild_rows();
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

        match row {
            DedupRow::Group {
                name,
                copies,
                size,
                savings,
                expanded,
                ..
            } => match col_ix {
                // Name column with chevron
                0 => gpui_component::h_flex()
                    .h_full()
                    .items_center()
                    .gap_1p5()
                    .child(
                        gpui_component::Icon::new(if *expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .with_size(crate::theme::ICON_XS)
                        .text_color(muted),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(FONT_SIZE_BODY)
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(SharedString::from(name.clone())),
                    )
                    .into_any_element(),
                // Copies
                1 => div()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_end()
                    .text_size(FONT_SIZE_BODY)
                    .child(SharedString::from(format!("{copies}")))
                    .into_any_element(),
                // Size
                2 => div()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_end()
                    .text_size(FONT_SIZE_BODY)
                    .child(SharedString::from(format_size(*size)))
                    .into_any_element(),
                // Savings
                3 => div()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_end()
                    .text_size(FONT_SIZE_BODY)
                    .text_color(muted)
                    .child(SharedString::from(format_size(*savings)))
                    .into_any_element(),
                _ => div().into_any_element(),
            },
            DedupRow::File {
                path, is_keeper, ..
            } => match col_ix {
                // File path with checkbox or "keep" label
                0 => {
                    let display = path.to_string_lossy().to_string();
                    let checked = self.selected.contains(path);
                    let table = cx.entity().clone();
                    let ix = row_ix;

                    let label_el: AnyElement = if *is_keeper {
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(theme::success_color(cx))
                            .child("keep")
                            .into_any_element()
                    } else {
                        Checkbox::new(SharedString::from(format!("chk-{row_ix}")))
                            .checked(checked)
                            .into_any_element()
                    };

                    gpui_component::h_flex()
                        .h_full()
                        .items_center()
                        .gap_2()
                        .pl(px(20.0))
                        .child(label_el)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(FONT_SIZE_CAPTION)
                                .text_color(muted)
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(SharedString::from(display)),
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
                _ => div().into_any_element(),
            },
        }
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let is_focused = self.focused == Some(row_ix);
        let table = cx.entity().clone();

        let row_data = self.rows.get(row_ix);
        let is_group = matches!(row_data, Some(DedupRow::Group { .. }));
        let is_file = matches!(
            row_data,
            Some(DedupRow::File {
                is_keeper: false,
                ..
            })
        );
        let group_ix = row_data.map(|r| match r {
            DedupRow::Group { group_ix, .. } | DedupRow::File { group_ix, .. } => *group_ix,
        });

        let mut el = div()
            .id(("dedup-row", row_ix))
            .group("table-row")
            .rounded(RADIUS)
            .cursor_pointer();

        if is_group {
            el = el.on_click(move |_, _, cx| {
                if let Some(gi) = group_ix {
                    table.update(cx, |state, cx| {
                        let d = state.delegate_mut();
                        d.focused = Some(row_ix);
                        d.toggle_expand(gi);
                        cx.notify();
                    });
                }
            });
        } else if is_file {
            let table2 = cx.entity().clone();
            if let Some(DedupRow::File { path, .. }) = self.rows.get(row_ix) {
                let p = path.clone();
                el = el.on_click(move |_, _, cx| {
                    table2.update(cx, |state, cx| {
                        let d = state.delegate_mut();
                        d.focused = Some(row_ix);
                        d.toggle_file(p.clone());
                        cx.notify();
                    });
                });
            }
        }

        if is_focused {
            el = el.bg(theme::surface_hover(cx));
        }

        el
    }
}
