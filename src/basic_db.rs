use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use futures::lock::{Mutex, MutexGuard};
use std::sync::Arc;
use std::collections::BTreeMap;
use std::path::PathBuf;
use crate::tree::{self, Tree};
use anyhow::{Result, Context, anyhow};
use crate::types::{Batch, BatchCommit, Commit, Key, Value};
use crate::commit_log::{CommitLog, CommitCommand};
use crate::command::Command;
use crate::log::Log;
use crate::loader;
use std::fmt;

pub struct Db {
    initialized: AtomicBool,
    next_batch: AtomicU64,
    next_batch_commit: Arc<AtomicU64>,
    next_commit: Arc<AtomicU64>,
    view_commit_limit: Arc<AtomicU64>,
    commit_lock: Arc<Mutex<()>>,
    trees: Arc<BTreeMap<String, Tree>>,
    commit_log: Arc<CommitLog>,
}

pub struct BatchWriter {
    batch: Batch,
    batch_writers: BTreeMap<String, tree::BatchWriter>,
    next_batch_commit: Arc<AtomicU64>,
    next_commit: Arc<AtomicU64>,
    view_commit_limit: Arc<AtomicU64>,
    commit_lock: Arc<Mutex<()>>,
    commit_log: Arc<CommitLog>,
}

#[derive(Clone)]
pub struct ViewReader {
    commit_limit: Commit,
    trees: Arc<BTreeMap<String, Tree>>,
}

pub struct Cursor {
    tree_cursor: tree::Cursor,
}

impl Db {
    pub fn new(tree_logs: BTreeMap<String, Log<Command>>, commit_log: Log<CommitCommand>) -> Db {
        let trees = tree_logs.into_iter().map(|(tree_name, log)| {
            (tree_name, Tree::new(log))
        }).collect();
        let trees = Arc::new(trees);

        let commit_log = Arc::new(CommitLog::new(commit_log));

        Db {
            initialized: AtomicBool::new(false),
            next_batch: AtomicU64::new(0),
            next_batch_commit: Arc::new(AtomicU64::new(0)),
            next_commit: Arc::new(AtomicU64::new(0)),
            view_commit_limit: Arc::new(AtomicU64::new(0)),
            commit_lock: Arc::new(Mutex::new(())),
            trees,
            commit_log,
        }
    }

    pub async fn init(&self) -> Result<()> {
        assert!(!self.initialized.load(Ordering::SeqCst));

        let init_state = loader::load(&self.commit_log, &self.trees).await?;
        log::trace!("init state {:?}", init_state);

        let view_commit_limit = init_state.next_commit.0;

        self.next_batch.store(init_state.next_batch.0, Ordering::SeqCst);
        self.next_batch_commit.store(init_state.next_batch_commit.0, Ordering::SeqCst);
        self.next_commit.store(init_state.next_commit.0, Ordering::SeqCst);
        self.view_commit_limit.store(view_commit_limit, Ordering::SeqCst);
        
        self.initialized.store(true, Ordering::SeqCst);

        Ok(())
    }

    pub fn batch(&self) -> BatchWriter {
        assert!(self.initialized.load(Ordering::SeqCst));

        let batch = Batch(self.next_batch.fetch_add(1, Ordering::SeqCst));
        assert_ne!(batch.0, u64::max_value());

        let batch_writers = self.trees.iter().map(|(name, tree)| {
            (name.clone(), tree.batch(batch))
        }).collect();

        BatchWriter {
            batch,
            batch_writers,
            next_batch_commit: self.next_batch_commit.clone(),
            next_commit: self.next_commit.clone(),
            view_commit_limit: self.view_commit_limit.clone(),
            commit_lock: self.commit_lock.clone(),
            commit_log: self.commit_log.clone(),
        }
    }

    pub fn view(&self) -> ViewReader {
        assert!(self.initialized.load(Ordering::SeqCst));

        let commit_limit = Commit(self.view_commit_limit.load(Ordering::SeqCst));

        ViewReader {
            commit_limit,
            trees: self.trees.clone(),
        }
    }

    pub async fn sync(&self) -> Result<()> {
        for (_, tree) in self.trees.iter() {
            tree.sync().await?;
        }

        Ok(())
    }
}

impl BatchWriter {
    pub fn number(&self) -> Batch {
        self.batch
    }

    pub async fn open(&self, tree: &str) -> Result<()> {
        let writer = self.tree_writer(tree);
        Ok(writer.open().await?)
    }

    pub async fn write(&self, tree: &str, key: Key, value: Value) -> Result<()> {
        let writer = self.tree_writer(tree);
        Ok(writer.write(key, value).await?)
    }

    pub async fn delete(&self, tree: &str, key: Key) -> Result<()> {
        let writer = self.tree_writer(tree);
        Ok(writer.delete(key).await?)
    }

