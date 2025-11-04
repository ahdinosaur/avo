// See https://github.com/cubic-vm/cubic/blob/main/src/image/image_factory.rs

use std::{collections::HashMap, path::Path};

use avo_machine::Machine;
use avo_system::{Arch, Os};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::AsyncReadExt;

use crate::{
    context::Context,
    fs::{self, FsError},
    http::HttpError,
};

#[derive(Error, Debug)]
pub enum ImageError {
    #[error("Failed to load image cache: {0}")]
    CacheLoad(#[from] toml::de::Error),

    #[error(transparent)]
    Hash(#[from] ImageHashError),

    #[error(transparent)]
    Http(#[from] HttpError),

    #[error(transparent)]
    Fs(#[from] FsError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageIndex {
    pub arch: Arch,
    pub os: Os,
    pub image: ImageRef,
    pub hash: ImageHashRef,
}

impl ImageIndex {
    pub fn to_image_file_name(&self) -> String {
        let arch = &self.arch;
        let os = &self.os;
        let ext = self.image.to_extension();
        format!("{arch}_{os}.{ext}")
    }
    pub fn to_hash_file_name(&self) -> String {
        let arch = &self.arch;
        let os = &self.os;
        let ext = self.hash.to_extension();
        format!("{arch}_{os}.{ext}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImageRef {
    #[serde(rename = "qcow2")]
    Qcow2 { url: String },
}

impl ImageRef {
    fn to_url(&self) -> &str {
        match self {
            ImageRef::Qcow2 { url } => url,
        }
    }
    fn to_extension(&self) -> &str {
        match self {
            ImageRef::Qcow2 { url: _ } => "qcow2",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImageHashRef {
    #[serde(rename = "sha512sums")]
    Sha512Sums { url: String },
}

impl ImageHashRef {
    fn to_url(&self) -> &str {
        match self {
            ImageHashRef::Sha512Sums { url } => url,
        }
    }
    fn to_extension(&self) -> &str {
        match self {
            ImageHashRef::Sha512Sums { url: _ } => "sha512sums",
        }
    }
}

#[derive(Error, Debug)]
pub enum ImageHashError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error("malformed file name from '{url}'")]
    MalformedFileName { url: String },

    #[error("hash sums missing entry for image '{name}'")]
    HashNotFound { name: String },

    #[error("malformed sha512sums line {line_index}: '{line}'")]
    MalformedLine { line_index: usize, line: String },

    #[error("sha512 mismatch for '{name}': expected {expected}, actual {actual}")]
    HashMismatch {
        name: String,
        expected: String,
        actual: String,
    },
}

#[derive(Debug, Clone)]
pub enum ImageHash<'a> {
    Sha512Sums { path: &'a Path },
}

impl<'a> ImageHash<'a> {
    fn new(hash_ref: &ImageHashRef, path: &'a Path) -> Self {
        match hash_ref {
            ImageHashRef::Sha512Sums { url: _ } => ImageHash::Sha512Sums { path },
        }
    }

    async fn validate(
        &self,
        image_index: &ImageIndex,
        image_path: &Path,
    ) -> Result<(), ImageHashError> {
        async fn sha512_file_hex<P: AsRef<Path>>(path: P) -> Result<String, FsError> {
            use sha2::{Digest, Sha512};

            let p = path.as_ref();
            let mut file = fs::open_file(p).await?;
            let mut hasher = Sha512::new();
            let mut buf = [0u8; 8192];

            loop {
                let n = file
                    .read(&mut buf)
                    .await
                    .map_err(|source| FsError::ReadFile {
                        path: p.to_path_buf(),
                        source,
                    })?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }

            let digest = hasher.finalize();
            let mut hex = String::with_capacity(digest.len() * 2);
            for b in digest {
                hex.push_str(&format!("{:02x}", b));
            }
            Ok(hex)
        }

        /// Parse the contents of a Debian-style sha512sums file and return the hash that
        /// corresponds to `image_name`.
        ///
        /// Accepts lines like:
        /// <128-hex> [space][space or more][optional '*']<filename>
        /// Ignores empty lines and lines starting with '#'.
        fn lookup_sha512_for(sums: &str, image_name: &str) -> Result<String, ImageHashError> {
            for (idx, raw_line) in sums.lines().enumerate() {
                let line = raw_line.trim_end_matches('\r').trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                // Split into two parts: hash and the rest (file name). Using splitn to
                // avoid splitting file names that might (rarely) contain spaces.
                let (hash, name_part) = if let Some((h, rest)) = split_once_whitespace(line) {
                    (h, rest)
                } else {
                    return Err(ImageHashError::MalformedLine {
                        line_index: idx + 1,
                        line: raw_line.to_string(),
                    });
                };

                // Normalize filename token: handle optional leading '*' (binary mode).
                let listed_name = name_part.trim_start_matches('*');

                // Some sums may include paths. Compare only the file name component.
                let listed_basename = std::path::Path::new(listed_name)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(listed_name);

                // Validate hash shape: 128 hex chars
                if hash.len() != 128 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err(ImageHashError::MalformedLine {
                        line_index: idx + 1,
                        line: raw_line.to_string(),
                    });
                }

                if listed_basename == image_name {
                    return Ok(hash.to_ascii_lowercase());
                }
            }

            Err(ImageHashError::HashNotFound {
                name: image_name.to_string(),
            })
        }

        /// Split `s` into two parts at the first run of ASCII whitespace:
        /// (left, right-without-leading-whitespace).
        fn split_once_whitespace(s: &str) -> Option<(&str, &str)> {
            let bytes = s.as_bytes().iter().enumerate();
            for (i, b) in bytes {
                if b.is_ascii_whitespace() {
                    // Skip all following whitespace to get start of right part
                    let mut j = i;
                    let sb = s.as_bytes();
                    while j < sb.len() && sb[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    return Some((&s[..i], &s[j..]));
                }
            }
            None
        }

        match self {
            ImageHash::Sha512Sums { path } => {
                let sums = fs::read_file_to_string(path).await?;

                // Resolve the target name we need to look up in sums
                let image_url = image_index.image.to_url();
                let image_name = image_url.split('/').next_back().ok_or_else(|| {
                    ImageHashError::MalformedFileName {
                        url: image_url.to_string(),
                    }
                })?;

                // Find expected hash for this image in the sums
                let expected = lookup_sha512_for(&sums, image_name)?;

                // Compute actual hash of the image
                let actual = sha512_file_hex(&image_path).await?;

                // Compare (case-insensitive to be safe)
                if expected.eq_ignore_ascii_case(&actual) {
                    Ok(())
                } else {
                    Err(ImageHashError::HashMismatch {
                        name: image_name.to_string(),
                        expected,
                        actual,
                    })
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImagesList(HashMap<String, ImageIndex>);

impl ImagesList {
    fn into_values(self) -> impl Iterator<Item = ImageIndex> {
        self.0.into_values()
    }
}

pub async fn get_images_list() -> Result<ImagesList, ImageError> {
    let images_str = include_str!("../images.toml");
    let images_list: ImagesList = toml::from_str(images_str)?;
    Ok(images_list)
}

pub async fn get_image_for_machine(machine: Machine) -> Result<Option<ImageIndex>, ImageError> {
    let images_list = get_images_list().await?;
    let image_index = images_list
        .into_values()
        .find(|image_index| image_index.os == machine.os && image_index.arch == machine.arch);
    Ok(image_index)
}

pub async fn fetch_image(mut ctx: Context, image_index: ImageIndex) -> Result<(), ImageError> {
    let image_path = ctx.paths().image_file(&image_index.to_image_file_name());

    fs::setup_directory_access(ctx.paths().images_dir()).await?;

    ctx.http_client()
        .download_file(image_index.image.to_url(), &image_path)
        .await?;

    let hash_path = ctx.paths().image_file(&image_index.to_hash_file_name());

    ctx.http_client()
        .download_file(image_index.hash.to_url(), &hash_path)
        .await?;

    let hash = ImageHash::new(&image_index.hash, &hash_path);
    hash.validate(&image_index, &image_path).await?;

    Ok(())
}
