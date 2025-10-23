//! Parameter schemas and values.

use indexmap::IndexMap;
use rimu::{Span, Spanned, Value};

use crate::FromRimu;

#[derive(Debug, Clone)]
pub enum ParamType {
    Boolean,
    String,
    Number,
    List { item: Box<Spanned<ParamType>> },
    Object { value: Box<Spanned<ParamType>> },
}

#[derive(Debug, Clone)]
pub struct ParamTypes(IndexMap<String, Spanned<ParamType>>);

#[derive(Debug, Clone)]
pub struct ParamValues(IndexMap<String, Spanned<Value>>);

#[derive(Debug, Clone)]
pub enum IntoParamValuesError {
    NotAnObject,
}

impl FromRimu for ParamValues {
    type Error = IntoParamValuesError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(object) = value else {
            return Err(IntoParamValuesError::NotAnObject);
        };
        Ok(ParamValues(object))
    }
}

impl ParamValues {
    pub fn into_rimu(self) -> Value {
        Value::Object(self.0)
    }
}

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

        let mut out: IndexMap<String, Spanned<ParamType>> = IndexMap::with_capacity(map.len());

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
            out.insert(key, typ);
        }

        Ok(ParamTypes(out))
    }
}

impl ParamValues {
    pub fn get(&self, key: &str) -> Option<&Spanned<Value>> {
        self.0.get(key)
    }
}