    pub async fn delete_range(&self, tree: &str, start_key: Key, end_key: Key) -> Result<()> {
        let writer = self.tree_writer(tree);
        Ok(writer.delete_range(start_key, end_key).await?)
    }

    pub async fn push_save_point(&self, tree: &str) -> Result<()> {
        let writer = self.tree_writer(tree);
        Ok(writer.push_save_point().await?)
    }

    pub async fn pop_save_point(&self, tree: &str) -> Result<()> {
        let writer = self.tree_writer(tree);
        Ok(writer.pop_save_point().await?)
    }

    pub async fn rollback_save_point(&self, tree: &str) -> Result<()> {
        let writer = self.tree_writer(tree);
        Ok(writer.rollback_save_point().await?)
    }

    pub fn new_batch_commit_number(&self) -> BatchCommit {
        // Take a new batch_commit number
        let batch_commit = BatchCommit(self.next_batch_commit.fetch_add(1, Ordering::SeqCst));
        assert_ne!(batch_commit.0, u64::max_value());
        batch_commit
    }

    pub async fn ready_commit(&self, tree: &str, batch_commit: BatchCommit) -> Result<()> {
        let writer = self.tree_writer(tree);
        Ok(writer.ready_commit(batch_commit).await?)
    }

    pub async fn abort_commit(&self, tree: &str, batch_commit: BatchCommit) -> Result<()> {
        let writer = self.tree_writer(tree);
        Ok(writer.abort_commit(batch_commit).await?)
    }

    pub async fn commit(&self, batch_commit: BatchCommit) -> Result<()> {
        // Next steps are under the commit lock in order
        // to keep commit numbers stored monotonically
        let commit_lock = self.commit_lock.lock().await;

        // Take a new commit number
        let commit = Commit(self.next_commit.fetch_add(1, Ordering::SeqCst));
        assert_ne!(commit.0, u64::max_value());

        // Write the master commit.
        // This is the only source of failure in the commit method,
        // and if this fails then the commit is effectively aborted;
        // if this succeeds then the remaining commit process must succeed.
        self.write_commit(&commit_lock, batch_commit, commit).await?;

        // Infallably promote each tree's writes to its index.
        for (tree, writer) in self.batch_writers.iter() {
            writer.commit_to_index(batch_commit, commit)
        }

        // Bump the view commit limit
        let new_commit_limit = commit.0.checked_add(1).expect("overflow");
        let old_commit_limit = self.view_commit_limit.swap(new_commit_limit, Ordering::SeqCst);
        assert!(old_commit_limit < new_commit_limit);

        Ok(())
    }

    /// NB: This must be called after the batch is committed
    pub async fn close(&self, tree: &str) -> Result<()> {
        let writer = self.tree_writer(tree);
        Ok(writer.close().await?)
    }

    fn tree_writer(&self, tree: &str) -> &tree::BatchWriter {
        self.batch_writers.get(tree).expect("tree")
    }

    async fn write_commit(&self, _commit_lock: &MutexGuard<'_, ()>, batch_commit: BatchCommit, commit: Commit) -> Result<()> {
        Ok(self.commit_log.commit(self.batch, batch_commit, commit).await?)
    }
}

impl ViewReader {
    pub async fn read(&self, tree: &str, key: &Key) -> Result<Option<Value>> {
        let tree = self.trees.get(tree).expect("tree");
        Ok(tree.read(self.commit_limit, key).await?)
    }

    pub fn cursor(&self, tree: &str) -> Cursor {
        let tree = self.trees.get(tree).expect("tree");
        let tree_cursor = tree.cursor(self.commit_limit);

        Cursor {
            tree_cursor,
        }
    }
}

impl Cursor {
    pub fn valid(&self) -> bool {
        self.tree_cursor.valid()
    }

    pub fn key(&self) -> Key {
        self.tree_cursor.key()
    }

    pub async fn value(&mut self) -> Result<Value> {
        Ok(self.tree_cursor.value().await?)
    }

    pub fn next(&mut self) {
        self.tree_cursor.next()
    }

    pub fn prev(&mut self) {
        self.tree_cursor.prev()
    }

    pub fn seek_first(&mut self) {
        self.tree_cursor.seek_first()
    }

    pub fn seek_last(&mut self) {
        self.tree_cursor.seek_last()
    }

    pub fn seek_key(&mut self, key: Key) {
        self.tree_cursor.seek_key(key)
    }

    pub fn seek_key_rev(&mut self, key: Key) {
        self.tree_cursor.seek_key_rev(key)
    }
}

impl fmt::Debug for Db {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Db")
            .field("initialized", &self.initialized)
            .field("next_batch", &self.next_batch)
            .field("next_batch_commit", &self.next_batch_commit)
            .field("next_commit", &self.next_commit)
            .field("view_commit_limit", &self.view_commit_limit)
            .finish()
    }
}

impl fmt::Debug for ViewReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ViewReader")
            .field("commit_limit", &self.commit_limit)
            .finish()
    }
}
