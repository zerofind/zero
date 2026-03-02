use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    h_flex,
    table::{Column, ColumnSort, TableDelegate, TableState},
    ActiveTheme, Icon, IconName, InteractiveElementExt as _, Sizable as _,
};

use crate::models::{SortDirection, SortField};
use crate::theme::{FONT_SIZE_BODY, FONT_SIZE_CAPTION};
use crate::ui::FileIcon;

use super::state::{self, BrowserEntry};

/// The 4-column file browser table delegate.
pub struct FileBrowserDelegate {
    pub entries: Vec<BrowserEntry>,
    pub sort_field: SortField,
    pub sort_direction: SortDirection,
    pub selected: Vec<usize>,
    pub split_active: bool,
    columns: Vec<Column>,
}

impl FileBrowserDelegate {
    pub fn new(entries: Vec<BrowserEntry>) -> Self {
        let mut delegate = Self {
            entries,
            sort_field: SortField::Name,
            sort_direction: SortDirection::Ascending,
            selected: Vec::new(),
            split_active: false,
            columns: Vec::new(),
        };
        delegate.rebuild_columns();
        delegate
    }

    fn column_for_index(&self, col_ix: usize) -> (SortField, &str, Pixels) {
        match col_ix {
            0 => (SortField::Name, "Name", px(400.0)),
            1 => (SortField::DateModified, "Date Modified", px(160.0)),
            2 => (SortField::Size, "Size", px(100.0)),
            3 => (SortField::Kind, "Kind", px(120.0)),
            _ => (SortField::Name, "", px(100.0)),
        }
    }

    fn rebuild_columns(&mut self) {
        self.columns = (0..4)
            .map(|col_ix| {
                let (field, name, width) = self.column_for_index(col_ix);
                let name_owned = name.to_string();
                let mut col =
                    Column::new(name_owned.clone(), name_owned).width(width).sortable();
                if col_ix == 2 {
                    col = col.text_right();
                }
                if field == self.sort_field {
                    col = match self.sort_direction {
                        SortDirection::Ascending => col.ascending(),
                        SortDirection::Descending => col.descending(),
                    };
                }
                col
            })
            .collect();
    }
}

impl TableDelegate for FileBrowserDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        4
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
        let (field, _, _) = self.column_for_index(col_ix);

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

        let _is_selected = self.selected.contains(&row_ix);
        let muted = cx.theme().muted_foreground;

        match col_ix {
            // Name column: indent + icon + name
            0 => {
                let indent = entry.depth as f32 * 20.0;
                let is_dir = entry.is_dir;
                let expanded = entry.expanded;
                let table = cx.entity().clone();

                h_flex()
                    .h_full()
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
                                .id(("chevron", row_ix))
                                .cursor_pointer()
                                .child(
                                    Icon::new(icon)
                                        .with_size(px(10.0))
                                        .text_color(muted),
                                )
                                .on_click(move |_, _, cx| {
                                    table.update(cx, |state, cx| {
                                        let delegate = state.delegate_mut();
                                        let was_expanded = delegate.entries[row_ix].expanded;
                                        let count = state::toggle_expand(
                                            &mut delegate.entries,
                                            row_ix,
                                        );

                                        if was_expanded {
                                            // Collapsed: remove selections in removed range, shift down
                                            delegate.selected.retain(|&idx| {
                                                idx <= row_ix || idx > row_ix + count
                                            });
                                            for idx in delegate.selected.iter_mut() {
                                                if *idx > row_ix + count {
                                                    *idx -= count;
                                                }
                                            }
                                        } else {
                                            // Expanded: shift selections after row_ix up by count
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
                    .when(!is_dir, |el| el.child(div().w(px(10.0))))
                    .child(FileIcon::new(
                        entry.extension.as_deref(),
                        entry.is_dir,
                    ))
                    .child(
                        div()
                            .text_size(FONT_SIZE_BODY)
                            .text_color(cx.theme().foreground)
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .min_w_0()
                            .child(SharedString::from(entry.name.clone())),
                    )
                    .into_any_element()
            }
            // Date Modified
            1 => div()
                .h_full()
                .flex()
                .items_center()
                .text_size(FONT_SIZE_CAPTION)
                .text_color(muted)
                .child(SharedString::from(state::format_date(entry.mtime)))
                .into_any_element(),
            // Size
            2 => div()
                .h_full()
                .flex()
                .items_center()
                .justify_end()
                .text_size(FONT_SIZE_CAPTION)
                .text_color(muted)
                .child(SharedString::from(state::format_size(entry.size)))
                .into_any_element(),
            // Kind
            3 => {
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
