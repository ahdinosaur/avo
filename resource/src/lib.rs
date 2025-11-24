mod epoch;
mod resources;
mod tree;
mod r#type;

pub use crate::epoch::{compute_epochs, EpochError};
pub use crate::r#type::ResourceType;
pub use crate::resources::*;

use ludis_operation::Operation;
use thiserror::Error;

use crate::resources::apt::AptParams;
use crate::resources::apt::{Apt, AptChange, AptResource, AptState};

#[derive(Debug, Clone)]
pub enum ResourceParams {
    Apt(AptParams),
}

#[derive(Debug, Clone)]
pub enum Resource {
    Apt(AptResource),
}

#[derive(Debug, Clone)]
pub enum ResourceState {
    Apt(AptState),
}

#[derive(Error, Debug)]
pub enum ResourceStateError {
    #[error("apt state error: {0}")]
    Apt(#[from] <Apt as ResourceType>::StateError),
}

#[derive(Debug, Clone)]
pub enum ResourceChange {
    Apt(AptChange),
}

impl ResourceParams {
    pub fn resources(self) -> Vec<Resource> {
        fn typed<R: ResourceType>(
            params: R::Params,
            map: impl Fn(R::Resource) -> Resource,
        ) -> Vec<Resource> {
            R::resources(params).into_iter().map(map).collect()
        }

        match self {
            ResourceParams::Apt(params) => typed::<Apt>(params, Resource::Apt),
        }
    }
}

impl Resource {
    pub async fn state(&self) -> Result<ResourceState, ResourceStateError> {
        async fn typed<R: ResourceType>(
            resource: &R::Resource,
            map: impl Fn(R::State) -> ResourceState,
            map_err: impl Fn(R::StateError) -> ResourceStateError,
        ) -> Result<ResourceState, ResourceStateError> {
            R::state(resource).await.map(map).map_err(map_err)
        }

        match self {
            Resource::Apt(resource) => {
                typed::<Apt>(resource, ResourceState::Apt, ResourceStateError::Apt).await
            }
        }
    }

    pub fn change(&self, state: &ResourceState) -> Option<ResourceChange> {
        fn typed<R: ResourceType>(
            resource: &R::Resource,
            state: &R::State,
            map: impl Fn(R::Change) -> ResourceChange,
        ) -> Option<ResourceChange> {
            R::change(resource, state).map(map)
        }

        match (self, state) {
            (Resource::Apt(resource), ResourceState::Apt(state)) => {
                typed::<Apt>(resource, state, ResourceChange::Apt)
            }
            _ => {
                // Programmer error, should never happen, or if it does should be immediately obvious.
                panic!("Unmatched resource and state")
            }
        }
    }
}

impl ResourceChange {
    pub fn operations(self) -> Vec<Operation> {
        match self {
            ResourceChange::Apt(change) => Apt::operations(change),
        }
    }
}
