use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme,
    table::{Column, ColumnSort, TableDelegate, TableState},
};

use crate::models::SortDirection;
use crate::theme::{FONT_SIZE_BODY, FONT_SIZE_CAPTION};

use super::loader::DataFrame;

/// Table delegate for displaying parsed data frames.
pub struct DataFrameDelegate {
    pub frame: DataFrame,
    pub sort_col: Option<usize>,
    pub sort_dir: SortDirection,
    columns: Vec<Column>,
}

impl DataFrameDelegate {
    pub fn new(frame: DataFrame) -> Self {
        let columns = frame
            .headers
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let width = Self::guess_column_width(name, &frame.rows, i);
                Column::new(name.clone(), name.clone())
                    .width(width)
                    .sortable()
            })
            .collect();

        Self {
            frame,
            sort_col: None,
            sort_dir: SortDirection::Ascending,
            columns,
        }
    }

    /// Heuristic column width based on header and first few values.
    fn guess_column_width(header: &str, rows: &[Vec<String>], col: usize) -> Pixels {
        let header_len = header.len();
        let sample_max = rows
            .iter()
            .take(50)
            .filter_map(|r| r.get(col))
            .map(|v| v.len())
            .max()
            .unwrap_or(0);

        let max_chars = header_len.max(sample_max).min(60);
        // Rough: ~8px per char + 24px padding
        px((max_chars as f32 * 8.0 + 24.0).clamp(80.0, 400.0))
    }

    fn rebuild_columns(&mut self) {
        self.columns = self
            .frame
            .headers
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let width = Self::guess_column_width(name, &self.frame.rows, i);
                let mut col = Column::new(name.clone(), name.clone())
                    .width(width)
                    .sortable();

                if self.sort_col == Some(i) {
                    col = match self.sort_dir {
                        SortDirection::Ascending => col.ascending(),
                        SortDirection::Descending => col.descending(),
                    };
                }
                col
            })
            .collect();
    }
}

impl TableDelegate for DataFrameDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.frame.headers.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.frame.rows.len()
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
        if self.sort_col == Some(col_ix) {
            self.sort_dir = self.sort_dir.toggle();
        } else {
            self.sort_col = Some(col_ix);
            self.sort_dir = SortDirection::Ascending;
        }

        let col = col_ix;
        let ascending = self.sort_dir == SortDirection::Ascending;

        self.frame.rows.sort_by(|a, b| {
            let va = a.get(col).map(|s| s.as_str()).unwrap_or("");
            let vb = b.get(col).map(|s| s.as_str()).unwrap_or("");

            // Try numeric comparison first
            let cmp = match (va.parse::<f64>(), vb.parse::<f64>()) {
                (Ok(na), Ok(nb)) => na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal),
                _ => va.cmp(vb),
            };

            if ascending { cmp } else { cmp.reverse() }
        });

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
        let muted = cx.theme().muted_foreground;

        let value = self
            .frame
            .rows
            .get(row_ix)
            .and_then(|r| r.get(col_ix))
            .map(|s| s.as_str())
            .unwrap_or("");

        if value.is_empty() {
            return div()
                .h_full()
                .flex()
                .items_center()
                .text_size(FONT_SIZE_CAPTION)
                .text_color(muted)
                .opacity(0.5)
                .child(SharedString::from("—"))
                .into_any_element();
        }

        // Right-align numbers
        let is_numeric = value.parse::<f64>().is_ok();

        div()
            .h_full()
            .flex()
            .items_center()
            .when(is_numeric, |el| el.justify_end())
            .text_size(FONT_SIZE_BODY)
            .text_color(cx.theme().foreground)
            .text_ellipsis()
            .whitespace_nowrap()
            .min_w_0()
            .child(SharedString::from(value.to_string()))
            .into_any_element()
    }
}
