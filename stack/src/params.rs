use indexmap::IndexMap;
use rimu::{Number, Spanned, Value};

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

pub enum IntoParamTypeError {
    NotAnObject,
    HasNoType,
    ListMissingItem,
    ObjectMissingValue,
}

impl TryFrom<Value> for ParamType {
    type Error = IntoParamTypeError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(object) = value else {
            return Err(IntoParamTypeError::NotAnObject);
        };

        let Some(typ) = object.get("type") else {
            return Err(IntoParamTypeError::HasNoType);
        };

        match typ {
            "boolean" => Ok(ParamType::Boolean),
            "string" => Ok(ParamType::String),
            "number" => Ok(ParamType::Number),
            "list" => {
                let item = object
                    .get("item")
                    .ok_or(IntoParamTypeError::ListMissingItem)?;
                Ok(ParamType::List { item })
            }
            "object" => {
                let value = object
                    .get("value")
                    .ok_or(IntoParamTypeError::ObjectMissingValue)?;
                Ok(ParamType::Object { value })
            }
        }
    }
}

pub enum IntoParamValueError {}
