use log::error;
use std::fs::{self, File};
use std::collections::BTreeMap;
use anyhow::Result;
use std::sync::Arc;
use std::path::{PathBuf, Path};
use crate::log::Log;
use crate::simple_log_file;
use crate::command::Command;
use crate::commit_log::CommitCommand;
use crate::fs_thread::FsThread;
use crate::basic_db as bdb;

#[derive(Clone, Debug)]
pub struct DbConfig {
    dir: PathBuf,
    trees: Vec<String>,
}

#[derive(Clone)]
pub struct Db {
    config: Arc<DbConfig>,
    inner: Arc<bdb::Db>,
    trees: Arc<Vec<String>>,
    dir_handle: Option<Arc<File>>, // Unix only
}

pub struct WriteBatch {
    inner: bdb::BatchWriter,
    trees: Arc<Vec<String>>,
    closed: bool,
}

pub struct ReadView {
    inner: bdb::ViewReader,
}

pub struct WriteTree<'batch> {
    tree: String,
    batch: &'batch WriteBatch,
}

pub struct ReadTree<'view> {
    tree: String,
    view: &'view ReadView,
}

pub struct Cursor {
    inner: bdb::Cursor,
}

impl Db {
    pub async fn open(config: DbConfig) -> Result<Db> {
        let (tree_logs, commit_log) = make_logs(&config)?;

        let db = bdb::Db::new(tree_logs, commit_log);
        db.init().await?;

        let dir_handle = if cfg!(unix) {
            // FIXME async file open
            Some(Arc::new(File::open(&config.dir)?))
        } else {
            None
        };

        let trees = Arc::new(config.trees.clone());

        return Ok(Db {
            config: Arc::new(config),
            inner: Arc::new(db),
            trees,
            dir_handle,
        });

        fn make_logs(config: &DbConfig) -> Result<(BTreeMap<String, Log<Command>>, Log<CommitCommand>)> {
            // FIXME: async create dir
            fs::create_dir_all(&config.dir)?;

            let fs_thread = Arc::new(FsThread::start()?);

            let tree_logs = config.trees.iter()
                .map(|tree| {
                    let path = config.dir.join(format!("{}.toml", tree));
                    (tree.clone(), path)
                });

            assert!(!config.trees.iter().any(|t| t == "commits"));
            let commit_log = config.dir.join(format!("commits.toml"));

            let tree_logs = tree_logs.into_iter()
                .map(|(tree, path)| {
                    (tree, Log::new(simple_log_file::create(path, fs_thread.clone())))
                }).collect();

            let commit_log = Log::new(simple_log_file::create(commit_log, fs_thread.clone()));

            Ok((tree_logs, commit_log))
        }
    }

    pub fn write_batch(&self) -> WriteBatch {
        WriteBatch {
            inner: self.inner.batch(),
            trees: self.trees.clone(),
            closed: false,
        }
    }

    pub fn read_view(&self) -> ReadView {
        ReadView {
            inner: self.inner.view(),
        }
    }

    pub async fn sync(&self) -> Result<()> {
        self.inner.sync().await?;

        // Also need to sync the directory
        if let Some(dir) = &self.dir_handle {
            // FIXME async
            dir.sync_all()?;
        }

        Ok(())
    }
}

impl WriteBatch {
    pub fn tree<'batch>(&'batch self, tree: &str) -> WriteTree<'batch> {
        WriteTree {
            tree: tree.to_string(),
            batch: self,
        }
    }

    pub async fn commit(&self) -> Result<()> {
        let batch_commit = self.inner.new_batch_commit_number();
        let mut error = None;
        for tree in self.trees.iter() {
            if error.is_none() {
                let r = self.inner.ready_commit(tree, batch_commit).await;
                if let Err(e) = r {
                    error = Some(e);
                }
            } else {
                let r = self.inner.abort_commit(tree, batch_commit).await;
                if let Err(e) = r {
                    error!("error aborting batch commit {} for batch {} for tree {}: {}",
                           batch_commit.0, self.inner.number().0, tree, e);
                }
            }
        }

        if let Some(e) = error {
            return Err(e);
        }

        self.inner.commit(batch_commit).await?;

        Ok(())
    }

    pub async fn abort(&self) {
        let batch_commit = self.inner.new_batch_commit_number();
        for tree in self.trees.iter() {
            let r = self.inner.abort_commit(tree, batch_commit).await;
            if let Err(e) = r {
                error!("error aborting batch commit {} for batch {} for tree {}: {}",
                       batch_commit.0, self.inner.number().0, tree, e);
            }
        }
    }

    pub async fn close(mut self) {
        for tree in self.trees.iter() {
            let r = self.inner.close(tree).await;
            if let Err(e) = r {
                error!("error closing batch {} for tree {}: {}",
                       self.inner.number().0, tree, e);
            }
        }

        self.closed = true;
    }
}

impl Drop for WriteBatch {
    fn drop(&mut self) {
        if !self.closed {
            error!("write batch {} not closed", self.inner.number().0);
            // TODO: last-ditch attempt in another thread?
        }
    }
}

impl ReadView {
    pub fn tree<'view>(&'view self, tree: &str) -> ReadTree<'view> {
        ReadTree {
            tree: tree.to_string(),
            view: self,
        }
    }
}

impl<'batch> WriteTree<'batch> {
    pub async fn write(&self, key: &[u8], value: &[u8]) -> Result<()> {
        panic!()
    }

    pub async fn delete(&self, key: &[u8]) -> Result<()> {
        panic!()
    }

    pub async fn delete_range(&self, start_key: &[u8], end_key: &[u8]) -> Result<()> {
        panic!()
    }
}

impl<'view> ReadTree<'view> {
    pub async fn read(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        panic!()
    }

    pub fn cursor(&self) -> Cursor {
        panic!()
    }
}

impl Cursor {
    pub fn valid(&self) -> bool {
        panic!()
    }

    pub async fn next(&mut self) -> Result<()> {
        panic!()
    }

    pub async fn prev(&mut self) -> Result<()> {
        panic!()
    }

    pub fn key_value(&self) -> (&[u8], &[u8]) {
        panic!()
    }

    pub async fn seek_first(&mut self) -> Result<()> {
        panic!()
    }

    pub async fn seek_last(&mut self) -> Result<()> {
        panic!()
    }

    pub async fn seek_key(&mut self, key: &[u8]) -> Result<()> {
        panic!()
    }

    pub async fn seek_key_rev(&mut self, key: &[u8]) -> Result<()> {
        panic!()
    }
}

