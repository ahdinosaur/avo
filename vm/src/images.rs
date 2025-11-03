// See https://github.com/cubic-vm/cubic/blob/main/src/image/image_factory.rs

use std::{collections::HashMap, io::Read, sync::LazyLock};

use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    env::Environment,
    fs::{self, FsError},
    http::{HttpClient, HttpError},
    system::Arch,
};

#[derive(Error, Debug)]
pub enum ImageError {
    #[error(transparent)]
    Fs(#[from] FsError),

    #[error(transparent)]
    Http(#[from] HttpError),

    #[error("Failed to deserialize image cache: {0}")]
    CacheDeserialize(#[from] toml::de::Error),

    #[error("Failed to serialize image cache: {0}")]
    CacheSerialize(#[from] toml::ser::Error),

    #[error("No images were discovered while fetching")]
    NoImagesFound,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Image {
    pub vendor: String,
    pub codename: String,
    pub version: String,
    pub arch: Arch,
    pub url: String,
    pub size: Option<u64>,
}

impl Image {
    pub fn to_name(&self) -> String {
        format!("{}:{}:{}", self.vendor, self.version, self.arch)
    }

    pub fn to_file_name(&self) -> String {
        format!("{}_{}_{}", self.vendor, self.codename, self.arch)
    }
}

#[derive(Debug, PartialEq, Clone, Default, Serialize, Deserialize)]
pub struct ImageCache {
    images: Vec<Image>,
    timestamp: u64,
}

impl ImageCache {
    pub fn deserialize(input: &str) -> Result<ImageCache, ImageError> {
        Ok(toml::from_str(input)?)
    }

    pub fn serialize(&self) -> Result<String, ImageError> {
        Ok(toml::to_string_pretty(self)?)
    }
}

struct ImageLocation {
    url: &'static str,
    pattern: LazyLock<Regex>,
    download_url: &'static str,
}

struct Distro {
    vendor: &'static str,
    name_pattern: &'static str,
    version_pattern: &'static str,
    overview_url: &'static str,
    overview_pattern: LazyLock<Regex>,
    images: HashMap<Arch, ImageLocation>,
}

static DISTROS: LazyLock<Vec<Distro>> = LazyLock::new(|| {
    vec![
        Distro {
            vendor: "archlinux",
            name_pattern: "(name)",
            version_pattern: "(name)",
            overview_url: "https://geo.mirror.pkgbuild.com/images/",
            overview_pattern: LazyLock::new(|| Regex::new(r">([a-z]+)/<").unwrap()),
            images: HashMap::from([
                (
                    Arch::X86_64,
                    ImageLocation {
                        url: "https://geo.mirror.pkgbuild.com/images/latest/",
                        pattern: LazyLock::new(|| {
                            Regex::new(r">(Arch-Linux-x86_64-cloudimg.qcow2)<").unwrap()
                        }),
                        download_url: "https://geo.mirror.pkgbuild.com/images/(name)/Arch-Linux-x86_64-cloudimg.qcow2",
                    },
                ),
                (
                    Arch::Aarch64,
                    ImageLocation {
                        url: "https://geo.mirror.pkgbuild.com/images/latest/",
                        pattern: LazyLock::new(|| {
                            Regex::new(r">(Arch-Linux-arm64-cloudimg.qcow2)<").unwrap()
                        }),
                        download_url: "https://geo.mirror.pkgbuild.com/images/(name)/Arch-Linux-arm64-cloudimg.qcow2",
                    },
                ),
            ]),
        },
        Distro {
            vendor: "debian",
            name_pattern: "(name)",
            version_pattern: "(version)",
            overview_url: "https://cloud.debian.org/images/cloud/",
            overview_pattern: LazyLock::new(|| Regex::new(r">([a-z]+)/<").unwrap()),
            images: HashMap::from([
                (
                    Arch::X86_64,
                    ImageLocation {
                        url: "https://cloud.debian.org/images/cloud/(name)/latest/",
                        pattern: LazyLock::new(|| {
                            Regex::new(r">debian-([0-9]+)-generic-amd64.qcow2<").unwrap()
                        }),
                        download_url: "https://cloud.debian.org/images/cloud/(name)/latest/debian-(version)-generic-amd64.qcow2",
                    },
                ),
                (
                    Arch::Aarch64,
                    ImageLocation {
                        url: "https://cloud.debian.org/images/cloud/(name)/latest/",
                        pattern: LazyLock::new(|| {
                            Regex::new(r">debian-([0-9]+)-generic-arm64.qcow2<").unwrap()
                        }),
                        download_url: "https://cloud.debian.org/images/cloud/(name)/latest/debian-(version)-generic-arm64.qcow2",
                    },
                ),
            ]),
        },
        Distro {
            vendor: "fedora",
            name_pattern: "(name)",
            version_pattern: "(name)",
            overview_url: "https://download.fedoraproject.org/pub/fedora/linux/releases/",
            overview_pattern: LazyLock::new(|| Regex::new(r">([4-9][0-9]+)/<").unwrap()),
            images: HashMap::from([
                (
                    Arch::X86_64,
                    ImageLocation {
                        url: "https://download.fedoraproject.org/pub/fedora/linux/releases/(name)/Cloud/x86_64/images/",
                        pattern: LazyLock::new(|| {
                            Regex::new(
                                r"Fedora-Cloud-Base-Generic-([0-9]+-[0-9]+.[0-9]+).x86_64.qcow2",
                            )
                            .unwrap()
                        }),
                        download_url: "https://download.fedoraproject.org/pub/fedora/linux/releases/(name)/Cloud/x86_64/images/Fedora-Cloud-Base-Generic-(version).x86_64.qcow2",
                    },
                ),
                (
                    Arch::Aarch64,
                    ImageLocation {
                        url: "https://download.fedoraproject.org/pub/fedora/linux/releases/(name)/Cloud/aarch64/images/",
                        pattern: LazyLock::new(|| {
                            Regex::new(
                                r"Fedora-Cloud-Base-Generic-([0-9]+-[0-9]+.[0-9]+).aarch64.qcow2",
                            )
                            .unwrap()
                        }),
                        download_url: "https://download.fedoraproject.org/pub/fedora/linux/releases/(name)/Cloud/aarch64/images/Fedora-Cloud-Base-Generic-(version).aarch64.qcow2",
                    },
                ),
            ]),
        },
        Distro {
            vendor: "opensuse",
            name_pattern: "(name)",
            version_pattern: "(name)",
            overview_url: "https://download.opensuse.org/repositories/Cloud:/Images:/",
            overview_pattern: LazyLock::new(|| Regex::new(r">Leap_([0-9]+\.[0-9]+)/<").unwrap()),
            images: HashMap::from([
                (
                    Arch::X86_64,
                    ImageLocation {
                        url: "https://download.opensuse.org/repositories/Cloud:/Images:/Leap_15.6/images/",
                        pattern: LazyLock::new(|| {
                            Regex::new(r">(openSUSE-Leap-[0-9]+.[0-9]+.x86_64-NoCloud.qcow2)<")
                                .unwrap()
                        }),
                        download_url: "https://download.opensuse.org/repositories/Cloud:/Images:/Leap_15.6/images/(version)",
                    },
                ),
                (
                    Arch::Aarch64,
                    ImageLocation {
                        url: "https://download.opensuse.org/repositories/Cloud:/Images:/Leap_15.6/images/",
                        pattern: LazyLock::new(|| {
                            Regex::new(r">(openSUSE-Leap-[0-9]+.[0-9]+.aarch64-NoCloud.qcow2)<")
                                .unwrap()
                        }),
                        download_url: "https://download.opensuse.org/repositories/Cloud:/Images:/Leap_15.6/images/(version)",
                    },
                ),
            ]),
        },
        Distro {
            vendor: "ubuntu",
            name_pattern: "(name)",
            version_pattern: "(version)",
            overview_url: "https://cloud-images.ubuntu.com/minimal/releases/",
            overview_pattern: LazyLock::new(|| Regex::new(r">([a-z]+)/<").unwrap()),
            images: HashMap::from([
                (
                    Arch::X86_64,
                    ImageLocation {
                        url: "https://cloud-images.ubuntu.com/minimal/releases/(name)/release/",
                        pattern: LazyLock::new(|| {
                            Regex::new(r">ubuntu-([0-9]+\.[0-9]+)-minimal-cloudimg-amd64.img<")
                                .unwrap()
                        }),
                        download_url: "https://cloud-images.ubuntu.com/minimal/releases/(name)/release/ubuntu-(version)-minimal-cloudimg-amd64.img",
                    },
                ),
                (
                    Arch::Aarch64,
                    ImageLocation {
                        url: "https://cloud-images.ubuntu.com/minimal/releases/(name)/release/",
                        pattern: LazyLock::new(|| {
                            Regex::new(r">ubuntu-([0-9]+\.[0-9]+)-minimal-cloudimg-arm64.img<")
                                .unwrap()
                        }),
                        download_url: "https://cloud-images.ubuntu.com/minimal/releases/(name)/release/ubuntu-(version)-minimal-cloudimg-arm64.img",
                    },
                ),
            ]),
        },
    ]
});

async fn get_http_content_matches(
    web: &mut HttpClient,
    url: &str,
    pattern: &LazyLock<Regex>,
) -> Vec<String> {
    web.download_content(url)
        .await
        .map(|content| {
            pattern
                .captures_iter(&content)
                .map(|content| content.extract::<1>())
                .map(|(_, values)| values[0].to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn replace_vars(text: &str, name: &str, version: &str) -> String {
    text.replace("(name)", name).replace("(version)", version)
}

pub struct ImageLoader {
    env: Environment,
}
impl ImageLoader {
    pub fn new(env: Environment) -> Self {
        Self { env }
    }

    pub async fn load_images(&self, arch: Arch) -> Result<ImageCache, ImageError> {
        // 1) Try cache
        let stale_cache = self.read_cache().await?;
        if let Some(cache) = &stale_cache {
            if is_cache_fresh(cache) {
                return Ok(cache.clone());
            }
        }

        // 2) Fetch; on failure return stale cache (if any)
        match self.fetch_and_cache(arch).await {
            Ok(new_cache) => Ok(new_cache),
            Err(fetch_err) => {
                if let Some(cache) = stale_cache {
                    // Return stale cache
                    Ok(cache)
                } else {
                    Err(fetch_err)
                }
            }
        }
    }

    async fn read_cache(&self) -> Result<Option<ImageCache>, ImageError> {
        let path = self.env.get_image_cache_file();
        let path_str = path.to_string_lossy().to_string();

        if !fs::path_exists(&path_str).await? {
            return Ok(None);
        }

        let content = fs::read_file_to_string(&path_str).await?;
        let cache = ImageCache::deserialize(&content)?;
        Ok(Some(cache))
    }

    async fn write_cache(&self, cache: &ImageCache) -> Result<(), ImageError> {
        // Ensure cache directory exists
        let cache_dir = self.env.get_cache_dir();
        let cache_dir_str = cache_dir.to_string_lossy().to_string();
        fs::create_dir(&cache_dir_str).await?;

        let path = self.env.get_image_cache_file();
        let path_str = path.to_string_lossy().to_string();
        let content = cache.serialize()?;
        fs::write_file(&path_str, content.as_bytes()).await?;
        Ok(())
    }

    async fn fetch_and_cache(&self, arch: Arch) -> Result<ImageCache, ImageError> {
        let mut web = HttpClient::new()?;
        let mut images: Vec<Image> = Vec::new();

        for distro in DISTROS.iter() {
            let Some(location) = distro.images.get(&arch) else {
                continue;
            };

            // Discover names (codenames or release branches)
            let names =
                get_http_content_matches(&mut web, distro.overview_url, &distro.overview_pattern)
                    .await;

            // Avoid duplicate names
            let mut seen_names = HashSet::new();
            for name in names {
                if !seen_names.insert(name.clone()) {
                    continue;
                }

                // Discover versions or filenames on the listing page
                let list_url = replace_vars(location.url, &name, "");
                let mut versions =
                    get_http_content_matches(&mut web, &list_url, &location.pattern).await;

                // If nothing matched, create a default "version" using the name
                if versions.is_empty() {
                    versions.push(name.clone());
                }

                // Avoid duplicate versions for this name
                let mut seen_versions = HashSet::new();
                for version in versions {
                    if !seen_versions.insert(version.clone()) {
                        continue;
                    }

                    let url = replace_vars(location.download_url, &name, &version);

                    // Best-effort file size (HEAD). Ignore errors and keep None.
                    let size = web.get_file_size(&url).await.ok().flatten();

                    images.push(Image {
                        vendor: distro.vendor.to_string(),
                        codename: name.clone(),
                        version: version.clone(),
                        arch,
                        url,
                        size,
                    });
                }
            }
        }

        if images.is_empty() {
            return Err(ImageError::NoImagesFound);
        }

        // Sort deterministically: vendor, codename, version, arch
        images.sort_by(|a, b| {
            (
                a.vendor.as_str(),
                a.codename.as_str(),
                a.version.as_str(),
                a.arch,
            )
                .cmp(&(
                    b.vendor.as_str(),
                    b.codename.as_str(),
                    b.version.as_str(),
                    b.arch,
                ))
        });

        let cache = ImageCache {
            images,
            timestamp: now_secs(),
        };

        // Best effort to persist cache; even if write fails, return fetched data
        if let Err(e) = self.write_cache(&cache).await {
            // Log-like behavior could be added here if a logger exists; otherwise ignore
            // and still return the fresh cache.
            eprintln!("Warning: failed to write image cache: {e}");
        }

        Ok(cache)
    }
}

fn is_cache_fresh(cache: &ImageCache) -> bool {
    const CACHE_TTL_SECS: u64 = 24 * 60 * 60; // 1 day
    let now = now_secs();
    now.saturating_sub(cache.timestamp) < CACHE_TTL_SECS
}

fn now_secs() -> u64 {
    time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .unwrap_or_else(|_| time::Duration::from_secs(0))
        .as_secs()
}
