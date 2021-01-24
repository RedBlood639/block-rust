use anyhow::Result;
use std::sync::Arc;
use std::path::PathBuf;

use crate::basic_db as bdb;

#[derive(Clone, Debug)]
pub struct DbConfig {
    dir: PathBuf,
    trees: Vec<String>,
}

#[derive(Clone)]
pub struct Db {
    inner: Arc<bdb::Db>,
    config: Arc<DbConfig>,
}

pub struct WriteBatch {
    inner: bdb::BatchWriter,
}

pub struct ReadView {
    inner: bdb::ViewReader,
}

pub struct WriteTree<'batch> {
    batch: &'batch WriteBatch,
}

pub struct ReadTree<'view> {
    view: &'view ReadView,
}

pub struct Cursor {
    inner: bdb::Cursor,
}

impl Db {
    pub async fn open(config: DbConfig) -> Result<Db> {
        panic!()
    }

    pub fn write_batch(&self) -> WriteBatch {
        panic!()
    }

    pub fn read_view(&self) -> ReadView {
        panic!()
    }

    pub async fn sync(&self) -> Result<()> {
        panic!()
    }
}

impl WriteBatch {
    pub fn tree<'batch>(&'batch self, tree: &str) -> WriteTree<'batch> {
        panic!()
    }

    pub async fn commit(self) -> Result<()> {
        panic!()
    }

    pub fn abort(self) {
        panic!()
    }
}

impl ReadView {
    pub fn tree<'view>(&'view self, tree: &str) -> ReadTree<'view> {
        panic!()
    }
}

impl<'batch> WriteTree<'batch> {
    pub fn write(&self, key: &[u8], value: &[u8]) {
        panic!()
    }

    pub fn delete(&self, key: &[u8]) {
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