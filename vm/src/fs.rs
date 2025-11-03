use std::path::{Path, PathBuf};
use std::process::Stdio;
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Error, Debug)]
pub enum FsError {
    #[error("Cannot create directory '{path}': {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to spawn copy from '{from}' to '{to}': {source}")]
    CopyDirSpawn {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed waiting for copy from '{from}' to '{to}': {source}")]
    CopyDirWait {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },

    #[error("Copy command returned non-zero status from '{from}' to '{to}'")]
    CopyDirStatus { from: PathBuf, to: PathBuf },

    #[error("Cannot read directory '{path}': {source}")]
    ReadDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Cannot iterate directory '{path}': {source}")]
    ReadDirEntry {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Cannot remove directory '{path}': {source}")]
    RemoveDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Cannot read directory metadata '{path}': {source}")]
    Metadata {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Cannot write directory '{path}' (read-only)")]
    ReadOnlyDir { path: PathBuf },

    #[error("Cannot create file '{path}': {source}")]
    CreateFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Cannot open file '{path}': {source}")]
    OpenFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Cannot determine if path exists '{path}': {source}")]
    PathExists {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Cannot write file '{path}': {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Cannot read file '{path}': {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Cannot rename file from '{from}' to '{to}': {source}")]
    RenameFile {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },

    #[error("Cannot delete file '{path}': {source}")]
    RemoveFile {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub async fn create_dir<P: AsRef<Path>>(path: P) -> Result<(), FsError> {
    let p = path.as_ref();
    fs::create_dir_all(p)
        .await
        .map_err(|source| FsError::CreateDir {
            path: p.to_path_buf(),
            source,
        })
}

pub async fn copy_dir<F: AsRef<Path>, T: AsRef<Path>>(from: F, to: T) -> Result<(), FsError> {
    let from_path = from.as_ref();
    let to_path = to.as_ref();
    let from_buf = from_path.to_path_buf();
    let to_buf = to_path.to_path_buf();

    let mut child = Command::new("cp")
        .arg("--recursive")
        .arg(from_path)
        .arg(to_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| FsError::CopyDirSpawn {
            from: from_buf.clone(),
            to: to_buf.clone(),
            source,
        })?;

    let status = child.wait().await.map_err(|source| FsError::CopyDirWait {
        from: from_buf.clone(),
        to: to_buf.clone(),
        source,
    })?;

    if status.success() {
        Ok(())
    } else {
        Err(FsError::CopyDirStatus {
            from: from_buf,
            to: to_buf,
        })
    }
}

pub async fn read_dir<P: AsRef<Path>>(path: P) -> Result<Vec<PathBuf>, FsError> {
    let p = path.as_ref();
    let mut dir = fs::read_dir(p).await.map_err(|source| FsError::ReadDir {
        path: p.to_path_buf(),
        source,
    })?;

    let mut entries = Vec::new();
    while let Some(entry) = dir
        .next_entry()
        .await
        .map_err(|source| FsError::ReadDirEntry {
            path: p.to_path_buf(),
            source,
        })?
    {
        entries.push(entry.path());
    }
    Ok(entries)
}

pub async fn remove_dir<P: AsRef<Path>>(path: P) -> Result<(), FsError> {
    let p = path.as_ref();
    fs::remove_dir_all(p)
        .await
        .map_err(|source| FsError::RemoveDir {
            path: p.to_path_buf(),
            source,
        })
}

pub async fn setup_directory_access<P: AsRef<Path>>(path: P) -> Result<(), FsError> {
    let p = path.as_ref();
    create_dir(p).await?;

    let permission = fs::metadata(p)
        .await
        .map_err(|source| FsError::Metadata {
            path: p.to_path_buf(),
            source,
        })?
        .permissions();

    if permission.readonly() {
        return Err(FsError::ReadOnlyDir {
            path: p.to_path_buf(),
        });
    }

    Ok(())
}

pub async fn create_file<P: AsRef<Path>>(path: P) -> Result<tokio::fs::File, FsError> {
    let p = path.as_ref();
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(p)
        .await
        .map_err(|source| FsError::CreateFile {
            path: p.to_path_buf(),
            source,
        })
}

pub async fn open_file<P: AsRef<Path>>(path: P) -> Result<tokio::fs::File, FsError> {
    let p = path.as_ref();
    fs::File::open(p).await.map_err(|source| FsError::OpenFile {
        path: p.to_path_buf(),
        source,
    })
}

pub async fn path_exists<P: AsRef<Path>>(path: P) -> Result<bool, FsError> {
    let p = path.as_ref();
    fs::try_exists(p)
        .await
        .map_err(|source| FsError::PathExists {
            path: p.to_path_buf(),
            source,
        })
}

pub async fn write_file<P: AsRef<Path>>(path: P, data: &[u8]) -> Result<(), FsError> {
    let p = path.as_ref();
    let mut file = create_file(p).await?;
    file.write_all(data)
        .await
        .map_err(|source| FsError::WriteFile {
            path: p.to_path_buf(),
            source,
        })
}

pub async fn read_file_to_string<P: AsRef<Path>>(path: P) -> Result<String, FsError> {
    let p = path.as_ref();
    fs::read_to_string(p)
        .await
        .map_err(|source| FsError::ReadFile {
            path: p.to_path_buf(),
            source,
        })
}

pub async fn rename_file<F: AsRef<Path>, T: AsRef<Path>>(from: F, to: T) -> Result<(), FsError> {
    let from_p = from.as_ref();
    let to_p = to.as_ref();
    fs::rename(from_p, to_p)
        .await
        .map_err(|source| FsError::RenameFile {
            from: from_p.to_path_buf(),
            to: to_p.to_path_buf(),
            source,
        })
}

pub async fn remove_file<P: AsRef<Path>>(path: P) -> Result<(), FsError> {
    let p = path.as_ref();
    fs::remove_file(p)
        .await
        .map_err(|source| FsError::RemoveFile {
            path: p.to_path_buf(),
            source,
        })
}
