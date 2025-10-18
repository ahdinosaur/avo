use crate::block::BlockDefinition;

pub enum Operator {
    Block(BlockDefinition),
    Core(CoreOperator),
}

pub enum CoreOperator {
    Package {},
    Command {},
}
