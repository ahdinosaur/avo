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
pub enum PlanId {
    Path(PathBuf),
    Git(Url, PathBuf),
}

impl From<PlanId> for StoreItemId {
    fn from(value: PlanId) -> Self {
        match value {
            PlanId::Path(path) => StoreItemId::LocalFile(path),
            PlanId::Git(_url, _path) => todo!(),
        }
    }
}

#[derive(Debug)]
pub enum SourceIdFromPlanIdError {
    Path(SourceIdFromPathError),
}

impl TryFrom<PlanId> for SourceId {
    type Error = SourceIdFromPlanIdError;

    fn try_from(value: PlanId) -> Result<Self, Self::Error> {
        match value {
            PlanId::Path(path) => SourceId::from_path(path).map_err(SourceIdFromPlanIdError::Path),
            PlanId::Git(mut url, path) => {
                url.query_pairs_mut()
                    .append_pair("path", &path.to_string_lossy());
                Ok(SourceId::Url(url))
            }
        }
    }
}

#[derive(Debug)]
pub enum ParseError {
    IncorrectPlanId {
        block_id: PlanId,
        error: Box<SourceIdFromPlanIdError>,
    },
    RimuParse(Vec<rimu::ParseError>),
    NoCode,
    Eval(Box<rimu::EvalError>),
    IntoPlan(Box<Spanned<IntoPlanError>>),
}

pub fn parse(code: &str, block_id: PlanId) -> Result<Spanned<Plan>, ParseError> {
    let source_id = block_id
        .clone()
        .try_into()
        .map_err(|error| ParseError::IncorrectPlanId {
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
