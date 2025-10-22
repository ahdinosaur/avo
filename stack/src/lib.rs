#![allow(dead_code)]

pub mod block;
pub mod operation;
pub mod params;
pub mod parser;
mod rimu_interop;
pub mod store;
pub mod system;

pub use rimu_interop::FromRimu;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(true, true);
    }
}
