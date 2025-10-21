use rimu::{ParseError, SourceId, SourceIdFromPathError, Span, SpannedBlock};
use std::{cell::RefCell, io, path::PathBuf, rc::Rc};
use url::Url;

use crate::block::{BlockDefinition, IntoBlockDefinitionError, SpannedBlockDefinition};

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
    NoCode,
    Eval(rimu::EvalError),
    IntoBlockDefinition(IntoBlockDefinitionError),
}

pub fn parse(code: &str, block_id: BlockId) -> Result<SpannedBlockDefinition, ParseError> {
    let source_id = block_id
        .clone()
        .try_into()
        .map_err(|error| ParseError::IncorrectBlockId { block_id, error })?;

    let (ast, errors) = rimu::parse(code, source_id);

    if !errors.is_empty() {
        return Err(ParseError::RimuParse(errors));
    }

    let Some(ast) = ast else {
        return Err(ParseError::NoCode);
    };

    let env = Rc::new(RefCell::new(rimu::Environment::new()));
    let value = rimu::evaluate(&ast, env).map_err(ParseError::Eval)?;

    BlockDefinition::try_from(value).map_err(ParseError::IntoBlockDefinition)
}
