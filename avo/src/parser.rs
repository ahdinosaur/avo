//! Parse Rimu source into a Plan (spanned).

use rimu::{SourceId, SourceIdFromPathError, Spanned};
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use url::Url;

use crate::{
    plan::{IntoPlanError, Plan},
    store::StoreItemId,
    FromRimu,
};

#[derive(Debug, Clone)]
pub enum BlockId {
    Path(PathBuf),
    Git(Url, PathBuf),
}

impl From<BlockId> for StoreItemId {
    fn from(value: BlockId) -> Self {
        match value {
            BlockId::Path(path) => StoreItemId::LocalFile(path),
            BlockId::Git(_url, _path) => todo!(),
        }
    }
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
        error: Box<SourceIdFromBlockIdError>,
    },
    RimuParse(Vec<rimu::ParseError>),
    NoCode,
    Eval(Box<rimu::EvalError>),
    IntoPlan(Box<Spanned<IntoPlanError>>),
}

pub fn parse(code: &str, block_id: BlockId) -> Result<Spanned<Plan>, ParseError> {
    let source_id = block_id
        .clone()
        .try_into()
        .map_err(|error| ParseError::IncorrectBlockId {
            block_id,
            error: Box::new(error),
        })?;

    let (ast, errors) = rimu::parse(code, source_id);
    if !errors.is_empty() {
        return Err(ParseError::RimuParse(errors));
    }

    let Some(ast) = ast else {
        return Err(ParseError::NoCode);
    };

    let env = Rc::new(RefCell::new(rimu::Environment::new()));
    let value = rimu::evaluate(&ast, env).map_err(|error| ParseError::Eval(Box::new(error)))?;
    Plan::from_rimu_spanned(value).map_err(|error| ParseError::IntoPlan(Box::new(error)))
}
