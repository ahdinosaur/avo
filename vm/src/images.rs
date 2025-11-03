// See https://github.com/cubic-vm/cubic/blob/main/src/image/image_factory.rs

use std::{collections::HashMap, io::Read, sync::LazyLock};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{env::Environment, http::HttpClient, system::Arch};

pub enum ImageError {}

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
    pub fn deserialize(input: &str) -> Result<ImageCache, toml::de::Error> {
        toml::from_str(input)
    }

    pub fn serialize(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
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
