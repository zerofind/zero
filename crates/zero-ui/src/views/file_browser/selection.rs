use super::delegate::FileBrowserDelegate;

impl FileBrowserDelegate {
    /// Select a single row, clearing previous selection.
    pub fn select(&mut self, row_ix: usize) {
        self.selected.clear();
        if row_ix < self.entries.len() {
            self.selected.push(row_ix);
        }
    }

    /// Toggle selection of a row (Cmd+click).
    pub fn toggle_select(&mut self, row_ix: usize) {
        if let Some(pos) = self.selected.iter().position(|&r| r == row_ix) {
            self.selected.remove(pos);
        } else if row_ix < self.entries.len() {
            self.selected.push(row_ix);
        }
    }

    /// Range-select from last selected to row_ix (Shift+click).
    pub fn range_select(&mut self, row_ix: usize) {
        let anchor = self.selected.last().copied().unwrap_or(0);
        let (start, end) = if row_ix > anchor {
            (anchor, row_ix)
        } else {
            (row_ix, anchor)
        };

        self.selected.clear();
        for i in start..=end.min(self.entries.len().saturating_sub(1)) {
            self.selected.push(i);
        }
    }

    /// Select all entries.
    pub fn select_all(&mut self) {
        self.selected = (0..self.entries.len()).collect();
    }

    /// Clear all selections.
    #[allow(dead_code)]
    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    /// Move selection up by one row.
    pub fn select_prev(&mut self) {
        let current = self.selected.first().copied().unwrap_or(0);
        if current > 0 {
            self.select(current - 1);
        }
    }

    /// Move selection down by one row.
    pub fn select_next(&mut self) {
        let current = self.selected.last().copied().unwrap_or(0);
        if current + 1 < self.entries.len() {
            self.select(current + 1);
        }
    }
}
