//! Parse Rimu source into a Plan (spanned).

use rimu::Spanned;
use std::{cell::RefCell, rc::Rc};

use crate::{
    plan::{IntoPlanError, Plan},
    FromRimu, PlanId,
};

#[derive(Debug)]
pub enum ParseError {
    RimuParse(Vec<rimu::ParseError>),
    NoCode,
    Eval(Box<rimu::EvalError>),
    IntoPlan(Box<Spanned<IntoPlanError>>),
}

pub fn parse(code: &str, block_id: PlanId) -> Result<Spanned<Plan>, ParseError> {
    let source_id = block_id.into();

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
