mod epoch;
mod resources;
mod spec;
mod tree;

pub use crate::resources::*;

pub use epoch::{compute_epochs, EpochError};
pub use resources::apt::{Apt, AptChange, AptResource, AptSpec, AptState};
pub use spec::ResourceType;
pub use tree::{ResourceId, ResourceSpec, ResourceTree};

use std::convert::Infallible;

use ludis_operation::Operation;

#[derive(Debug, Clone)]
pub enum Resource {
    Apt(AptResource),
}

#[derive(Debug, Clone)]
pub enum ResourceState {
    Apt(AptState),
}

#[derive(Debug, Clone)]
pub enum ResourceChange {
    Apt(AptChange),
}

#[derive(Debug, thiserror::Error)]
pub enum ResourceStateError {
    #[error("apt state error (infallible)")]
    Apt(#[from] Infallible),
}

/// Step 1: turn specs of an epoch into atomic resources.
pub fn specs_to_resources(specs: &[ResourceSpec]) -> Vec<Resource> {
    let mut apt_specs = Vec::new();

    for s in specs {
        match s {
            ResourceSpec::Apt(spec) => apt_specs.push(spec.clone()),
        }
    }

    let mut resources = Vec::new();

    if !apt_specs.is_empty() {
        let atoms = Apt::atoms(apt_specs);
        resources.extend(atoms.into_iter().map(Resource::Apt));
    }

    resources
}

/// Step 2: query current state for each resource.
pub async fn query_states(
    resources: &[Resource],
) -> Result<Vec<ResourceState>, ResourceStateError> {
    let mut out = Vec::with_capacity(resources.len());

    for r in resources {
        match r {
            Resource::Apt(res) => {
                let st = Apt::state(res).await?;
                out.push(ResourceState::Apt(st));
            }
        }
    }

    Ok(out)
}

/// Step 3: compute changes from resource + state pairs.
/// Assumes `states` is a response to `resources` in order.
pub fn resources_to_changes(
    resources: &[Resource],
    states: &[ResourceState],
) -> Vec<ResourceChange> {
    assert_eq!(
        resources.len(),
        states.len(),
        "resources_to_changes: lengths differ"
    );

    let mut out = Vec::new();
    for (r, s) in resources.iter().zip(states.iter()) {
        match (r, s) {
            (Resource::Apt(res), ResourceState::Apt(st)) => {
                if let Some(change) = Apt::change(res, st) {
                    out.push(ResourceChange::Apt(change));
                }
            }
        }
    }
    out
}

/// Step 4: convert changes to operations.
pub fn changes_to_operations(changes: &[ResourceChange]) -> Vec<Operation> {
    let mut out = Vec::new();

    for change in changes {
        match change {
            ResourceChange::Apt(ch) => {
                out.extend(Apt::to_operations(ch.clone()).into_iter());
            }
        }
    }

    out
}
