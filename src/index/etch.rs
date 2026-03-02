//! Etch-backed persistence for SearchIndex
//!
//! WAL ops: collection 0, key = path (String), value = postcard-encoded FileNode.
//! Snapshot = full SearchIndex serialized with postcard (includes pre-built bitmaps).

use etchdb::{EtchKey, Op, Replayable, Store, Transactable, WalBackend};

use super::node::FileNode;
use super::search::{IndexError, SearchIndex};

const NODES: u8 = 0;

impl Replayable for SearchIndex {
    fn apply(&mut self, ops: &[Op]) -> etchdb::Result<()> {
        for op in ops {
            match op {
                Op::Put { value, .. } => {
                    let node: FileNode = postcard::from_bytes(value).map_err(|e| {
                        etchdb::Error::WalCorrupted {
                            offset: 0,
                            reason: format!("bad file node: {e}"),
                        }
                    })?;
                    self.insert(node);
                }
                Op::Delete { key, .. } => {
                    let path = String::from_bytes(key)?;
                    self.remove(&path);
                }
            }
        }
        Ok(())
    }
}

/// Transaction overlay for index writes.
pub struct IndexOverlay {
    inserts: Vec<FileNode>,
    deletes: Vec<String>,
}

/// Transaction handle for index operations.
pub struct IndexTx<'a> {
    pub state: &'a SearchIndex,
    ops: Vec<Op>,
    overlay: IndexOverlay,
}

impl<'a> IndexTx<'a> {
    /// Insert a file node.
    pub fn put_node(&mut self, node: &FileNode) {
        let value = postcard::to_allocvec(node).expect("FileNode serialization");
        self.ops.push(Op::Put {
            collection: NODES,
            key: node.path.to_bytes(),
            value,
        });
        self.overlay.inserts.push(node.clone());
    }

    /// Delete a node by path.
    pub fn delete_node(&mut self, path: &str) {
        self.ops.push(Op::Delete {
            collection: NODES,
            key: path.to_string().to_bytes(),
        });
        self.overlay.deletes.push(path.to_string());
    }
}

impl Transactable for SearchIndex {
    type Tx<'a> = IndexTx<'a>;
    type Overlay = IndexOverlay;

    fn begin_tx(&self) -> IndexTx<'_> {
        IndexTx {
            state: self,
            ops: Vec::new(),
            overlay: IndexOverlay {
                inserts: Vec::new(),
                deletes: Vec::new(),
            },
        }
    }

    fn finish_tx(tx: IndexTx<'_>) -> (Vec<Op>, IndexOverlay) {
        (tx.ops, tx.overlay)
    }

    fn apply_overlay(&mut self, overlay: IndexOverlay) {
        for path in &overlay.deletes {
            self.remove(path);
        }
        for node in overlay.inserts {
            self.insert(node);
        }
    }
}

/// Open an etch store for an index directory.
pub fn open_index_store(
    dir: &std::path::Path,
) -> Result<Store<SearchIndex, WalBackend<SearchIndex>>, IndexError> {
    Store::<SearchIndex, WalBackend<SearchIndex>>::open_wal(dir.to_path_buf())
        .map_err(|e| IndexError::Io(std::io::Error::other(e.to_string())))
}

/// Save a SearchIndex to an etch store directory.
///
/// Opens (or creates) the store, writes all nodes in a single transaction,
/// then flushes to ensure persistence.
pub fn save_index_via_etch(index: &SearchIndex, dir: &std::path::Path) -> Result<(), IndexError> {
    std::fs::create_dir_all(dir)?;

    let store = open_index_store(dir)?;
    let nodes: Vec<FileNode> = index.iter().cloned().collect();

    store
        .write(|tx| {
            for node in &nodes {
                tx.put_node(node);
            }
            Ok(())
        })
        .map_err(|e| IndexError::Io(std::io::Error::other(e.to_string())))?;

    store
        .flush()
        .map_err(|e| IndexError::Io(std::io::Error::other(e.to_string())))?;

    Ok(())
}
