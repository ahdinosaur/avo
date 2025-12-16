use async_trait::async_trait;
use lusid_fs::{self as fs, read_file_to_string, FsError};
use std::{
    fmt::Display,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use tokio::io::AsyncRead;
use tracing::info;

use crate::OperationType;

#[derive(Debug, Clone)]
pub enum FileSource {
    Contents(Vec<u8>),
    Path(FilePath),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FilePath(String);

impl FilePath {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl Display for FilePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMode(u32);

impl FileMode {
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

impl Display for FileMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:o}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileUser(String);

impl FileUser {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for FileUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileGroup(String);

impl FileGroup {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for FileGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub enum FileOperation {
    WriteFile {
        path: FilePath,
        source: FileSource,
    },
    CopyFile {
        source: FilePath,
        destination: FilePath,
    },
    MoveFile {
        source: FilePath,
        destination: FilePath,
    },
    RemoveFile {
        path: FilePath,
    },
    CreateDirectory {
        path: FilePath,
    },
    RemoveDirectory {
        path: FilePath,
    },
    CreateSymlink {
        path: FilePath,
        target: FilePath,
    },
    CreateHardLink {
        path: FilePath,
        target: FilePath,
    },
    ChangeMode {
        path: FilePath,
        mode: FileMode,
    },
    ChangeUser {
        path: FilePath,
        user: FileUser,
    },
    ChangeGroup {
        path: FilePath,
        group: FileGroup,
    },
}

impl Display for FileOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileOperation::WriteFile { path, source } => match source {
                FileSource::Contents(contents) => write!(
                    f,
                    "File::WriteFile(path = {}, source = Contents({} bytes))",
                    path,
                    contents.len()
                ),
                FileSource::Path(source_path) => write!(
                    f,
                    "File::WriteFile(path = {}, source = Path({}))",
                    path, source_path
                ),
            },
            FileOperation::CopyFile {
                source,
                destination,
            } => write!(
                f,
                "File::CopyFile(source = {}, destination = {})",
                source, destination
            ),
            FileOperation::MoveFile {
                source,
                destination,
            } => write!(
                f,
                "File::MoveFile(source = {}, destination = {})",
                source, destination
            ),
            FileOperation::RemoveFile { path } => write!(f, "File::RemoveFile(path = {})", path),
            FileOperation::CreateDirectory { path } => {
                write!(f, "File::CreateDirectory(path = {})", path)
            }
            FileOperation::RemoveDirectory { path } => {
                write!(f, "File::RemoveDirectory(path = {})", path)
            }
            FileOperation::CreateSymlink { path, target } => write!(
                f,
                "File::CreateSymlink(path = {}, target = {})",
                path, target
            ),
            FileOperation::CreateHardLink { path, target } => write!(
                f,
                "File::CreateHardLink(path = {}, target = {})",
                path, target
            ),
            FileOperation::ChangeMode { path, mode } => {
                write!(f, "File::ChangeMode(path = {}, mode = {})", path, mode)
            }
            FileOperation::ChangeUser { path, user } => {
                write!(f, "File::ChangeUser(path = {}, user = {})", path, user)
            }
            FileOperation::ChangeGroup { path, group } => {
                write!(f, "File::ChangeGroup(path = {}, group = {})", path, group)
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum FileApplyError {
    #[error(transparent)]
    Fs(#[from] FsError),
}

#[derive(Debug, Clone)]
pub struct File;

#[async_trait]
impl OperationType for File {
    type Operation = FileOperation;

    fn merge(operations: Vec<Self::Operation>) -> Vec<Self::Operation> {
        operations
    }

    type ApplyOutput = Pin<Box<dyn Future<Output = Result<(), Self::ApplyError>> + Send + 'static>>;
    type ApplyError = FileApplyError;

    type ApplyStdout = Pin<Box<dyn AsyncRead + Send + 'static>>;
    type ApplyStderr = Pin<Box<dyn AsyncRead + Send + 'static>>;

    async fn apply(
        operation: &Self::Operation,
    ) -> Result<(Self::ApplyOutput, Self::ApplyStdout, Self::ApplyStderr), Self::ApplyError> {
        let stdout = Box::pin(tokio::io::empty());
        let stderr = Box::pin(tokio::io::empty());

        match operation.clone() {
            FileOperation::WriteFile { path, source } => {
                info!("[file] write file: {}", path);
                Ok((
                    Box::pin(async move {
                        match source {
                            FileSource::Contents(contents) => {
                                fs::write_file_atomic(path.as_path(), &contents).await?;
                            }
                            FileSource::Path(source_path) => {
                                let source = read_file_to_string(source_path)
                                fs::write
                                write_file_from_path_atomic(path.as_path(), source_path.as_path())
                                    .await?;
                            }
                        }
                        Ok(())
                    }),
                    stdout,
                    stderr,
                ))
            }
            FileOperation::CopyFile {
                source,
                destination,
            } => {
                info!("[file] copy file: {} -> {}", source, destination);
                Ok((
                    Box::pin(async move {
                        copy_file_atomic(destination.as_path(), source.as_path()).await?;
                        Ok(())
                    }),
                    stdout,
                    stderr,
                ))
            }
            FileOperation::MoveFile {
                source,
                destination,
            } => {
                info!("[file] move file: {} -> {}", source, destination);
                Ok((
                    Box::pin(async move {
                        move_file(source.as_path(), destination.as_path()).await?;
                        Ok(())
                    }),
                    stdout,
                    stderr,
                ))
            }
            FileOperation::RemoveFile { path } => {
                info!("[file] remove file: {}", path);
                Ok((
                    Box::pin(async move {
                        remove_file_if_exists(path.as_path()).await?;
                        Ok(())
                    }),
                    stdout,
                    stderr,
                ))
            }
            FileOperation::CreateDirectory { path } => {
                info!("[file] create directory: {}", path);
                Ok((
                    Box::pin(async move {
                        fs::create_dir_all(path.as_path()).await?;
                        Ok(())
                    }),
                    stdout,
                    stderr,
                ))
            }
            FileOperation::RemoveDirectory { path } => {
                info!("[file] remove directory: {}", path);
                Ok((
                    Box::pin(async move {
                        remove_directory_if_exists(path.as_path()).await?;
                        Ok(())
                    }),
                    stdout,
                    stderr,
                ))
            }
            FileOperation::CreateSymlink { path, target } => {
                info!("[file] create symlink: {} -> {}", path, target);
                Ok((
                    Box::pin(async move {
                        create_symlink_idempotent(path.as_path(), target.as_path()).await?;
                        Ok(())
                    }),
                    stdout,
                    stderr,
                ))
            }
            FileOperation::CreateHardLink { path, target } => {
                info!("[file] create hard link: {} -> {}", path, target);
                Ok((
                    Box::pin(async move {
                        create_hard_link_idempotent(path.as_path(), target.as_path()).await?;
                        Ok(())
                    }),
                    stdout,
                    stderr,
                ))
            }
            FileOperation::ChangeMode { path, mode } => {
                info!("[file] change mode: {} -> {}", path, mode);
                Ok((
                    Box::pin(async move {
                        change_mode_idempotent(path.as_path(), mode).await?;
                        Ok(())
                    }),
                    stdout,
                    stderr,
                ))
            }
            FileOperation::ChangeUser { path, user } => {
                info!("[file] change user: {} -> {}", path, user);
                Ok((
                    Box::pin(async move {
                        change_user_idempotent(path.as_path(), user.as_str()).await?;
                        Ok(())
                    }),
                    stdout,
                    stderr,
                ))
            }
            FileOperation::ChangeGroup { path, group } => {
                info!("[file] change group: {} -> {}", path, group);
                Ok((
                    Box::pin(async move {
                        change_group_idempotent(path.as_path(), group.as_str()).await?;
                        Ok(())
                    }),
                    stdout,
                    stderr,
                ))
            }
        }
    }
}

async fn remove_file_if_exists(path: &Path) -> Result<(), io::Error> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

async fn remove_directory_if_exists(path: &Path) -> Result<(), io::Error> {
    match fs::remove_dir_all(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

async fn move_file(source: &Path, destination: &Path) -> Result<(), io::Error> {
    if fs::try_exists(destination).await? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("destination already exists: {}", destination.display()),
        ));
    }
    fs::rename(source, destination).await?;
    Ok(())
}

async fn write_file_from_bytes(
    destination_path: &Path,
    contents: &[u8],
) -> Result<(), FileApplyError> {
    if file_contents_are_equal_to_bytes(destination_path, contents).await? {
        return Ok(());
    }

    let temporary_path = temporary_path_for(destination_path)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary_path)
        .await?;

    file.write_all(contents).await?;
    file.flush().await?;

    drop(file);

    replace_file_atomically(&temporary_path, destination_path).await?;
    Ok(())
}

async fn write_file_from_path_atomic(
    destination_path: &Path,
    source_path: &Path,
) -> Result<(), FileApplyError> {
    if fs::try_exists(destination_path).await?
        && files_are_equal(destination_path, source_path).await?
    {
        return Ok(());
    }

    let temporary_path = temporary_path_for(destination_path)?;

    let mut source_file = fs::File::open(source_path).await?;
    let mut destination_file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary_path)
        .await?;

    tokio::io::copy(&mut source_file, &mut destination_file).await?;
    destination_file.flush().await?;

    drop(destination_file);

    replace_file_atomically(&temporary_path, destination_path).await?;
    Ok(())
}

async fn copy_file_atomic(
    destination_path: &Path,
    source_path: &Path,
) -> Result<(), FileApplyError> {
    write_file_from_path_atomic(destination_path, source_path).await
}

async fn replace_file_atomically(
    temporary_path: &Path,
    destination_path: &Path,
) -> Result<(), FileApplyError> {
    fs::rename(temporary_path, destination_path).await?;
    return Ok(());
}

async fn file_contents_are_equal_to_bytes(path: &Path, expected: &[u8]) -> Result<bool, io::Error> {
    if !fs::try_exists(path).await? {
        return Ok(false);
    }

    let metadata = fs::metadata(path).await?;
    if metadata.len() != expected.len() as u64 {
        return Ok(false);
    }

    let mut file = fs::File::open(path).await?;
    let mut offset = 0usize;
    let mut buffer = vec![0u8; 64 * 1024];

    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }

        let end_offset = offset + count;
        if end_offset > expected.len() {
            return Ok(false);
        }

        if buffer[..count] != expected[offset..end_offset] {
            return Ok(false);
        }

        offset = end_offset;
    }

    Ok(offset == expected.len())
}

async fn files_are_equal(left: &Path, right: &Path) -> Result<bool, io::Error> {
    let left_metadata = fs::metadata(left).await?;
    let right_metadata = fs::metadata(right).await?;

    if left_metadata.len() != right_metadata.len() {
        return Ok(false);
    }

    let mut left_file = fs::File::open(left).await?;
    let mut right_file = fs::File::open(right).await?;

    let mut left_buffer = vec![0u8; 64 * 1024];
    let mut right_buffer = vec![0u8; 64 * 1024];

    loop {
        let left_count = left_file.read(&mut left_buffer).await?;
        let right_count = right_file.read(&mut right_buffer).await?;

        if left_count != right_count {
            return Ok(false);
        }

        if left_count == 0 {
            break;
        }

        if left_buffer[..left_count] != right_buffer[..right_count] {
            return Ok(false);
        }
    }

    Ok(true)
}

async fn create_symlink_idempotent(path: &Path, target: &Path) -> Result<(), FileApplyError> {
    if fs::try_exists(path).await? {
        let link_metadata = fs::symlink_metadata(path).await?;

        if link_metadata.is_symlink() {
            let existing_target = fs::read_link(path).await?;
            if existing_target == target {
                return Ok(());
            }

            remove_file_if_exists(path).await?;
        } else {
            return Err(FileApplyError::InvalidPath(format!(
                "path exists and is not a symbolic link: {}",
                path.display()
            )));
        }
    }

    create_symlink(path, target).await?;
    Ok(())
}

async fn create_symlink(path: &Path, target: &Path) -> Result<(), FileApplyError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs as unix_file_system;

        let path = path.to_owned();
        let target = target.to_owned();

        tokio::task::spawn_blocking(move || {
            unix_file_system::symlink(&target, &path).map_err(FileApplyError::from)
        })
        .await
        .map_err(|join_error| {
            FileApplyError::LookupFailed(format!("symlink task failed: {}", join_error))
        })??;

        return Ok(());
    }

    #[cfg(not(unix))]
    {
        let _ = (path, target);
        Err(FileApplyError::UnsupportedPlatform(
            "CreateSymlink is only implemented for unix platforms",
        ))
    }
}

