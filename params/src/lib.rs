//! Parameter schemas and values.

use indexmap::IndexMap;
use rimu::{from_serde_value, SerdeValue, SerdeValueError, Span, Spanned, Value};

use rimu_interop::FromRimu;
use serde::de::DeserializeOwned;

#[derive(Debug, Clone)]
pub enum ParamType {
    Boolean,
    String,
    Number,
    List { item: Box<Spanned<ParamType>> },
    Object { value: Box<Spanned<ParamType>> },
}

#[derive(Debug, Clone)]
pub struct ParamField {
    typ: ParamType,
    optional: bool,
}

impl ParamField {
    pub const fn new(typ: ParamType, optional: bool) -> Self {
        Self { typ, optional }
    }

    pub fn typ(&self) -> &ParamType {
        &self.typ
    }
    pub fn optional(&self) -> &bool {
        &self.optional
    }
}

#[derive(Debug, Clone)]
pub struct ParamTypes(IndexMap<String, Spanned<ParamField>>);

impl ParamTypes {
    pub const fn new(map: IndexMap<String, Spanned<ParamField>>) -> Self {
        ParamTypes(map)
    }
}

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

    pub fn get(&self, key: &str) -> Option<&Spanned<Value>> {
        self.0.get(key)
    }

    pub fn into_type<T>(self) -> Result<T, SerdeValueError>
    where
        T: DeserializeOwned,
    {
        let value = Value::Object(self.0);
        let serde_value = SerdeValue::from(value);
        from_serde_value(serde_value)
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
pub enum IntoParamFieldError {
    NotAnObject,
    OptionalNotABoolean { span: Span },
    FieldType(IntoParamTypeError),
}

impl FromRimu for ParamField {
    type Error = IntoParamFieldError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(mut object) = value else {
            return Err(IntoParamFieldError::NotAnObject);
        };

        // Optional defaults to false (required by default).
        let optional = if let Some(optional_value) = object.swap_remove("optional") {
            let (inner, span) = optional_value.take();
            match inner {
                Value::Boolean(b) => b,
                _ => return Err(IntoParamFieldError::OptionalNotABoolean { span }),
            }
        } else {
            false
        };

        // Parse the underlying type (ignore unknown keys).
        let typ =
            ParamType::from_rimu(Value::Object(object)).map_err(IntoParamFieldError::FieldType)?;

        Ok(ParamField { typ, optional })
    }
}

#[derive(Debug, Clone)]
pub enum IntoParamTypesError {
    NotAnObject,
    Entry {
        key: String,
        error: Box<Spanned<IntoParamFieldError>>,
    },
}

impl FromRimu for ParamTypes {
    type Error = IntoParamTypesError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(map) = value else {
            return Err(IntoParamTypesError::NotAnObject);
        };

        let mut out: IndexMap<String, Spanned<ParamField>> = IndexMap::with_capacity(map.len());

        for (key, value) in map {
            let field = match ParamField::from_rimu_spanned(value) {
                Ok(field) => field,
                Err(error) => {
                    return Err(IntoParamTypesError::Entry {
                        key: key.clone(),
                        error: Box::new(error),
                    })
                }
            };
            out.insert(key, field);
        }

        Ok(ParamTypes(out))
    }
}

#[derive(Debug, Clone)]
pub enum ValidateValueError {
    TypeMismatch {
        expected_type: Box<Spanned<ParamType>>,
        got_value: Box<Spanned<Value>>,
    },
    ListItem {
        index: usize,
        error: Box<ValidateValueError>,
    },
    ObjectEntry {
        key: String,
        error: Box<ValidateValueError>,
    },
}

#[derive(Debug, Clone)]
pub enum ParamValidationError {
    MissingParam {
        key: String,
        expected_type: Box<Spanned<ParamType>>,
    },
    UnknownParam {
        key: String,
        value: Box<Spanned<Value>>,
    },
    InvalidParam {
        key: String,
        error: Box<ValidateValueError>,
    },
}

#[derive(Debug, Clone)]
pub struct ParamValidationErrors {
    pub errors: Vec<ParamValidationError>,
}

impl ParamValidationErrors {
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }
}

fn mismatch(typ: &Spanned<ParamType>, value: &Spanned<Value>) -> ValidateValueError {
    ValidateValueError::TypeMismatch {
        expected_type: Box::new(typ.clone()),
        got_value: Box::new(value.clone()),
    }
}

pub fn validate(
    param_type: &Spanned<ParamType>,
    value: &Spanned<Value>,
) -> Result<(), ValidateValueError> {
    let typ_inner = param_type.inner();
    let value_inner = value.inner();

    match typ_inner {
        ParamType::Boolean => match value_inner {
            Value::Boolean(_) => Ok(()),
            _ => Err(mismatch(param_type, value)),
        },

        ParamType::String => match value_inner {
            Value::String(_) => Ok(()),
            _ => Err(mismatch(param_type, value)),
        },

        ParamType::Number => match value_inner {
            Value::Number(_) => Ok(()),
            _ => Err(mismatch(param_type, value)),
        },

        ParamType::List { item } => {
            let Value::List(items) = value_inner else {
                return Err(mismatch(param_type, value));
            };

            for (index, item_value) in items.iter().enumerate() {
                if let Err(error) = validate(item, item_value) {
                    return Err(ValidateValueError::ListItem {
                        index,
                        error: Box::new(error),
                    });
                }
            }

            Ok(())
        }

        ParamType::Object { value: value_type } => {
            let Value::Object(map) = value_inner else {
                return Err(mismatch(param_type, value));
            };

            for (key, entry_value) in map.iter() {
                if let Err(error) = validate(value_type, entry_value) {
                    return Err(ValidateValueError::ObjectEntry {
                        key: key.clone(),
                        error: Box::new(error),
                    });
                }
            }

            Ok(())
        }
    }
}

impl ParamTypes {
    pub fn validate(&self, values: &ParamValues) -> Result<(), ParamValidationErrors> {
        let mut errors: Vec<ParamValidationError> = Vec::new();

        // Requiredness and per-field validation.
        for (key, spanned_field) in self.0.iter() {
            let (field, field_span) = spanned_field.clone().take();

            let spanned_type = Spanned::new(field.typ().clone(), field_span);

            match values.0.get(key) {
                Some(spanned_value) => {
                    if let Err(error) = validate(&spanned_type, spanned_value) {
                        errors.push(ParamValidationError::InvalidParam {
                            key: key.clone(),
                            error: Box::new(error),
                        });
                    }
                }
                None => {
                    if !field.optional {
                        errors.push(ParamValidationError::MissingParam {
                            key: key.clone(),
                            expected_type: Box::new(spanned_type),
                        });
                    }
                }
            }
        }

        // Unknown keys.
        for (key, spanned_value) in values.0.iter() {
            if !self.0.contains_key(key) {
                errors.push(ParamValidationError::UnknownParam {
                    key: key.clone(),
                    value: Box::new(spanned_value.clone()),
                });
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ParamValidationErrors { errors })
        }
    }
}
