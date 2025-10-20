use rimu::SourceId;
use std::{io, path::PathBuf};
use url::Url;

use crate::block::BlockDefinition;

#[derive(Debug, Clone)]
pub enum BlockId {
    Path(PathBuf),
    Git(Url, PathBuf),
}

impl TryInto<SourceId> for BlockId {
    fn try_into(self) -> Result<SourceId, Self::Error> {
        match self {
            BlockId::Path(path) => SourceId::from_path(path),
            BlockId::Git(url) => SourceId::Url(url),
        }
    }
}

pub enum ParseError {
    BadBlockId { block_id: BlockId },
}

pub fn parse(block_id: BlockId) -> Result<BlockDefinition, ParseError> {
    let source_id = block_id
        .clone()
        .try_into()
        .map_err(|_| ParseError::BadBlockId { block_id })?;

    rimu::parse(code, source)
}
