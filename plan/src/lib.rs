use displaydoc::Display;
use lusid_causality::{CausalityMeta, CausalityTree, NodeId};
use lusid_params::{validate, ParamValues, ParamsValidationError};
use lusid_resource::ResourceParams;
use lusid_store::{Store, StoreError, StoreItemId};
use rimu::Spanned;
use std::{path::PathBuf, string::FromUtf8Error};
use thiserror::Error;

mod core;
mod eval;
mod id;
mod load;
mod model;

pub use crate::id::PlanId;
use crate::{
    core::{core_module, is_core_module},
    model::Plan,
};
use crate::{
    eval::{evaluate, EvalError},
    load::{load, LoadError},
};

#[derive(Debug, Error, Display)]
pub enum PlanError {
    /// Failed to read plan source from store for id {id:?}
    StoreRead {
        id: StoreItemId,
        #[source]
        source: StoreError,
    },

    /// Failed to decode plan source as UTF-8
    InvalidUtf8(#[from] FromUtf8Error),

    /// Failed to load plan source
    Load(#[from] LoadError),

    /// Parameter validation failed
    Validate(#[from] ParamsValidationError),

    /// Failed to evaluate plan setup
    Eval(#[from] EvalError),

    /// Failed to convert plan item to resource
    PlanActionToResource(#[from] PlanActionToResourceError),
}

/// Top-level planning routine: load plan, validate parameters, and evaluate to
/// a CausalityTree<Resource>.
#[tracing::instrument(skip_all)]
pub async fn plan(
    plan_id: PlanId,
    param_values: Option<Spanned<ParamValues>>,
    store: &mut Store,
) -> Result<CausalityTree<ResourceParams>, PlanError> {
    tracing::debug!("Plan {plan_id:?} with params {param_values:?}");
    let children = plan_recursive(plan_id, param_values.as_ref(), store).await?;
    let tree = CausalityTree::Branch {
        id: None,
        meta: CausalityMeta::default(),
        children,
    };
    tracing::trace!("Planned resource tree: {:?}", tree);
    Ok(tree)
}

async fn plan_recursive(
    plan_id: PlanId,
    param_values: Option<&Spanned<ParamValues>>,
    store: &mut Store,
) -> Result<Vec<CausalityTree<ResourceParams>>, PlanError> {
    let store_item_id: StoreItemId = plan_id.clone().into();
    let bytes = store
        .read(&store_item_id)
        .await
        .map_err(|source| PlanError::StoreRead {
            id: store_item_id.clone(),
            source,
        })?;
    let code = String::from_utf8(bytes)?;
    let plan = load(&code, &plan_id)?;

    let Plan {
        name: _,
        version: _,
        params: param_types,
        setup,
    } = plan.into_inner();

    validate(param_types.as_ref(), param_values)?;

    let plan_actions = evaluate(setup, param_values.cloned())?;

    let mut resources = Vec::with_capacity(plan_actions.len());
    for plan_action in plan_actions {
        let node = Box::pin(plan_action_to_resource(plan_action, &plan_id, store)).await?;
        resources.push(node);
    }

    Ok(resources)
}

#[derive(Debug, Error, Display)]
pub enum PlanActionToResourceError {
    /// Missing required parameters in plan action
    MissingParams,

    /// Parameters validation for resource failed
    ParamsValidation(#[from] ParamsValidationError),

    /// Failed to convert parameter values to resource params
    SerdeValue(#[from] rimu::SerdeValueError),

    /// Unsupported core module id \"{id}\"
    UnsupportedCoreModuleId { id: String },

    /// Failed to compute subtree for nested plan
    PlanSubtree(#[from] Box<PlanError>),
}

async fn plan_action_to_resource(
    plan_action: Spanned<crate::model::PlanAction>,
    current_plan_id: &PlanId,
    store: &mut Store,
) -> Result<CausalityTree<ResourceParams>, PlanActionToResourceError> {
    let (plan_action, _span) = plan_action.take();
    let crate::model::PlanAction {
        id,
        ref module,
        params: param_values,
        before,
        after,
    } = plan_action;

    let id = id.map(|id| NodeId::new(id.into_inner()));
    let before = before
        .into_iter()
        .map(|v| v.into_inner())
        .map(NodeId::new)
        .collect();
    let after = after
        .into_iter()
        .map(|v| v.into_inner())
        .map(NodeId::new)
        .collect();

    if let Some(core_module_id) = is_core_module(module) {
        let params = core_module(core_module_id, param_values)?;
        Ok(CausalityTree::Leaf {
            id,
            node: params,
            meta: CausalityMeta { before, after },
        })
    } else {
        let path = PathBuf::from(module.inner());
        let plan_id = current_plan_id.join(path);
        let children = plan_recursive(plan_id, param_values.as_ref(), store)
            .await
            .map_err(Box::new)?;
        Ok(CausalityTree::Branch {
            id,
            children,
            meta: CausalityMeta { before, after },
        })
    }
}
