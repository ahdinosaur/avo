use std::path::{Path, PathBuf};
use std::process::Stdio;
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub async fn create_dir(path: &str) -> Result<(), FsError> {
    if !Path::new(path).exists() {
        fs::create_dir_all(path)
            .await
            .map_err(|source| FsError::CreateDir {
                path: path.to_string(),
                source,
            })?;
    }

    Ok(())
}

pub async fn copy_dir(from: &str, to: &str) -> Result<(), FsError> {
    Command::new("cp")
        .arg("--recursive")
        .arg(from)
        .arg(to)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| FsError::CopyDirSpawn {
            from: from.to_string(),
            to: to.to_string(),
            source,
        })?
        .wait()
        .await
        .map_err(|source| FsError::CopyDirWait {
            from: from.to_string(),
            to: to.to_string(),
            source,
        })?
        .success()
        .then_some(())
        .ok_or_else(|| FsError::CopyDirStatus {
            from: from.to_string(),
            to: to.to_string(),
        })
}

pub async fn read_dir(path: &str) -> Result<Vec<PathBuf>, FsError> {
    let mut dir = fs::read_dir(path)
        .await
        .map_err(|source| FsError::ReadDir {
            path: path.to_string(),
            source,
        })?;

    let mut entries = Vec::new();
    while let Some(entry) = dir
        .next_entry()
        .await
        .map_err(|source| FsError::ReadDirEntry {
            path: path.to_string(),
            source,
        })?
    {
        entries.push(entry.path());
    }
    Ok(entries)
}

pub async fn remove_dir(path: &str) -> Result<(), FsError> {
    fs::remove_dir_all(path)
        .await
        .map_err(|source| FsError::RemoveDir {
            path: path.to_string(),
            source,
        })
}

pub async fn setup_directory_access(path: &str) -> Result<(), FsError> {
    create_dir(path).await?;

    let permission = fs::metadata(path)
        .await
        .map_err(|source| FsError::Metadata {
            path: path.to_string(),
            source,
        })?
        .permissions();

    if permission.readonly() {
        return Err(FsError::ReadOnlyDir {
            path: path.to_string(),
        });
    }

    Ok(())
}

pub async fn create_file(path: &str) -> Result<tokio::fs::File, FsError> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .await
        .map_err(|source| FsError::CreateFile {
            path: path.to_string(),
            source,
        })
}

pub async fn open_file(path: &str) -> Result<tokio::fs::File, FsError> {
    fs::File::open(path)
        .await
        .map_err(|source| FsError::OpenFile {
            path: path.to_string(),
            source,
        })
}

pub async fn path_exists(path: &str) -> Result<bool, FsError> {
    fs::try_exists(path)
        .await
        .map_err(|source| FsError::PathExists {
            path: path.to_string(),
            source,
        })
}

pub async fn write_file(path: &str, data: &[u8]) -> Result<(), FsError> {
    let mut file = create_file(path).await?;
    file.write_all(data)
        .await
        .map_err(|source| FsError::WriteFile {
            path: path.to_string(),
            source,
        })
}

pub async fn read_file_to_string(path: &str) -> Result<String, FsError> {
    fs::read_to_string(path)
        .await
        .map_err(|source| FsError::ReadFile {
            path: path.to_string(),
            source,
        })
}

pub async fn rename_file(from: &str, to: &str) -> Result<(), FsError> {
    fs::rename(from, to)
        .await
        .map_err(|source| FsError::RenameFile {
            from: from.to_string(),
            to: to.to_string(),
            source,
        })
}

pub async fn remove_file(path: &str) -> Result<(), FsError> {
    fs::remove_file(path)
        .await
        .map_err(|source| FsError::RemoveFile {
            path: path.to_string(),
            source,
        })
}

#[derive(Error, Debug)]
pub enum FsError {
    #[error("Cannot create directory '{path}': {source}")]
    CreateDir {
        path: String,
        source: std::io::Error,
    },

    #[error("Failed to spawn copy from '{from}' to '{to}': {source}")]
    CopyDirSpawn {
        from: String,
        to: String,
        source: std::io::Error,
    },

    #[error("Failed waiting for copy from '{from}' to '{to}': {source}")]
    CopyDirWait {
        from: String,
        to: String,
        source: std::io::Error,
    },

    #[error("Copy command returned non-zero status from '{from}' to '{to}'")]
    CopyDirStatus { from: String, to: String },

    #[error("Cannot read directory '{path}': {source}")]
    ReadDir {
        path: String,
        source: std::io::Error,
    },

    #[error("Cannot iterate directory '{path}': {source}")]
    ReadDirEntry {
        path: String,
        source: std::io::Error,
    },

    #[error("Cannot remove directory '{path}': {source}")]
    RemoveDir {
        path: String,
        source: std::io::Error,
    },

    #[error("Cannot read directory metadata '{path}': {source}")]
    Metadata {
        path: String,
        source: std::io::Error,
    },

    #[error("Cannot write directory '{path}' (read-only)")]
    ReadOnlyDir { path: String },

    #[error("Cannot create file '{path}': {source}")]
    CreateFile {
        path: String,
        source: std::io::Error,
    },

    #[error("Cannot open file '{path}': {source}")]
    OpenFile {
        path: String,
        source: std::io::Error,
    },

    #[error("Cannot determine if path exists '{path}': {source}")]
    PathExists {
        path: String,
        source: std::io::Error,
    },

    #[error("Cannot write file '{path}': {source}")]
    WriteFile {
        path: String,
        source: std::io::Error,
    },

    #[error("Cannot read file '{path}': {source}")]
    ReadFile {
        path: String,
        source: std::io::Error,
    },

    #[error("Cannot rename file from '{from}' to '{to}': {source}")]
    RenameFile {
        from: String,
        to: String,
        source: std::io::Error,
    },

    #[error("Cannot delete file '{path}': {source}")]
    RemoveFile {
        path: String,
        source: std::io::Error,
    },
}
