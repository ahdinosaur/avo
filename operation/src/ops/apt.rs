use async_trait::async_trait;
use std::collections::BTreeSet;

use tracing::info;

use crate::OperationType;

#[derive(Debug, Clone)]
pub enum AptOperation {
    Install { packages: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct AptOperationType;

#[async_trait]
impl OperationType for AptOperationType {
    type Operation = AptOperation;
    type ApplyError = ();

    fn merge(ops: Vec<Self::Operation>) -> Vec<Self::Operation> {
        // Merge all Install ops into a single Install op with unique sorted packages.
        let mut install: BTreeSet<String> = BTreeSet::new();

        for op in ops {
            match op {
                AptOperation::Install { packages } => {
                    for p in packages {
                        install.insert(p);
                    }
                }
            }
        }

        if install.is_empty() {
            Vec::new()
        } else {
            vec![AptOperation::Install {
                packages: install.into_iter().collect(),
            }]
        }
    }

    async fn apply(ops: Vec<Self::Operation>) -> Result<(), Self::ApplyError> {
        for op in ops {
            match op {
                AptOperation::Install { packages } => {
                    if packages.is_empty() {
                        info!("[apt] nothing to install");
                    } else {
                        info!("[apt] install: {}", packages.join(", "));
                        // Real world: run apt-get install -y <packages> (privilege required)
                        // For now, we just log.
                    }
                }
            }
        }
        Ok(())
    }
}
