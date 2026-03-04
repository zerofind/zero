use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
};

use crate::theme::FONT_SIZE_CAPTION;

use super::render::FileBrowserView;
use super::state::BrowserEntry;

/// Display mode for filtered results.
pub(super) enum DisplayMode {
    /// Filtered by search query.
    Filtered {
        #[allow(dead_code)]
        query: String,
        original_entries: Vec<BrowserEntry>,
    },
    /// Showing search results from command palette.
    SearchResults { query: String },
}

impl FileBrowserView {
    /// Show search results from command palette in the file browser.
    pub fn show_search_results(
        &mut self,
        query: String,
        entries: Vec<BrowserEntry>,
        cx: &mut Context<Self>,
    ) {
        // Save originals if not already in a display mode
        if self.display_mode.is_none() {
            // We don't need to save originals for SearchResults mode
            // since we navigate away from it rather than restoring
        }

        self.table_state.update(cx, |state, cx| {
            let delegate = state.delegate_mut();
            delegate.entries = entries;
            delegate.selected.clear();
            cx.notify();
        });

        self.display_mode = Some(DisplayMode::SearchResults { query });
        self.search_active = false;
        self.search_input = None;
        cx.notify();
    }

    /// Toggle the in-browser search bar.
    pub fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_active {
            self.dismiss_search(window, cx);
        } else {
            self.search_active = true;
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter files..."));
            // Subscribe to input changes (annotate event type to disambiguate)
            cx.subscribe(
                &input,
                |this, input, _ev: &gpui_component::input::InputEvent, cx| {
                    let text = input.read(cx).value().to_string();
                    this.apply_search_filter(&text, cx);
                },
            )
            .detach();
            input.focus_handle(cx).focus(window);
            self.search_input = Some(input);
            cx.notify();
        }
    }

    /// Dismiss the search bar and restore original entries.
    pub fn dismiss_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_active = false;
        self.search_input = None;

        // Restore original entries if we were filtering
        match self.display_mode.take() {
            Some(DisplayMode::Filtered {
                original_entries, ..
            }) => {
                self.table_state.update(cx, |state, cx| {
                    let delegate = state.delegate_mut();
                    delegate.entries = original_entries;
                    delegate.selected.clear();
                    cx.notify();
                });
            }
            Some(DisplayMode::SearchResults { .. }) => {
                // Reload directory to restore normal view
                self.reload(cx);
            }
            None => {}
        }
        self.display_mode = None;
        // Restore focus to the table
        self.table_state.focus_handle(cx).focus(window);
        cx.notify();
    }

    /// Apply the current search query to filter entries.
    fn apply_search_filter(&mut self, raw_query: &str, cx: &mut Context<Self>) {
        let query = raw_query.trim().to_lowercase();

        if query.is_empty() {
            // Restore original entries
            if let Some(DisplayMode::Filtered {
                original_entries, ..
            }) = self.display_mode.take()
            {
                self.table_state.update(cx, |state, cx| {
                    let delegate = state.delegate_mut();
                    delegate.entries = original_entries;
                    delegate.selected.clear();
                    cx.notify();
                });
            }
            self.display_mode = None;
            cx.notify();
            return;
        }

        // Save original entries if transitioning from Normal
        let original = match &self.display_mode {
            Some(DisplayMode::Filtered {
                original_entries, ..
            }) => original_entries.clone(),
            _ => self.table_state.read(cx).delegate().entries.clone(),
        };

        // Filter: client-side matching against filename
        let filtered: Vec<BrowserEntry> = original
            .iter()
            .filter(|e| e.name.to_lowercase().contains(&query))
            .cloned()
            .collect();

        // If query is 3+ chars and search index is available, also search index
        let mut index_results = Vec::new();
        if query.len() >= 3 {
            let results = self.search.read(cx).search(&query, 50);
            for result in results {
                let result_path = std::path::Path::new(&result.node.path);
                // Only include results not already in filtered list
                if !filtered.iter().any(|e| e.path == result_path)
                    && let Some(entry) = BrowserEntry::from_fs(result_path, 0)
                {
                    index_results.push(entry);
                }
            }
        }

        let mut all_results = filtered;
        all_results.extend(index_results);

        self.table_state.update(cx, |state, cx| {
            let delegate = state.delegate_mut();
            delegate.entries = all_results;
            delegate.selected.clear();
            cx.notify();
        });

        self.display_mode = Some(DisplayMode::Filtered {
            query,
            original_entries: original,
        });
        cx.notify();
    }

    /// Render the search bar UI.
    pub(super) fn render_search_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.search_active {
            return None;
        }
        let input = self.search_input.as_ref()?;
        let muted = cx.theme().muted_foreground;

        let result_count = self.table_state.read(cx).delegate().entries.len();
        let is_filtering = self.display_mode.is_some();

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
                    if ev.keystroke.key == "escape" {
                        this.dismiss_search(window, cx);
                    }
                }))
                .child(div().flex_1().child(Input::new(input)))
                .when(is_filtering, |el| {
                    el.child(
                        div()
                            .text_size(FONT_SIZE_CAPTION)
                            .text_color(muted)
                            .child(SharedString::from(format!("{} results", result_count))),
                    )
                })
                .child(
                    Button::new("dismiss-search")
                        .ghost()
                        .small()
                        .compact()
                        .icon(IconName::Close)
                        .text_color(muted)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dismiss_search(window, cx);
                        })),
                )
                .into_any_element(),
        )
    }
}
