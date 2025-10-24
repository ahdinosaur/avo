use avo_store::Store;
use directories::ProjectDirs;

pub use avo_operation as operation;
pub use avo_params as params;
pub use avo_plan as plan;

pub fn create_store() -> Store {
    let project_dirs =
        ProjectDirs::from("dev", "Avo Org", "Avo").expect("Failed to get project directory");
    let cache_dir = project_dirs.cache_dir();
    Store::new(cache_dir.to_path_buf())
}