async fn create_hard_link_idempotent(path: &Path, target: &Path) -> Result<(), FileApplyError> {
    if fs::try_exists(path).await? {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            let existing_metadata = fs::metadata(path).await?;
            let target_metadata = fs::metadata(target).await?;

            if existing_metadata.ino() == target_metadata.ino()
                && existing_metadata.dev() == target_metadata.dev()
            {
                return Ok(());
            }
        }

        #[cfg(not(unix))]
        {
            return Err(FileApplyError::InvalidPath(format!(
                "hard link destination already exists: {}",
                path.display()
            )));
        }

        return Err(FileApplyError::InvalidPath(format!(
            "path exists but does not match hard link target: {}",
            path.display()
        )));
    }

    fs::hard_link(target, path).await?;
    Ok(())
}

async fn change_mode_idempotent(path: &Path, mode: FileMode) -> Result<(), FileApplyError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let metadata = fs::metadata(path).await?;
        let current_mode = metadata.permissions().mode() & 0o7777;
        let desired_mode = mode.as_u32() & 0o7777;

        if current_mode == desired_mode {
            return Ok(());
        }

        let permissions = std::fs::Permissions::from_mode(desired_mode);
        fs::set_permissions(path, permissions).await?;
        return Ok(());
    }

    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Err(FileApplyError::UnsupportedPlatform(
            "ChangeMode is only implemented for unix platforms",
        ))
    }
}

