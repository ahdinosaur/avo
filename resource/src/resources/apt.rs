use async_trait::async_trait;
use indexmap::indexmap;
use ludis_operation::ops::apt::AptOperation;
use ludis_operation::Operation;
use ludis_params::{ParamField, ParamType, ParamTypes};
use rimu::{SourceId, Span, Spanned};
use serde::Deserialize;

use crate::ResourceType;

/// Atomic resource (per-package)
#[derive(Debug, Clone)]
pub struct AptResource {
    pub package: String,
}

/// A minimal state model (placeholder).
#[derive(Debug, Clone)]
pub struct AptState {
    pub installed: bool,
}

/// A change needed to reach desired state.
#[derive(Debug, Clone)]
pub enum AptChange {
    Install { package: String },
}

#[derive(Debug, Clone)]
pub struct Apt;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AptParams {
    Package { package: String },
    Packages { packages: Vec<String> },
}

#[async_trait]
impl ResourceType for Apt {
    const ID: &'static str = "apt";

    fn param_types() -> Option<Spanned<ParamTypes>> {
        let span = Span::new(SourceId::empty(), 0, 0);
        Some(Spanned::new(
            ParamTypes::Union(vec![
                indexmap! {
                    "package".to_string() =>
                        Spanned::new(ParamField::new(ParamType::String), span.clone()),
                },
                indexmap! {
                    "packages".to_string() => Spanned::new(
                        ParamField::new(ParamType::List {
                            item: Box::new(Spanned::new(ParamType::String, span.clone())),
                        }),
                        span.clone(),
                    ),
                },
            ]),
            span,
        ))
    }

    type Params = AptParams;
    type Resource = AptResource;

    fn resources(params: Self::Params) -> Vec<Self::Resource> {
        match params {
            AptParams::Package { package } => vec![AptResource { package }],
            AptParams::Packages { packages } => packages
                .into_iter()
                .map(|package| AptResource { package })
                .collect(),
        }
    }

    type State = AptState;
    type StateError = std::convert::Infallible;
    async fn state(_resource: &Self::Resource) -> Result<Self::State, Self::StateError> {
        // For demo purposes: always claim not installed.
        Ok(AptState { installed: false })
    }

    type Change = AptChange;
    fn change(resource: &Self::Resource, state: &Self::State) -> Option<Self::Change> {
        if state.installed {
            None
        } else {
            Some(AptChange::Install {
                package: resource.package.clone(),
            })
        }
    }

    fn operations(change: Self::Change) -> Vec<Operation> {
        match change {
            AptChange::Install { package } => {
                vec![Operation::Apt(AptOperation::Install {
                    packages: vec![package],
                })]
            }
        }
    }
}
