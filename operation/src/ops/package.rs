use async_trait::async_trait;
use indexmap::indexmap;
use ludis_params::{ParamField, ParamType, ParamTypes};
use rimu::{SourceId, Span, Spanned};
use serde::Deserialize;

use crate::{spec::OperationSpec, Operation};

/// A package operation.
#[derive(Debug, Clone)]
pub struct PackageOperation {
    packages: Vec<String>,
}

/// A single-package atom.
#[derive(Debug, Clone)]
pub struct PackageOperationAtom {
    package: String,
}

/// A concrete package change.
#[derive(Debug, Clone)]
pub enum PackageOperationDelta {
    Install { package: String },
}

impl From<PackageOperation> for Operation {
    fn from(value: PackageOperation) -> Self {
        Operation::Package(value)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PackageParams {
    Package { package: String },
    Packages { packages: Vec<String> },
}

pub struct PackageSpec;

#[async_trait]
impl OperationSpec for PackageSpec {
    const ID: &str = "package";

    fn param_types() -> Option<Spanned<ParamTypes>> {
        let span = Span::new(SourceId::empty(), 0, 0);
        Some(Spanned::new(
            ParamTypes::Union(vec![
                indexmap! {
                    "package".to_string() =>
                        Spanned::new(ParamField::new(ParamType::String), span.clone())
                },
                indexmap! {
                    "packages".to_string() =>
                        Spanned::new(
                            ParamField::new(ParamType::List {
                                item: Box::new(Spanned::new(ParamType::String, span.clone())),
                            }),
                            span.clone(),
                        )
                },
            ]),
            span,
        ))
    }

    type Params = PackageParams;
    type Operation = PackageOperation;

    fn operation(params: Self::Params) -> Self::Operation {
        match params {
            PackageParams::Package { package } => Self::Operation {
                packages: vec![package],
            },
            PackageParams::Packages { packages } => Self::Operation { packages },
        }
    }

    type Atom = PackageOperationAtom;

    fn atoms(ops: impl IntoIterator<Item = Self::Operation>) -> Vec<Self::Atom> {
        let mut atoms = Vec::new();
        for op in ops {
            for pkg in op.packages {
                atoms.push(PackageOperationAtom { package: pkg });
            }
        }
        atoms
    }

    type Delta = PackageOperationDelta;
    type DeltaError = ();

    async fn delta(atom: Self::Atom) -> Result<Option<Self::Delta>, Self::DeltaError> {
        // TODO, check if the package is already installed.
        Ok(Some(PackageOperationDelta::Install {
            package: atom.package,
        }))
    }

    type ApplyError = ();

    async fn apply(deltas: Vec<Self::Delta>) -> Result<(), Self::ApplyError> {
        // Merge all packages into one batched install.
        let mut install: Vec<String> = Vec::new();
        for delta in deltas {
            match delta {
                PackageOperationDelta::Install { package } => {
                    install.push(package);
                }
            }
        }

        install.sort();
        install.dedup();

        // TODO, actually install packages
        if install.is_empty() {
            tracing::info!("[pkg] nothing to do");
        } else {
            tracing::info!("[pkg] install: {}", install.join(", "));
        }

        Ok(())
    }
}
