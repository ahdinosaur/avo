use crate::{http::HttpClient, paths::Paths};

#[derive(Debug, Clone)]
pub struct Context {
    http_client: HttpClient,
    paths: Paths,
}

impl Context {
    pub fn new(http_client: HttpClient, paths: Paths) -> Self {
        Self { http_client, paths }
    }

    pub fn http_client(&mut self) -> &mut HttpClient {
        &mut self.http_client
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }
}
