pub use crate::resources::*;

use async_trait::async_trait;
use lusid_causality::Tree;
use lusid_operation::Operation;
use lusid_params::ParamTypes;
use rimu::Spanned;
use serde::de::DeserializeOwned;
use thiserror::Error;

mod resources;

use crate::resources::apt::AptParams;
use crate::resources::apt::{Apt, AptChange, AptResource, AptState};

/// ResourceType:
/// - ParamTypes for Rimu schema
/// - Resource (atom)
/// - State (current)
/// - Change (delta needed)
/// - Conversion from Change -> Operation(s)
#[async_trait]
pub trait ResourceType {
    const ID: &'static str;

    /// Schema for resource params.
    fn param_types() -> Option<Spanned<ParamTypes>>;

    /// Resource params (friendly user definition).
    type Params: DeserializeOwned;

    /// Resource atom (indivisible system definition).
    type Resource: Clone;

    /// Create resource atom from params.
    fn resources(params: Self::Params) -> Vec<Tree<Self::Resource>>;

    /// Current state of resource on machine.
    type State;

    /// Possible error when fetching current state of resource on machine.
    type StateError;

    /// Fetch current state of resource on machine.
    async fn state(resource: &Self::Resource) -> Result<Self::State, Self::StateError>;

    /// A change from current state.
    type Change;

    /// Get change atomic resource from current state to intended state.
    fn change(resource: &Self::Resource, state: &Self::State) -> Option<Self::Change>;

    // Convert atomic resource change into operations (mutations).
    fn operations(change: Self::Change) -> Vec<Tree<Operation>>;
}

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
    pub fn resources(self) -> Vec<Tree<Resource>> {
        fn typed<R: ResourceType>(
            params: R::Params,
            map: impl Fn(R::Resource) -> Resource + Copy,
        ) -> Vec<Tree<Resource>> {
            R::resources(params)
                .into_iter()
                .map(|tree| tree.map(map))
                .collect()
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
    pub fn operations(self) -> Vec<Tree<Operation>> {
        match self {
            ResourceChange::Apt(change) => Apt::operations(change),
        }
    }
}
