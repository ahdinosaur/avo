use lusid_fs::{self as fs, FsError};
use reqwest::Client;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;

const REQUEST_TIMEOUT_SEC: u64 = 10;

#[derive(Error, Debug)]
pub enum HttpError {
    #[error("Failed to build HTTP client: {0}")]
    BuildClient(#[source] reqwest::Error),

    #[error("HTTP request error: {0}")]
    Request(#[source] reqwest::Error),

    #[error("HTTP stream error: {0}")]
    Stream(#[source] reqwest::Error),

    #[error("File write error for '{path}': {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error(transparent)]
    Fs(#[from] FsError),
}

#[derive(Debug, Clone)]
pub struct HttpClient {
    client: Client,
}

impl HttpClient {
    pub fn new() -> Result<Self, HttpError> {
        let client = Client::builder()
            .read_timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SEC))
            .gzip(true)
            .brotli(true)
            .build()
            .map_err(HttpError::BuildClient)?;
        Ok(HttpClient { client })
    }

    #[allow(dead_code)]
    pub async fn get_file_size(&self, url: &str) -> Result<Option<u64>, HttpError> {
        let resp = self
            .client
            .head(url)
            .send()
            .await
            .map_err(HttpError::Request)?;
        let size = resp
            .headers()
            .get("Content-Length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        Ok(size)
    }

    pub async fn download_file<P: AsRef<Path>>(
        &self,
        url: &str,
        file_path: P,
    ) -> Result<(), HttpError> {
        let file_path = file_path.as_ref();

        if fs::path_exists(file_path).await? {
            return Ok(());
        }

        let mut temp_file = PathBuf::from(file_path);
        temp_file.add_extension("tmp");

        if fs::path_exists(&temp_file).await? {
            fs::remove_file(&temp_file).await?;
        }

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(HttpError::Request)?;

        let mut file = fs::create_file(&temp_file).await?;

        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(HttpError::Stream)?;
            file.write_all(&bytes)
                .await
                .map_err(|source| HttpError::Write {
                    path: temp_file.clone(),
                    source,
                })?;
        }

        // Ensure all data is flushed before renaming
        file.flush().await.map_err(|source| HttpError::Write {
            path: temp_file.clone(),
            source,
        })?;

        fs::rename_file(&temp_file, file_path).await?;

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn download_content(&self, url: &str) -> Result<String, HttpError> {
        self.client
            .get(url)
            .send()
            .await
            .map_err(HttpError::Request)?
            .text()
            .await
            .map_err(HttpError::Request)
    }
}
