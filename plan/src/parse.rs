//! Parse Rimu source into a Plan (spanned).

use std::{cell::RefCell, rc::Rc};

use displaydoc::Display;
use rimu::Spanned;
use thiserror::Error;

use crate::{
    plan::{IntoPlanError, Plan},
    FromRimu, PlanId,
};

#[derive(Debug, Error, Display)]
pub enum ParseError {
    /// Rimu parse failed
    RimuParse(Vec<rimu::ParseError>),
    /// No code found in source
    NoCode,
    /// Evaluating Rimu AST failed
    Eval(Box<rimu::EvalError>),
    /// Failed to convert Rimu value into Plan
    IntoPlan(Box<Spanned<IntoPlanError>>),
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
    let value = rimu::evaluate(&ast, env).map_err(|error| ParseError::Eval(Box::new(error)))?;
    Plan::from_rimu_spanned(value).map_err(|error| ParseError::IntoPlan(Box::new(error)))
}
