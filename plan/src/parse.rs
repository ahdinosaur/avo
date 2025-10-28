//! Parse Rimu source into a Plan (spanned).

use std::{cell::RefCell, rc::Rc};

use displaydoc::Display;
use rimu::Spanned;
use thiserror::Error;

use crate::{
    plan::{Plan, PlanFromRimuError},
    FromRimu, PlanId,
};

#[derive(Debug, Error, Display)]
pub enum ParseError {
    /// Rimu parse failed
    RimuParse(Vec<rimu::ParseError>),
    /// No code found in source
    NoCode,
    /// Evaluating Rimu AST failed
    Eval(#[from] Box<rimu::EvalError>),
    /// Failed to convert Rimu value into Plan
    PlanFromRimu(Box<Spanned<PlanFromRimuError>>),
}

pub fn parse(code: &str, plan_id: &PlanId) -> Result<Spanned<Plan>, ParseError> {
    let source_id = plan_id.clone().into();
    let (ast, errors) = rimu::parse(code, source_id);
    if !errors.is_empty() {
        return Err(ParseError::RimuParse(errors));
    }
    let Some(ast) = ast else {
        return Err(ParseError::NoCode);
    };

    let env = Rc::new(RefCell::new(rimu::Environment::new()));
    let value = rimu::evaluate(&ast, env).map_err(Box::new)?;
    let plan = Plan::from_rimu_spanned(value)
        .map_err(|error| ParseError::PlanFromRimu(Box::new(error)))?;
    Ok(plan)
}
