use std::path::PathBuf;

/// Identifies the active pane in split view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaneId {
    Left,
    Right,
}

/// State for the right (secondary) split pane.
pub struct SplitPane {
    pub current_path: PathBuf,
    pub history: Vec<PathBuf>,
    pub history_idx: usize,
}

impl SplitPane {
    pub fn new(path: PathBuf) -> Self {
        Self {
            current_path: path.clone(),
            history: vec![path],
            history_idx: 0,
        }
    }

    pub fn navigate_to(&mut self, path: PathBuf) {
        if self.history_idx + 1 < self.history.len() {
            self.history.truncate(self.history_idx + 1);
        }
        self.history.push(path.clone());
        self.history_idx = self.history.len() - 1;
        self.current_path = path;
    }

    #[allow(dead_code)]
    pub fn go_back(&mut self) {
        if self.history_idx == 0 {
            return;
        }
        self.history_idx -= 1;
        // history_idx validated by bounds check above
        #[allow(clippy::indexing_slicing)]
        self.current_path
            .clone_from(&self.history[self.history_idx]);
    }

    #[allow(dead_code)]
    pub fn go_forward(&mut self) {
        if self.history_idx + 1 >= self.history.len() {
            return;
        }
        self.history_idx += 1;
        // history_idx validated by bounds check above
        #[allow(clippy::indexing_slicing)]
        self.current_path
            .clone_from(&self.history[self.history_idx]);
    }
}
