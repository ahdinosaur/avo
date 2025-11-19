use ludis_store::Store;
use directories::ProjectDirs;

pub use ludis_operation as operation;
pub use ludis_params as params;
pub use ludis_plan as plan;

pub fn create_store() -> Store {
    let project_dirs =
        ProjectDirs::from("dev", "Ludis Org", "Ludis").expect("Failed to get project directory");
    let cache_dir = project_dirs.cache_dir();
    Store::new(cache_dir.to_path_buf())
}
