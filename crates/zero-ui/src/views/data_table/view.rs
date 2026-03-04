use std::path::PathBuf;

use gpui::*;
use gpui_component::table::{Table, TableState};
use gpui_component::{ActiveTheme, v_flex};

use crate::theme::{self, FONT_SIZE_BODY, FONT_SIZE_CAPTION};

use super::delegate::DataFrameDelegate;
use super::loader::DataFrame;

// -- Events ------------------------------------------------------------------

pub enum DataTableEvent {
    Close,
}

impl EventEmitter<DataTableEvent> for DataTableView {}

// -- View --------------------------------------------------------------------

pub struct DataTableView {
    path: PathBuf,
    /// Parsed data waiting for table state creation (needs window in render).
    pending_frame: Option<DataFrame>,
    table_state: Option<Entity<TableState<DataFrameDelegate>>>,
    loading: bool,
    error: Option<String>,
    info: Option<String>,
    pub focus_handle: FocusHandle,
}

impl DataTableView {
    pub fn new(path: PathBuf, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            path: path.clone(),
            pending_frame: None,
            table_state: None,
            loading: true,
            error: None,
            info: None,
            focus_handle: cx.focus_handle(),
        };

        view.load_file(path, cx);
        view
    }

    fn load_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.loading = true;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let load_path = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { super::loader::load_delimited(&load_path) })
                .await;

            this.update(cx, |view, cx| {
                view.loading = false;

                match result {
                    Ok(frame) => {
                        let col_count = frame.headers.len();
                        let row_count = frame.rows.len();
                        let total = frame.total_rows;
                        let truncated = frame.truncated;

                        let info = if truncated {
                            format!(
                                "{} rows \u{00d7} {} columns (showing first {})",
                                format_number(total),
                                col_count,
                                format_number(row_count),
                            )
                        } else {
                            format!(
                                "{} rows \u{00d7} {} columns",
                                format_number(row_count),
                                col_count,
                            )
                        };
                        view.info = Some(info);
                        view.pending_frame = Some(frame);
                    }
                    Err(e) => {
                        view.error = Some(e);
                    }
                }

                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Create the TableState from the pending frame (requires window access).
    fn materialize_table(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(frame) = self.pending_frame.take() {
            let delegate = DataFrameDelegate::new(frame);
            let state = cx.new(|cx| TableState::new(delegate, window, cx).col_selectable(false));
            self.table_state = Some(state);
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        cx.emit(DataTableEvent::Close);
    }

    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.to_string_lossy().to_string())
    }

    pub fn info(&self) -> Option<&str> {
        self.info.as_deref()
    }

    pub fn path_str(&self) -> String {
        self.path.to_string_lossy().to_string()
    }
}

impl Render for DataTableView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Materialize the table state if data just arrived
        if self.pending_frame.is_some() {
            self.materialize_table(window, cx);
        }

        let muted = cx.theme().muted_foreground;

        let content: AnyElement = if let Some(err) = &self.error {
            div()
                .p_4()
                .text_size(FONT_SIZE_BODY)
                .text_color(muted)
                .child(SharedString::from(format!("Error: {err}")))
                .into_any_element()
        } else if self.loading {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(FONT_SIZE_CAPTION)
                .text_color(muted)
                .child("Loading\u{2026}")
                .into_any_element()
        } else if let Some(state) = &self.table_state {
            div()
                .flex_1()
                .size_full()
                .overflow_hidden()
                .child(Table::new(state).bordered(false).stripe(true))
                .into_any_element()
        } else {
            div().into_any_element()
        };

        v_flex()
            .track_focus(&self.focus_handle)
            .key_context("DataTableView")
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    this.close(cx);
                }
                if ev.keystroke.key == "w"
                    && (ev.keystroke.modifiers.platform || ev.keystroke.modifiers.control)
                {
                    this.close(cx);
                }
            }))
            .size_full()
            .bg(theme::content_bg(cx))
            .child(content)
    }
}

/// Format a number with thousands separators.
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