async fn change_user_idempotent(path: &Path, user: &str) -> Result<(), FileApplyError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let metadata = fs::metadata(path).await?;
        let current_user_identifier = metadata.uid();

        let desired_user_identifier = resolve_user_identifier(user).await?;
        if current_user_identifier == desired_user_identifier {
            return Ok(());
        }

        chown(path, Some(desired_user_identifier), None).await?;
        return Ok(());
    }

    #[cfg(not(unix))]
    {
        let _ = (path, user);
        Err(FileApplyError::UnsupportedPlatform(
            "ChangeUser is only implemented for unix platforms",
        ))
    }
}

async fn change_group_idempotent(path: &Path, group: &str) -> Result<(), FileApplyError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let metadata = fs::metadata(path).await?;
        let current_group_identifier = metadata.gid();

        let desired_group_identifier = resolve_group_identifier(group).await?;
        if current_group_identifier == desired_group_identifier {
            return Ok(());
        }

        chown(path, None, Some(desired_group_identifier)).await?;
        return Ok(());
    }

    #[cfg(not(unix))]
    {
        let _ = (path, group);
        Err(FileApplyError::UnsupportedPlatform(
            "ChangeGroup is only implemented for unix platforms",
        ))
    }
}

#[cfg(unix)]
async fn chown(
    path: &Path,
    user_identifier: Option<u32>,
    group_identifier: Option<u32>,
) -> Result<(), FileApplyError> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let path = path.to_owned();

    tokio::task::spawn_blocking(move || {
        let path = CString::new(path.as_os_str().as_bytes()).map_err(|error| {
            FileApplyError::InvalidPath(format!("invalid path bytes: {}", error))
        })?;

        let user_identifier = user_identifier
            .map(|value| value as libc::uid_t)
            .unwrap_or(!0 as libc::uid_t);

        let group_identifier = group_identifier
            .map(|value| value as libc::gid_t)
            .unwrap_or(!0 as libc::gid_t);

        let result = unsafe { libc::chown(path.as_ptr(), user_identifier, group_identifier) };
        if result != 0 {
            return Err(FileApplyError::InputOutput(io::Error::last_os_error()));
        }

        Ok(())
    })
    .await
    .map_err(|join_error| {
        FileApplyError::LookupFailed(format!("chown task failed: {}", join_error))
    })??;

    Ok(())
}

