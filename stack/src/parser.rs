use rimu::{Block, EvalError, ParseError, SourceId, SourceIdFromPathError, Span, SpannedBlock};
use std::{io, path::PathBuf};
use url::Url;

use crate::block::{BlockDefinition, SpannedBlockDefinition};

#[derive(Debug, Clone)]
pub enum BlockId {
    Path(PathBuf),
    Git(Url, PathBuf),
}

pub enum SourceIdFromBlockIdError {
    Path(SourceIdFromPathError),
}

impl TryFrom<BlockId> for SourceId {
    type Error = SourceIdFromBlockIdError;
    fn try_from(value: BlockId) -> Result<Self, Self::Error> {
        match value {
            BlockId::Path(path) => {
                SourceId::from_path(path).map_err(SourceIdFromBlockIdError::Path)
            }
            BlockId::Git(mut url, path) => {
                url.query_pairs_mut()
                    .append_pair("path", &path.to_string_lossy());
                Ok(SourceId::Url(url))
            }
        }
    }
}

pub enum ParseError {
    IncorrectBlockId {
        block_id: BlockId,
        error: SourceIdFromBlockIdError,
    },
    RimuParse(Vec<rimu::ParseError>),
    ParsingReturnedNone,
    EvaluatingBlockDefinition {
        block: rimu::Block,
        span: Span,
        error: EvalError,
    },
}

pub fn parse(code: &str, block_id: BlockId) -> Result<SpannedBlockDefinition, ParseError> {
    let source_id = block_id
        .clone()
        .try_into()
        .map_err(|error| ParseError::IncorrectBlockId { block_id, error })?;

    let (output, errors) = rimu::parse(code, source_id);

    if !errors.is_empty() {
        return Err(ParseError::RimuParse(errors));
    }

    let Some(output) = output else {
        return Err(ParseError::ParsingReturnedNone);
    }

    let (block, span) = output.take();
    let block_definition = BlockDefinition::try_from(block).map_err(|block| EvaluatingBlockDefinition;
    Span::new(block_definition, span)

}
