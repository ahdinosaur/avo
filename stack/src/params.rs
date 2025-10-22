use indexmap::IndexMap;
use rimu::{Number, Span, Spanned, Value};

use crate::FromRimu;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamName(String);

#[derive(Debug, Clone)]
pub enum ParamType {
    Boolean,
    String,
    Number,
    List { item: Box<Spanned<ParamType>> },
    Object { value: Box<Spanned<ParamType>> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    Boolean(bool),
    String(String),
    Number(Number),
    List(Vec<Spanned<ParamValue>>),
    Object(IndexMap<String, Spanned<ParamValue>>),
}

#[derive(Debug, Clone)]
pub struct ParamTypes(pub IndexMap<ParamName, Spanned<ParamType>>);

#[derive(Debug, Clone)]
pub struct ParamValues(pub IndexMap<ParamName, Spanned<ParamValue>>);

#[derive(Debug, Clone)]
pub enum IntoParamTypeError {
    NotAnObject,
    HasNoType,
    TypeNotAString { span: Span },
    UnknownType(String),
    ListMissingItem,
    ListItem(Box<Spanned<IntoParamTypeError>>),
    ObjectMissingValue,
    ObjectValue(Box<Spanned<IntoParamTypeError>>),
}

impl FromRimu for ParamType {
    type Error = IntoParamTypeError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(mut object) = value else {
            return Err(IntoParamTypeError::NotAnObject);
        };

        let Some(typ) = object.get("type") else {
            return Err(IntoParamTypeError::HasNoType);
        };

        let (typ, typ_span) = typ.clone().take();
        let Value::String(typ) = typ else {
            return Err(IntoParamTypeError::TypeNotAString { span: typ_span });
        };

        match typ.as_str() {
            "boolean" => Ok(ParamType::Boolean),
            "string" => Ok(ParamType::String),
            "number" => Ok(ParamType::Number),
            "list" => {
                let item = object
                    .swap_remove("item")
                    .ok_or(IntoParamTypeError::ListMissingItem)?;
                let item = ParamType::from_rimu_spanned(item)
                    .map_err(|error| IntoParamTypeError::ListItem(Box::new(error)))?;
                Ok(ParamType::List {
                    item: Box::new(item),
                })
            }
            "object" => {
                let value = object
                    .swap_remove("value")
                    .ok_or(IntoParamTypeError::ObjectMissingValue)?;
                let value = ParamType::from_rimu_spanned(value)
                    .map_err(|error| IntoParamTypeError::ObjectValue(Box::new(error)))?;
                Ok(ParamType::Object {
                    value: Box::new(value),
                })
            }
            other => Err(IntoParamTypeError::UnknownType(other.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub enum IntoParamValueError {
    UnsupportedValueKind,
    ListItem(Box<Spanned<IntoParamValueError>>),
    ObjectValue {
        key: String,
        source: Box<Spanned<IntoParamValueError>>,
    },
}

impl FromRimu for ParamValue {
    type Error = IntoParamValueError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Boolean(b) => Ok(ParamValue::Boolean(b)),
            Value::String(s) => Ok(ParamValue::String(s)),
            Value::Number(n) => Ok(ParamValue::Number(n)),
            Value::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    let item = ParamValue::from_rimu_spanned(item)
                        .map_err(|error| IntoParamValueError::ListItem(Box::new(error)))?;
                    out.push(item);
                }
                Ok(ParamValue::List(out))
            }
            Value::Object(map) => {
                let mut out = IndexMap::with_capacity(map.len());
                for (key, value) in map {
                    let value = ParamValue::from_rimu_spanned(value).map_err(|error| {
                        IntoParamValueError::ObjectValue {
                            key: key.clone(),
                            source: Box::new(error),
                        }
                    })?;
                    out.insert(key, value);
                }
                Ok(ParamValue::Object(out))
            }
            _ => Err(IntoParamValueError::UnsupportedValueKind),
        }
    }
}

#[derive(Debug, Clone)]
pub enum IntoParamTypesError {
    NotAnObject,
    Entry {
        key: String,
        error: Box<Spanned<IntoParamTypeError>>,
    },
}

impl FromRimu for ParamTypes {
    type Error = IntoParamTypesError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(map) = value else {
            return Err(IntoParamTypesError::NotAnObject);
        };

        let mut out: IndexMap<ParamName, Spanned<ParamType>> = IndexMap::with_capacity(map.len());

        for (key, value) in map {
            let typ = match ParamType::from_rimu_spanned(value) {
                Ok(typ) => typ,
                Err(error) => {
                    return Err(IntoParamTypesError::Entry {
                        key: key.clone(),
                        error: Box::new(error),
                    })
                }
            };

            out.insert(ParamName(key), typ);
        }

        Ok(ParamTypes(out))
    }
}

#[derive(Debug, Clone)]
pub enum IntoParamValuesError {
    NotAnObject,
    Entry {
        key: String,
        error: Box<Spanned<IntoParamValueError>>,
    },
}

impl FromRimu for ParamValues {
    type Error = IntoParamValuesError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(map) = value else {
            return Err(IntoParamValuesError::NotAnObject);
        };

        let mut out: IndexMap<ParamName, Spanned<ParamValue>> = IndexMap::with_capacity(map.len());

        for (key, value) in map {
            let value = match ParamValue::from_rimu_spanned(value) {
                Ok(value) => value,
                Err(error) => {
                    return Err(IntoParamValuesError::Entry {
                        key: key.clone(),
                        error: Box::new(error),
                    })
                }
            };

            out.insert(ParamName(key), value);
        }

        Ok(ParamValues(out))
    }
}
