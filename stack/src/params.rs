use indexmap::IndexMap;
use rimu::{Number, Spanned, Value};

#[derive(Debug, Clone)]
pub struct ParamName(String);

#[derive(Debug, Clone)]
pub enum ParamType {
    Boolean,
    String,
    Number,
    List { item: Box<ParamType> },
    Object { value: Box<ParamType> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    Boolean(bool),
    String(String),
    Number(Number),
    List(Vec<ParamValue>),
    Object(IndexMap<String, ParamValue>),
}

#[derive(Debug, Clone)]
pub struct ParamTypes(IndexMap<Spanned<ParamName>, Spanned<ParamType>>);

#[derive(Debug, Clone)]
pub struct ParamValues(IndexMap<Spanned<ParamName>, Spanned<ParamValue>>);

#[derive(Debug, Clone)]
pub enum IntoParamTypeError {
    NotAnObject,
    HasNoType,
    TypeNotAString,
    UnknownType(String),
    ListMissingItem,
    ObjectMissingValue,
}

impl TryFrom<Spanned<Value>> for ParamType {
    type Error = IntoParamTypeError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(object) = value else {
            return Err(IntoParamTypeError::NotAnObject);
        };

        let Some(typ) = object.get("type") else {
            return Err(IntoParamTypeError::HasNoType);
        };
        let Value::String(typ) = typ else {
            return Err(IntoParamTypeError::TypeNotAString);
        };

        match typ.as_str() {
            "boolean" => Ok(ParamType::Boolean),
            "string" => Ok(ParamType::String),
            "number" => Ok(ParamType::Number),
            "list" => {
                let item = object
                    .get("item")
                    .ok_or(IntoParamTypeError::ListMissingItem)?;
                let item = ParamType::try_from(item)?;
                Ok(ParamType::List { item })
            }
            "object" => {
                let value = object
                    .get("value")
                    .ok_or(IntoParamTypeError::ObjectMissingValue)?;
                let value = ParamType::try_from(value)?;
                Ok(ParamType::Object { value })
            }
            other => Err(IntoParamTypeError::UnknownType(other.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub enum IntoParamValueError {
    UnsupportedType,
    ListItem(Box<IntoParamValueError>),
    ObjectValue {
        key: String,
        source: Box<IntoParamValueError>,
    },
}

impl TryFrom<Value> for ParamValue {
    type Error = IntoParamValueError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Boolean(b) => Ok(ParamValue::Boolean(b)),
            Value::String(s) => Ok(ParamValue::String(s)),
            Value::Number(n) => Ok(ParamValue::Number(n)),
            Value::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    let pv = ParamValue::try_from(item)
                        .map_err(|e| IntoParamValueError::ListItem(Box::new(e)))?;
                    out.push(pv);
                }
                Ok(ParamValue::List(out))
            }
            Value::Object(map) => {
                let mut out = IndexMap::with_capacity(map.len());
                for (k, v) in map {
                    let pv =
                        ParamValue::try_from(v).map_err(|e| IntoParamValueError::ObjectValue {
                            key: k.clone(),
                            source: Box::new(e),
                        })?;
                    out.insert(k, pv);
                }
                Ok(ParamValue::Object(out))
            }
            _ => Err(IntoParamValueError::UnsupportedValueKind),
        }
    }
}
