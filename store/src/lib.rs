use std::{fmt::Debug, io, path::PathBuf};

use async_trait::async_trait;

#[async_trait]
pub trait SubStore {
    type ItemId;
    type Error: Debug;

    fn new(cache_dir: PathBuf) -> Self;
    async fn read(&mut self, id: &Self::ItemId) -> Result<Vec<u8>, Self::Error>;
}

#[derive(Debug, Clone)]
pub struct Store {
    local_file_store: LocalFileStore,
}

#[derive(Debug, Clone)]
pub enum StoreItemId {
    LocalFile(PathBuf),
}

#[derive(Debug)]
pub enum StoreError {
    LocalFile(io::Error),
}

impl Store {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            local_file_store: LocalFileStore::new(cache_dir.join("files")),
        }
    }

    pub async fn read(&mut self, id: &StoreItemId) -> Result<Vec<u8>, StoreError> {
        match id {
            StoreItemId::LocalFile(id) => self
                .local_file_store
                .read(id)
                .await
                .map_err(StoreError::LocalFile),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LocalFileStore;

#[async_trait]
impl SubStore for LocalFileStore {
    type ItemId = PathBuf;
    type Error = io::Error;

    fn new(_cache_dir: PathBuf) -> Self {
        Self
    }

    async fn read(&mut self, id: &Self::ItemId) -> Result<Vec<u8>, Self::Error> {
        tokio::fs::read(id).await
    }
}
