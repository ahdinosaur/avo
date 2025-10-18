use std::path::PathBuf;

use crate::{
    operator::Operator,
    params::{ParamTypes, ParamValues},
};

pub struct Name(String);
pub struct Version(String);

pub struct BlockCallRef {
    op: PathBuf,
    params: ParamValues,
}

pub struct BlockCall {
    op: Operator,
    params: ParamValues,
}

pub type BlocksFn = Box<dyn Fn(ParamValues) -> Vec<BlockCallRef>>;

pub struct BlockDefinition {
    name: Name,
    version: Version,
    params: ParamTypes,
    blocks: BlocksFn,
}
