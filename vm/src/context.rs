use ludis_env::{Environment, EnvironmentError};
use thiserror::Error;

use crate::{
    http::{HttpClient, HttpError},
    paths::{ExecutablePaths, ExecutablePathsError, Paths},
};

#[derive(Error, Debug)]
pub enum ContextError {
    #[error(transparent)]
    Http(#[from] HttpError),

    #[error(transparent)]
    Env(#[from] EnvironmentError),

    #[error(transparent)]
    ExecutablePaths(#[from] ExecutablePathsError),
}

#[derive(Debug, Clone)]
pub struct Context {
    http_client: HttpClient,
    paths: Paths,
    executables: ExecutablePaths,
}

impl Context {
    pub fn new() -> Result<Self, ContextError> {
        let http_client = HttpClient::new()?;
        let env = Environment::create()?;
        let paths = Paths::new(env);
        let executables = ExecutablePaths::new()?;

        Ok(Self {
            http_client,
            paths,
            executables,
        })
    }

    pub fn http_client(&mut self) -> &mut HttpClient {
        &mut self.http_client
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn executables(&self) -> &ExecutablePaths {
        &self.executables
    }
}
