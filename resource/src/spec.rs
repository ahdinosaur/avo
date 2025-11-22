use async_trait::async_trait;
use ludis_operation::Operation;
use ludis_params::ParamTypes;
use rimu::Spanned;
use serde::de::DeserializeOwned;

/// ResourceType:
/// - ParamTypes for Rimu schema
/// - Spec (desired state)
/// - Resource (atom)
/// - State (current)
/// - Change (delta needed)
/// - Conversion from Change -> Operation(s)
#[async_trait]
pub trait ResourceType {
    const ID: &'static str;

    fn param_types() -> Option<Spanned<ParamTypes>>;

    type Params: DeserializeOwned;
    type Spec: Clone;

    fn spec(params: Self::Params) -> Self::Spec;

    type Resource: Clone;
    fn atoms(specs: impl IntoIterator<Item = Self::Spec>) -> Vec<Self::Resource>;

    type State;
    type StateError;

    fn change(resource: &Self::Resource, state: &Self::State) -> Option<Self::Change>;

    type Change;

    fn to_operations(change: Self::Change) -> Vec<Operation>;

    async fn state(resource: &Self::Resource) -> Result<Self::State, Self::StateError>;
}
