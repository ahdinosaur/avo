mod paths;

use ludis_http::{HttpClient, HttpError};
use thiserror::Error;

pub use crate::paths::{Paths, PathsError};

#[derive(Error, Debug)]
pub enum ContextError {
    #[error(transparent)]
    Paths(#[from] PathsError),

    #[error(transparent)]
    Http(#[from] HttpError),
}

#[derive(Debug, Clone)]
pub struct Context {
    paths: Paths,
    http: HttpClient,
}

impl Context {
    pub fn create() -> Result<Self, ContextError> {
        let paths = Paths::create()?;
        let http = HttpClient::new()?;
        Ok(Self { paths, http })
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn http_client(&mut self) -> &mut HttpClient {
        &mut self.http
    }
}
