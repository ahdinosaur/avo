use async_trait::async_trait;
use lusid_cmd::{Command, CommandError};
use std::{collections::BTreeSet, fmt::Display, pin::Pin};
use thiserror::Error;
use tokio::process::{ChildStderr, ChildStdout};
use tracing::info;
use url::Url;

use crate::OperationType;

#[derive(Debug, Clone)]
pub enum FileSource {
    Contents(Vec<u8>),
    Path(FilePath),
}

#[derive(Debug, Clone)]
pub struct FilePath(String);
#[derive(Debug, Clone)]
pub struct FileMode(u32);
#[derive(Debug, Clone)]
pub struct FileUser(String);
#[derive(Debug, Clone)]
pub struct FileGroup(String);

#[derive(Debug, Clone)]
pub enum FileOperation {
    WriteFile { path: FilePath, source: FileSource },
    ChangeMode { path: FilePath, mode: FileMode },
    ChangeUser { path: FilePath, user: FileUser },
    ChangeGroup { path: FilePath, group: FileGroup },
    CreateDirectory { path: FilePath },
}

impl Display for FileOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileOperation::Update => write!(f, "File::Update"),
            FileOperation::Install { packages } => {
                write!(f, "File::Install(packages = [{}])", packages.join(", "))
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum FileApplyError {
    #[error(transparent)]
    Command(#[from] CommandError),
}

#[derive(Debug, Clone)]
pub struct File;

#[async_trait]
impl OperationType for File {
    type Operation = FileOperation;

    fn merge(operations: Vec<Self::Operation>) -> Vec<Self::Operation> {
        let mut update = false;
        let mut install: BTreeSet<String> = BTreeSet::new();

        for operation in operations {
            match operation {
                FileOperation::Update => {
                    update = true;
                }
                FileOperation::Install { packages } => {
                    for package in packages {
                        install.insert(package);
                    }
                }
            }
        }

        let mut operations = Vec::new();
        if update {
            operations.push(FileOperation::Update);
        }
        if !install.is_empty() {
            operations.push(FileOperation::Install {
                packages: install.into_iter().collect(),
            })
        }
        operations
    }

    type ApplyOutput = Pin<Box<dyn Future<Output = Result<(), Self::ApplyError>> + Send + 'static>>;
    type ApplyError = FileApplyError;
    type ApplyStdout = ChildStdout;
    type ApplyStderr = ChildStderr;

    async fn apply(
        operation: &Self::Operation,
    ) -> Result<(Self::ApplyOutput, Self::ApplyStdout, Self::ApplyStderr), Self::ApplyError> {
        match operation {
            FileOperation::Update => {
                info!("[apt] update");
                let mut cmd = Command::new("apt-get");
                cmd.env("DEBIAN_FRONTEND", "noninteractive").arg("update");
                let output = cmd.sudo().output().await?;
                Ok((
                    Box::pin(async move {
                        output.status.await?;
                        Ok(())
                    }),
                    output.stdout,
                    output.stderr,
                ))
            }
            FileOperation::Install { packages } => {
                info!("[apt] install: {}", packages.join(", "));
                let mut cmd = Command::new("apt-get");
                cmd.env("DEBIAN_FRONTEND", "noninteractive")
                    .arg("install")
                    .arg("-y")
                    .args(packages);
                let output = cmd.sudo().output().await?;
                Ok((
                    Box::pin(async move {
                        output.status.await?;
                        Ok(())
                    }),
                    output.stdout,
                    output.stderr,
                ))
            }
        }
    }
}
