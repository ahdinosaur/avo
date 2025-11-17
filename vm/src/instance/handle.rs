use std::{num::ParseIntError, path::PathBuf, str::FromStr};
use thiserror::Error;

use crate::{
    fs::{self, FsError},
    instance::InstancePaths,
};

pub struct InstanceHandle {
    instance_dir: PathBuf,
}

#[derive(Error, Debug)]
pub enum InstanceHandleError {
    #[error(transparent)]
    Fs(#[from] FsError),
}

impl InstanceHandle {
    pub fn new(instance_dir: PathBuf) -> Self {
        Self { instance_dir }
    }

    fn paths(&self) -> InstancePaths<'_> {
        InstancePaths::new(&self.instance_dir)
    }
}
