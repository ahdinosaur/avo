use indexmap::IndexMap;
use rimu::Number;

pub enum ParamType {
    Boolean,
    String,
    Number,
    List { item: Box<ParamType> },
    Object { value: Box<ParamType> },
}

#[derive(Clone, PartialEq)]
pub enum ParamValue {
    Boolean(bool),
    String(String),
    Number(Number),
    List(Vec<ParamValue>),
    Object(IndexMap<String, ParamValue>),
}

pub struct ParamTypes(IndexMap<String, ParamType>);
pub struct ParamValues(IndexMap<String, ParamValue>);
