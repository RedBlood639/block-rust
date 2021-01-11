pub use std::sync::Arc;
pub use std::sync::{RwLock, RwLockWriteGuard};
pub use std::collections::BTreeMap;

#[derive(Eq, PartialEq, Ord, PartialOrd)]
pub struct Key(pub String);

#[derive(Copy, Clone)]
pub enum Value {
    Written(Address),
    Deleted(Address),
}

#[derive(Copy, Clone)]
#[derive(Eq, PartialEq, Ord, PartialOrd)]
pub struct Generation(pub usize);

#[derive(Copy, Clone)]
pub struct Address(pub usize);

pub struct Index {
    keymap: Arc<RwLock<BTreeMap<Key, Arc<Node>>>>,
}

pub struct Node {
    prev_next: RwLock<(Option<Arc<Node>>, Option<Arc<Node>>)>,
    history: RwLock<Vec<(Generation, Value)>>,
}

pub struct Cursor {
    gen: Generation,
    current: Option<Node>,
    keymap: Arc<RwLock<BTreeMap<Key, Arc<Node>>>>,
}

pub struct CommitLock<'index> {
    gen: Generation,
    lock_guard: RwLockWriteGuard<'index, BTreeMap<Key, Arc<Node>>>,
}

impl Index {
    pub fn new() -> Index {
        Index {
            keymap: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn read(&self, gen: Generation, key: &Key) -> Option<Value> {
        let map = self.keymap.read().expect("lock");
        if let Some(node) = map.get(key) {
            let history = node.history.read().expect("lock");
            for (h_gen, value) in history.iter().rev() {
                if *h_gen < gen {
                    return Some(*value);
                }
            }
            None
        } else {
            None
        }
    }

    pub fn cursor(&self, gen: Generation) -> Cursor {
        Cursor {
            gen,
            current: None,
            keymap: self.keymap.clone(),
        }
    }
}
