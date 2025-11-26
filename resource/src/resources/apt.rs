use async_trait::async_trait;
use indexmap::indexmap;
use lusid_causality::Tree;
use lusid_cmd::{Command, CommandError};
use lusid_operation::ops::apt::AptOperation;
use lusid_operation::Operation;
use lusid_params::{ParamField, ParamType, ParamTypes};
use rimu::{SourceId, Span, Spanned};
use serde::Deserialize;
use thiserror::Error;

use crate::ResourceType;

#[derive(Debug, Clone)]
pub struct AptResource {
    pub package: String,
}

#[derive(Debug, Clone)]
pub enum AptState {
    NotInstalled,
    Installed,
}

#[derive(Error, Debug)]
pub enum AptStateError {
    #[error(transparent)]
    Command(#[from] CommandError),

    #[error("failed to parse status: {status}")]
    ParseStatus { status: String },
}

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

    fn resources(params: Self::Params) -> Vec<Tree<Self::Resource>> {
        match params {
            AptParams::Package { package } => vec![Tree::leaf(AptResource { package })],
            AptParams::Packages { packages } => vec![Tree::branch(
                packages
                    .into_iter()
                    .map(|package| Tree::leaf(AptResource { package }))
                    .collect(),
            )],
        }
    }

    type State = AptState;
    type StateError = AptStateError;
    async fn state(resource: &Self::Resource) -> Result<Self::State, Self::StateError> {
        let (_status, stdout, error_value) = Command::new("dpkg-query")
            .args(["-W", "-f='${Status}'", &resource.package])
            .run_with_error_handler(|stderr| {
                let stderr = String::from_utf8_lossy(stderr);
                if stderr.contains("no packages found matching") {
                    Some(AptState::NotInstalled)
                } else {
                    None
                }
            })
            .await?;

        if let Some(state) = error_value {
            return Ok(state);
        }

        let stdout = String::from_utf8_lossy(&stdout);
        let status_parts: Vec<_> = stdout.trim_matches('\'').split(" ").collect();
        let Some(status) = status_parts.get(2) else {
            return Err(AptStateError::ParseStatus {
                status: stdout.to_string(),
            });
        };
        match *status {
            "not-installed" => Ok(AptState::NotInstalled),
            "unpacked" => Ok(AptState::NotInstalled),
            "half-installed" => Ok(AptState::NotInstalled),
            "installed" => Ok(AptState::Installed),
            "config-files" => Ok(AptState::NotInstalled),
            _ => Err(AptStateError::ParseStatus {
                status: stdout.to_string(),
            }),
        }
    }

    type Change = AptChange;
    fn change(resource: &Self::Resource, state: &Self::State) -> Option<Self::Change> {
        match state {
            AptState::Installed => None,
            AptState::NotInstalled => Some(AptChange::Install {
                package: resource.package.clone(),
            }),
        }
    }

    fn operations(change: Self::Change) -> Vec<Tree<Operation>> {
        match change {
            AptChange::Install { package } => {
                vec![Tree::leaf(Operation::Apt(AptOperation::Install {
                    packages: vec![package],
                }))]
            }
        }
    }
}
