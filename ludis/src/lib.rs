use ludis_env::Environment;
use ludis_store::Store;

pub use ludis_operation as operation;
pub use ludis_params as params;
pub use ludis_plan as plan;

pub fn create_store() -> Store {
    let env = Environment::create().expect("Failed to get Ludis project environment");
    Store::new(env.cache_dir())
}
