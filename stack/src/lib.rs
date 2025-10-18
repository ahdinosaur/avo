#![allow(dead_code)]

pub mod system;

use std::{collections::HashMap, path::PathBuf};

// TODO
type Version = String;

enum ParamType {
    Boolean,
}

enum ParamValue {
    Boolean(bool),
}

struct BlockCall {
    operator: PathBuf,
    params: HashMap<String, ParamValue>,
}

type ParamTypes = HashMap<String, ParamType>;
type ParamValues = HashMap<String, ParamValue>;

struct BlockDefinition {
    name: String,
    version: Version,
    params: ParamTypes,
    blocks: Box<dyn Fn(ParamValues) -> Vec<BlockCall>>,
}

enum Operator {
    Block(BlockDefinition),
    Core(CoreOperator),
}

enum CoreOperator {
    Package {},
    Command {},
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(true, true);
    }
}