#[cfg(unix)]
async fn resolve_user_identifier(user: &str) -> Result<u32, FileApplyError> {
    if let Ok(value) = user.parse::<u32>() {
        return Ok(value);
    }

    let user = user.to_string();

    tokio::task::spawn_blocking(move || resolve_user_identifier_blocking(&user))
        .await
        .map_err(|join_error| {
            FileApplyError::LookupFailed(format!("user lookup task failed: {}", join_error))
        })?
}

#[cfg(unix)]
fn resolve_user_identifier_blocking(user: &str) -> Result<u32, FileApplyError> {
    use std::ffi::CString;

    let user = CString::new(user)
        .map_err(|error| FileApplyError::LookupFailed(format!("invalid user name: {}", error)))?;

    let mut password_entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();

    let buffer_size = 16 * 1024;
    let mut buffer = vec![0u8; buffer_size];

    let status = unsafe {
        libc::getpwnam_r(
            user.as_ptr(),
            &mut password_entry,
            buffer.as_mut_ptr() as *mut libc::c_char,
            buffer.len(),
            &mut result,
        )
    };

    if status != 0 {
        return Err(FileApplyError::LookupFailed(format!(
            "getpwnam_r failed: {}",
            io::Error::from_raw_os_error(status)
        )));
    }

    if result.is_null() {
        return Err(FileApplyError::LookupFailed(format!(
            "user not found: {}",
            user.to_string_lossy()
        )));
    }

    Ok(password_entry.pw_uid as u32)
}

