use async_trait::async_trait;
use indexmap::indexmap;
use ludis_params::{ParamField, ParamType, ParamTypes};
use rimu::{SourceId, Span, Spanned};
use serde::Deserialize;

use crate::{
    traits::{OperationAtomTrait, OperationDeltaTrait, OperationTrait},
    Operation, OperationKind,
};

/// Install packages.
#[derive(Debug, Clone)]
pub struct PackageOperation {
    packages: Vec<String>,
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

impl OperationTrait for PackageOperation {
    fn kind() -> OperationKind {
        OperationKind::Package
    }

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

    fn new(params: Self::Params) -> Self {
        match params {
            PackageParams::Package { package } => Self {
                packages: vec![package],
            },
            PackageParams::Packages { packages } => Self { packages },
        }
    }

    type Atom = PackageOperationAtom;

    fn atoms(ops: impl IntoIterator<Item = Self>) -> Vec<Self::Atom> {
        let mut atoms = Vec::new();
        for op in ops {
            for pkg in op.packages {
                atoms.push(PackageOperationAtom { package: pkg });
            }
        }
        atoms
    }
}

/// Single-package atom.
#[derive(Debug, Clone)]
pub struct PackageOperationAtom {
    package: String,
}

impl OperationAtomTrait for PackageOperationAtom {
    type Delta = PackageOperationDelta;

    fn delta(&self) -> Option<Self::Delta> {
        // TODO: Inspect real system state (e.g., dpkg/rpm) to decide if the package
        //       is already installed. For now, always produce a delta.
        Some(PackageOperationDelta {
            packages: vec![self.package.clone()],
        })
    }
}

/// Per-package delta (will be batched across the epoch).
#[derive(Debug, Clone)]
pub struct PackageOperationDelta {
    packages: Vec<String>,
}

#[async_trait]
impl OperationDeltaTrait for PackageOperationDelta {
    type Error = ();

    async fn apply(deltas: Vec<Self>) -> Result<(), Self::Error> {
        // Merge all package lists into one batched install.
        let mut packages: Vec<String> = Vec::new();
        for d in deltas {
            packages.extend(d.packages);
        }
        packages.sort();
        packages.dedup();

        if packages.is_empty() {
            tracing::info!("[pkg] nothing to do");
        } else {
            tracing::info!("[pkg] install: {}", packages.join(", "));
        }
        Ok(())
        // Real impl would invoke the package manager once here.
    }
}
