use lusid_store::StoreItemId;
use rimu::SourceId;
use std::{
    fmt::Display,
    path::{Path, PathBuf},
};
use url::Url;

#[derive(Debug, Clone)]
pub enum PlanId {
    Path(PathBuf),
    Git(Url, PathBuf),
}

impl PlanId {
    pub fn join<P: AsRef<Path>>(&self, path: P) -> PlanId {
        match self {
            PlanId::Path(current_path) => PlanId::Path(relative(current_path, path)),
            PlanId::Git(url, current_path) => {
                PlanId::Git(url.clone(), relative(current_path, path))
            }
        }
    }

    pub fn as_path(self) -> Option<PathBuf> {
        match self {
            PlanId::Path(path) => Some(path),
            PlanId::Git(_, _) => None,
        }
    }
}

fn relative<P: AsRef<Path>>(current_path: &Path, next_path: P) -> PathBuf {
    current_path
        .parent()
        .unwrap_or(&PathBuf::default())
        .join(next_path)
}

impl Display for PlanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanId::Path(path) => write!(f, "Path({})", path.display()),
            PlanId::Git(url, path) => write!(f, "Git({}, {})", url, path.display()),
        }
    }
}

impl From<PlanId> for StoreItemId {
    fn from(value: PlanId) -> Self {
        match value {
            PlanId::Path(path) => StoreItemId::LocalFile(path),
            PlanId::Git(_url, _path) => todo!(),
        }
    }
}

impl From<PlanId> for SourceId {
    fn from(value: PlanId) -> Self {
        match value {
            PlanId::Path(path) => SourceId::from(path.to_string_lossy().to_string()),
            PlanId::Git(mut url, path) => {
                url.query_pairs_mut()
                    .append_pair("path", &path.to_string_lossy());
                SourceId::from(url.to_string())
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlanItemId {
    plan_id: PlanId,
    item_id: String,
}

#[derive(Debug, Clone)]
pub struct SubItemId(String);

#[derive(Debug, Clone)]
pub enum NodeId {
    Plan(PlanId),
    PlanItem(PlanItemId),
    SubItem(SubItemId),
}
