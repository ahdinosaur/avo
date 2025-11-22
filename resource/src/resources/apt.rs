use async_trait::async_trait;
use indexmap::indexmap;
use ludis_operation::ops::apt::AptOperation;
use ludis_operation::Operation;
use ludis_params::{ParamField, ParamType, ParamTypes};
use rimu::{SourceId, Span, Spanned};
use serde::Deserialize;

use crate::ResourceType;

/// Desired Apt state spec for resource tree leaves.
#[derive(Debug, Clone)]
pub struct AptSpec {
    pub packages: Vec<String>,
}

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
    type Spec = AptSpec;

    fn spec(params: Self::Params) -> Self::Spec {
        match params {
            AptParams::Package { package } => AptSpec {
                packages: vec![package],
            },
            AptParams::Packages { packages } => AptSpec { packages },
        }
    }

    type Resource = AptResource;

    fn atoms(specs: impl IntoIterator<Item = Self::Spec>) -> Vec<Self::Resource> {
        let mut out = Vec::new();
        for spec in specs {
            for p in spec.packages {
                out.push(AptResource { package: p });
            }
        }
        out
    }

    type State = AptState;
    type StateError = std::convert::Infallible;

    fn change(resource: &Self::Resource, state: &Self::State) -> Option<Self::Change> {
        if state.installed {
            None
        } else {
            Some(AptChange::Install {
                package: resource.package.clone(),
            })
        }
    }

    type Change = AptChange;

    fn to_operations(change: Self::Change) -> Vec<Operation> {
        match change {
            AptChange::Install { package } => {
                vec![Operation::Apt(AptOperation::Install {
                    packages: vec![package],
                })]
            }
        }
    }

    // For demo purposes: always claim not installed.
    async fn state(_resource: &Self::Resource) -> Result<Self::State, Self::StateError> {
        Ok(AptState { installed: false })
    }
}
