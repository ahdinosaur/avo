use std::{io, path::PathBuf};

use async_trait::async_trait;

#[async_trait]
pub trait SubStore {
    type ItemId;
    type Error;

    fn new(cache_dir: PathBuf) -> Self;
    async fn read(&self, id: &Self::ItemId) -> Result<Vec<u8>, Self::Error>;
}

pub struct Store {
    local_file_store: LocalFileStore,
}

pub enum StoreItemId {
    LocalFile(<LocalFileStore as SubStore>::ItemId),
}

pub enum StoreError {
    LocalFile(<LocalFileStore as SubStore>::Error),
}

impl Store {
    fn new(cache_dir: PathBuf) -> Self {
        Self {
            local_file_store: LocalFileStore::new(cache_dir.join("files")),
        }
    }

    async fn read(&self, id: &StoreItemId) -> Result<Vec<u8>, StoreError> {
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

    async fn read(&self, id: &Self::ItemId) -> Result<Vec<u8>, Self::Error> {
        tokio::fs::read(id).await
    }
}