#[cfg(unix)]
async fn resolve_group_identifier(group: &str) -> Result<u32, FileApplyError> {
    if let Ok(value) = group.parse::<u32>() {
        return Ok(value);
    }

    let group = group.to_string();

    tokio::task::spawn_blocking(move || resolve_group_identifier_blocking(&group))
        .await
        .map_err(|join_error| {
            FileApplyError::LookupFailed(format!("group lookup task failed: {}", join_error))
        })?
}

#[cfg(unix)]
fn resolve_group_identifier_blocking(group: &str) -> Result<u32, FileApplyError> {
    use std::ffi::CString;

    let group = CString::new(group)
        .map_err(|error| FileApplyError::LookupFailed(format!("invalid group name: {}", error)))?;

    let mut group_entry: libc::group = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::group = std::ptr::null_mut();

    let buffer_size = 16 * 1024;
    let mut buffer = vec![0u8; buffer_size];

    let status = unsafe {
        libc::getgrnam_r(
            group.as_ptr(),
            &mut group_entry,
            buffer.as_mut_ptr() as *mut libc::c_char,
            buffer.len(),
            &mut result,
        )
    };

    if status != 0 {
        return Err(FileApplyError::LookupFailed(format!(
            "getgrnam_r failed: {}",
            io::Error::from_raw_os_error(status)
        )));
    }

    if result.is_null() {
        return Err(FileApplyError::LookupFailed(format!(
            "group not found: {}",
            group.to_string_lossy()
        )));
    }

    Ok(group_entry.gr_gid as u32)
}
