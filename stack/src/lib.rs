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

struct BlockDefinition<BlocksFn>
where
    BlocksFn: Fn(ParamValues) -> Vec<BlockCall>,
{
    name: String,
    version: Version,
    params: ParamTypes,
    blocks: BlocksFn,
}

enum Operator {
    Block(BlockDefinition<Box<dyn Fn(ParamValues) -> Vec<BlockCall>>>),
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
