// Inspiration: https://github.com/cubic-vm/cubic/blob/68566f79d72e2037bce1b75246d92e6da7b999e5/src/env/environment_factory.rs

use std::{
    env::{self, VarError},
    path::{Path, PathBuf},
};

use thiserror::Error;

const PROJECT_NAME: &str = "ludis";

#[derive(Debug, Clone)]
pub struct Environment {
    data_dir: PathBuf,
    cache_dir: PathBuf,
    runtime_dir: PathBuf,
}

#[derive(Error, Debug, Clone)]
pub enum EnvironmentError {
    #[error(transparent)]
    Var(#[from] VarError),
}

impl Environment {
    pub fn new(data_dir: PathBuf, cache_dir: PathBuf, runtime_dir: PathBuf) -> Self {
        Self {
            data_dir,
            cache_dir,
            runtime_dir,
        }
    }

    #[cfg(target_os = "linux")]
    pub fn create() -> Result<Environment, EnvironmentError> {
        let data_dirs: PathBuf = Self::var("XDG_DATA_HOME")
            .or_else(|_| Self::var("HOME").map(|home| format!("{home}/.local/share")))
            .map(From::from)?;

        let cache_dirs: PathBuf = Self::var("XDG_CACHE_HOME")
            .or_else(|_| Self::var("HOME").map(|home| format!("{home}/.cache")))
            .map(From::from)?;

        let runtime_dirs: PathBuf = Self::var("XDG_RUNTIME_DIR")
            .or_else(|_| Self::var("UID").map(|uid| format!("/run/user/{uid}")))
            .map(From::from)?;

        Ok(Environment::new(
            data_dirs.join(PROJECT_NAME),
            cache_dirs.join(PROJECT_NAME),
            runtime_dirs.join(PROJECT_NAME),
        ))
    }

    #[cfg(target_os = "macos")]
    pub fn create() -> Result<Environment, EnvironmentError> {
        let home_dir: PathBuf = Self::var("HOME").map(From::from)?;

        Ok(Environment::new(
            home_dir.join("Library").join(PROJECT_NAME),
            home_dir.join("Library").join("Caches").join(PROJECT_NAME),
            home_dir.join("Library").join("Caches").join(PROJECT_NAME),
        ))
    }
    #[cfg(target_os = "windows")]
    pub fn create() -> Result<Environment, EnvironmentError> {
        let local_app_data_dir: PathBuf = Self::var("LOCALAPPDATA").map(From::from)?;
        let temp_dir: PathBuf = Self::var("TEMP").map(From::from)?;

        Ok(Environment::new(
            local_app_data_dir.join(PROJECT_NAME),
            temp_dir.join(PROJECT_NAME),
            temp_dir.join(PROJECT_NAME),
        ))
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    fn var(var: &str) -> Result<String, EnvironmentError> {
        env::var(var).map_err(From::from)
    }
}
