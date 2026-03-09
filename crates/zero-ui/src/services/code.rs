//! Code indexing service for the UI

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gpui::*;

use zero::code::CodeIndex;

#[allow(dead_code)]
pub enum CodeEvent {
    ProjectIndexed { path: PathBuf, symbols: usize },
    IndexingFinished,
    Error(String),
}

impl EventEmitter<CodeEvent> for CodeService {}

#[allow(dead_code)] // API ready for UI views to consume
pub struct CodeService {
    index: Arc<Mutex<Option<CodeIndex>>>,
}

#[allow(dead_code)] // API ready for UI views to consume
impl CodeService {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let index = match CodeIndex::new() {
            Ok(ci) => Some(ci),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to initialize CodeIndex");
                None
            }
        };
        Self {
            index: Arc::new(Mutex::new(index)),
        }
    }

    /// Get a fresh CodeIndex (for passing to MCP server)
    pub fn code_index(&self) -> Option<CodeIndex> {
        CodeIndex::new().ok()
    }

    pub fn index_project(&self, path: PathBuf, cx: &mut Context<Self>) {
        let index = Arc::clone(&self.index);
        cx.spawn(async move |this, cx| {
            let result = {
                let mut guard = index.lock().expect("code index lock");
                if let Some(ci) = guard.as_mut() {
                    ci.index_project(&path)
                        .map(|p| (p.path.clone(), p.symbol_count))
                        .map_err(|e| e.to_string())
                } else {
                    Err("CodeIndex not initialized".to_string())
                }
            };

            this.update(cx, |_this, cx| match result {
                Ok((project_path, symbols)) => {
                    cx.emit(CodeEvent::ProjectIndexed {
                        path: project_path,
                        symbols,
                    });
                }
                Err(e) => {
                    cx.emit(CodeEvent::Error(e));
                }
            })
        })
        .detach();
    }
}
