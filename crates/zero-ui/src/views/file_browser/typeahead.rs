use std::time::Instant;

use gpui::*;

use super::render::FileBrowserView;

const TYPEAHEAD_TIMEOUT_MS: u128 = 800;

impl FileBrowserView {
    /// Handle a key-down event for type-ahead navigation.
    /// When the user types printable characters, jump to the first entry
    /// whose name starts with the accumulated buffer (like Finder).
    pub(super) fn handle_typeahead(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        // Skip when search, inline edit, or dialog is active
        if self.search_active || self.is_editing(cx) {
            return;
        }

        // Only handle printable chars without cmd/ctrl
        if ev.keystroke.modifiers.platform || ev.keystroke.modifiers.control {
            return;
        }

        let Some(ref key_char) = ev.keystroke.key_char else {
            return;
        };

        // Skip space (reserved for QuickLook) and other control-ish keys
        if ev.keystroke.key == "space" || key_char.is_empty() {
            return;
        }

        let now = Instant::now();
        let timed_out = self
            .typeahead_last_key
            .map(|last| now.duration_since(last).as_millis() > TYPEAHEAD_TIMEOUT_MS)
            .unwrap_or(true);

        if timed_out {
            self.typeahead_buffer.clear();
        }
        self.typeahead_last_key = Some(now);

        // Single-char cycling: same character repeated (d, d, d → cycles matches)
        let is_repeat = self.typeahead_buffer.len() == 1
            && key_char.len() == 1
            && self.typeahead_buffer.eq_ignore_ascii_case(key_char);

        if is_repeat {
            self.typeahead_cycle_next(cx);
        } else {
            self.typeahead_buffer.push_str(key_char);
            self.typeahead_jump(cx);
        }
    }

    /// Jump to the first entry whose name starts with the typeahead buffer.
    fn typeahead_jump(&mut self, cx: &mut Context<Self>) {
        let query = self.typeahead_buffer.to_lowercase();

        let found = {
            let delegate = self.table_state.read(cx).delegate();
            delegate
                .entries
                .iter()
                .position(|e| e.name.to_lowercase().starts_with(&query))
        };

        if let Some(idx) = found {
            self.table_state.update(cx, |state, cx| {
                state.set_selected_row(idx, cx);
            });
        }
    }

    /// Cycle to the next entry matching the single-character buffer,
    /// wrapping around from the current selection.
    fn typeahead_cycle_next(&mut self, cx: &mut Context<Self>) {
        let query = self.typeahead_buffer.to_lowercase();

        let found = {
            let delegate = self.table_state.read(cx).delegate();
            let current = delegate.selected.first().copied().unwrap_or(0);
            let len = delegate.entries.len();
            if len == 0 {
                return;
            }

            let mut result = None;
            for offset in 1..=len {
                let idx = (current + offset) % len;
                if delegate.entries[idx]
                    .name
                    .to_lowercase()
                    .starts_with(&query)
                {
                    result = Some(idx);
                    break;
                }
            }
            result
        };

        if let Some(idx) = found {
            self.table_state.update(cx, |state, cx| {
                state.set_selected_row(idx, cx);
            });
        }
    }

    /// Clear the typeahead buffer (called on navigation, reload, etc.).
    pub(super) fn clear_typeahead(&mut self) {
        self.typeahead_buffer.clear();
        self.typeahead_last_key = None;
    }
}
