//! Etch-backed persistence for `TodoFile`
//!
//! WAL ops: collection 0, key = task ID (u64), value = postcard-encoded Task.
//! Snapshot = full `TodoState` serialized with postcard.

use std::path::Path;

use etchdb::{EtchKey, Op, Replayable, Store, Transactable, WalBackend};
use serde::{Deserialize, Serialize};

use super::file::TodoFile;
use super::manager::TodoError;
use super::task::Task;

const TASKS: u8 = 0;

/// The etch state wrapping a `TodoFile`.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct TodoState {
    pub file: TodoFile,
}

impl Replayable for TodoState {
    fn apply(&mut self, ops: &[Op]) -> etchdb::Result<()> {
        for op in ops {
            match op {
                Op::Put { key, value, .. } => {
                    let id = u64::from_bytes(key)?;
                    let task: Task =
                        postcard::from_bytes(value).map_err(|e| etchdb::Error::WalCorrupted {
                            offset: 0,
                            reason: format!("bad task: {e}"),
                        })?;
                    self.file.upsert_task(id, task);
                }
                Op::Delete { key, .. } => {
                    let id = u64::from_bytes(key)?;
                    self.file.remove_task(id);
                }
            }
        }
        Ok(())
    }
}

/// Transaction overlay for todo writes.
pub struct TodoOverlay {
    upserts: Vec<(u64, Task)>,
    deletes: Vec<u64>,
}

/// Transaction handle exposing typed put/delete on tasks.
pub struct TodoTx<'a> {
    pub state: &'a TodoState,
    ops: Vec<Op>,
    overlay: TodoOverlay,
}

impl TodoTx<'_> {
    /// Read a task (checks overlay first, then committed state).
    pub fn get_task(&self, id: u64) -> Option<&Task> {
        // Check deletes
        if self.overlay.deletes.contains(&id) {
            return None;
        }
        // Check upserts (last write wins)
        for (uid, task) in self.overlay.upserts.iter().rev() {
            if *uid == id {
                return Some(task);
            }
        }
        // Fall through to committed
        self.state.file.get_task(id)
    }

    /// Insert or update a task.
    pub fn put_task(&mut self, task: &Task) {
        let value = postcard::to_allocvec(task).expect("Task serialization");
        self.ops.push(Op::Put {
            collection: TASKS,
            key: task.id.to_bytes(),
            value,
        });
        self.overlay.upserts.push((task.id, task.clone()));
    }

    /// Delete a task by ID.
    pub fn delete_task(&mut self, id: u64) {
        self.ops.push(Op::Delete {
            collection: TASKS,
            key: id.to_bytes(),
        });
        self.overlay.deletes.push(id);
    }
}

impl Transactable for TodoState {
    type Tx<'a> = TodoTx<'a>;
    type Overlay = TodoOverlay;

    fn begin_tx(&self) -> TodoTx<'_> {
        TodoTx {
            state: self,
            ops: Vec::new(),
            overlay: TodoOverlay {
                upserts: Vec::new(),
                deletes: Vec::new(),
            },
        }
    }

    fn finish_tx(tx: TodoTx<'_>) -> (Vec<Op>, TodoOverlay) {
        (tx.ops, tx.overlay)
    }

    fn apply_overlay(&mut self, overlay: TodoOverlay) {
        for id in &overlay.deletes {
            self.file.remove_task(*id);
        }
        for (id, task) in overlay.upserts {
            self.file.upsert_task(id, task);
        }
    }
}

/// Open an etch store for a todo directory.
///
/// The `dir` is the etch directory (e.g., `project.todo/`).
/// Contains `snapshot.postcard` + `wal.bin`.
pub fn open_store(dir: &Path) -> Result<Store<TodoState, WalBackend<TodoState>>, TodoError> {
    Store::<TodoState, WalBackend<TodoState>>::open_wal(dir.to_path_buf())
        .map_err(|e| TodoError::Io(std::io::Error::other(e.to_string())))
}
