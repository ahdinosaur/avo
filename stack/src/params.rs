use indexmap::IndexMap;
use rimu::{Number, Spanned};

pub struct ParamName(String);

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

pub struct ParamTypes(IndexMap<Spanned<ParamName>, Spanned<ParamType>>);
pub struct ParamValues(IndexMap<Spanned<ParamName>, Spanned<ParamValue>>);
