use async_trait::async_trait;
use ludis_operation::Operation;
use ludis_params::ParamTypes;
use rimu::Spanned;
use serde::de::DeserializeOwned;

/// ResourceType:
/// - ParamTypes for Rimu schema
/// - Resource (atom)
/// - State (current)
/// - Change (delta needed)
/// - Conversion from Change -> Operation(s)
#[async_trait]
pub trait ResourceType {
    const ID: &'static str;

    fn param_types() -> Option<Spanned<ParamTypes>>;

    type Params: DeserializeOwned;
    type Resource: Clone;

    fn resources(params: Self::Params) -> Vec<Self::Resource>;

    type State;
    type StateError;
    async fn state(resource: &Self::Resource) -> Result<Self::State, Self::StateError>;

    type Change;
    fn change(resource: &Self::Resource, state: &Self::State) -> Option<Self::Change>;

    fn operations(change: Self::Change) -> Vec<Operation>;
}
