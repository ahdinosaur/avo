use avo_params::ParamValues;
use displaydoc::Display;
use rimu::{Spanned, Value, call};
use rimu_interop::FromRimu;
use thiserror::Error;

use crate::plan::{IntoPlanActionError, PlanAction, SetupFunction};

#[derive(Debug, Error, Display)]
pub enum EvalError {
    /// Calling setup function failed
    Call(Box<rimu::EvalError>),
    /// Setup returned a non-list value
    ReturnedNotList,
    /// Invalid PlanAction value
    InvalidPlanAction(Box<Spanned<IntoPlanActionError>>),
}

pub(crate) fn evaluate(
    setup: Spanned<SetupFunction>,
    params: Spanned<ParamValues>,
) -> Result<Vec<Spanned<PlanAction>>, EvalError> {
    let (setup, setup_span) = setup.take();
    let (params, params_span) = params.take();

    let args = vec![Spanned::new(params.into_rimu(), params_span)];
    let result =
        call(setup_span, setup.0, &args).map_err(|error| EvalError::Call(Box::new(error)))?;
    let (result, _result_span) = result.take();

    let Value::List(items) = result else {
        return Err(EvalError::ReturnedNotList);
    };

    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let call = PlanAction::from_rimu_spanned(item)
            .map_err(|error| EvalError::InvalidPlanAction(Box::new(error)))?;
        out.push(call)
    }
    Ok(out)
}
