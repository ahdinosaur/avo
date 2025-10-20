use std::{io, path::PathBuf};

use async_trait::async_trait;

pub struct StoreContext {
    cache_dir: PathBuf,
}

#[async_trait]
pub trait Store {
    const STORE_ID: &str;
    type ItemId;
    type Error;

    async fn read(&self, id: &Self::ItemId, ctx: &StoreContext) -> Result<Vec<u8>, Self::Error>;
}

#[derive(Debug, Clone, Default)]
pub struct LocalFileStore;

#[async_trait]
impl Store for LocalFileStore {
    const STORE_ID: &str = "files";
    type ItemId = PathBuf;
    type Error = io::Error;

    async fn read(&self, id: &Self::ItemId, _ctx: &StoreContext) -> Result<Vec<u8>, Self::Error> {
        tokio::fs::read(id).await
    }
}
