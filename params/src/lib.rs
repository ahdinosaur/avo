//! Parameter schemas and values.

use displaydoc::Display;
use indexmap::IndexMap;
use rimu::{SerdeValue, SerdeValueError, SourceId, Span, Spanned, Value, from_serde_value};
use rimu_interop::{FromRimu, ToRimuError, to_rimu};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

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
    pub const fn new(typ: ParamType) -> Self {
        Self {
            typ,
            optional: false,
        }
    }

    pub fn typ(&self) -> &ParamType {
        &self.typ
    }

    pub fn optional(&self) -> &bool {
        &self.optional
    }
}

#[derive(Debug, Clone)]
pub enum ParamTypes {
    // A single object structure: keys -> fields
    Struct(IndexMap<String, Spanned<ParamField>>),
    // A union of possible object structures.
    Union(Vec<IndexMap<String, Spanned<ParamField>>>),
}

#[derive(Debug, Clone)]
pub struct ParamValues(IndexMap<String, Spanned<Value>>);

#[derive(Debug, Clone, Error, Display)]
pub enum ParamValuesFromTypeError {
    /// Failed to convert serializable value to Rimu
    ToRimu(#[source] ToRimuError),
    /// Failed to convert Rimu value into parameter values
    FromRimu(#[source] ParamValuesFromRimuError),
}

impl ParamValues {
    pub fn from_type<T>(
        value: T,
        source_id: SourceId,
    ) -> Result<Spanned<Self>, ParamValuesFromTypeError>
    where
        T: Serialize,
    {
        let rimu_value = to_rimu(value, source_id).map_err(ParamValuesFromTypeError::ToRimu)?;
        ParamValues::from_rimu_spanned(rimu_value)
            .map_err(|error| ParamValuesFromTypeError::FromRimu(error.into_inner()))
    }
}

#[derive(Debug, Clone, Error, Display)]
pub enum ParamValuesFromRimuError {
    /// Expected an object mapping parameter names to values
    NotAnObject,
}

impl FromRimu for ParamValues {
    type Error = ParamValuesFromRimuError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(object) = value else {
            return Err(ParamValuesFromRimuError::NotAnObject);
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

#[derive(Debug, Clone, Error, Display)]
pub enum ParamTypeFromRimuError {
    /// Expected an object for parameter type
    NotAnObject,
    /// Missing property: "type"
    HasNoType,
    /// The "type" property must be a string
    TypeNotAString { span: Span },
    /// Unknown parameter type: {0}
    UnknownType(String),
    /// List type is missing required "item" property
    ListMissingItem,
    /// Invalid "item" type in list: {0:?}
    ListItem(Box<Spanned<ParamTypeFromRimuError>>),
    /// Object type is missing required "value" property
    ObjectMissingValue,
    /// Invalid "value" type in object: {0:?}
    ObjectValue(Box<Spanned<ParamTypeFromRimuError>>),
}

impl FromRimu for ParamType {
    type Error = ParamTypeFromRimuError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(mut object) = value else {
            return Err(ParamTypeFromRimuError::NotAnObject);
        };

        let Some(typ) = object.get("type") else {
            return Err(ParamTypeFromRimuError::HasNoType);
        };

        let (typ, typ_span) = typ.clone().take();
        let Value::String(typ) = typ else {
            return Err(ParamTypeFromRimuError::TypeNotAString { span: typ_span });
        };

        match typ.as_str() {
            "boolean" => Ok(ParamType::Boolean),
            "string" => Ok(ParamType::String),
            "number" => Ok(ParamType::Number),
            "list" => {
                let item = object
                    .swap_remove("item")
                    .ok_or(ParamTypeFromRimuError::ListMissingItem)?;
                let item = ParamType::from_rimu_spanned(item)
                    .map_err(|error| ParamTypeFromRimuError::ListItem(Box::new(error)))?;
                Ok(ParamType::List {
                    item: Box::new(item),
                })
            }
            "object" => {
                let value = object
                    .swap_remove("value")
                    .ok_or(ParamTypeFromRimuError::ObjectMissingValue)?;
                let value = ParamType::from_rimu_spanned(value)
                    .map_err(|error| ParamTypeFromRimuError::ObjectValue(Box::new(error)))?;
                Ok(ParamType::Object {
                    value: Box::new(value),
                })
            }
            other => Err(ParamTypeFromRimuError::UnknownType(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Error, Display)]
pub enum ParamFieldFromRimuError {
    /// Expected an object for parameter field
    NotAnObject,
    /// The "optional" property must be a boolean
    OptionalNotABoolean { span: Span },
    /// Invalid field type: {0:?}
    FieldType(#[source] ParamTypeFromRimuError),
}

impl FromRimu for ParamField {
    type Error = ParamFieldFromRimuError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(mut object) = value else {
            return Err(ParamFieldFromRimuError::NotAnObject);
        };

        let optional = if let Some(optional_value) = object.swap_remove("optional") {
            let (inner, span) = optional_value.take();
            match inner {
                Value::Boolean(b) => b,
                _ => {
                    return Err(ParamFieldFromRimuError::OptionalNotABoolean { span });
                }
            }
        } else {
            false
        };

        let typ = ParamType::from_rimu(Value::Object(object))
            .map_err(ParamFieldFromRimuError::FieldType)?;
        Ok(ParamField { typ, optional })
    }
}

#[derive(Debug, Clone, Error, Display)]
pub enum ParamTypesFromRimuError {
    /// Expected an object (struct) or a list (union) for parameter types
    NotAnObjectOrList,
    /// Invalid struct entry for key "{key}": {error:?}
    StructEntry {
        key: String,
        error: Box<Spanned<ParamFieldFromRimuError>>,
    },
    /// Union item at index {index} is not an object
    UnionItemNotAnObject { index: usize, span: Span },
    /// Invalid union item entry for key "{key}" at index {index}: {error:?}
    UnionItemEntry {
        index: usize,
        key: String,
        error: Box<Spanned<ParamFieldFromRimuError>>,
    },
}

impl FromRimu for ParamTypes {
    type Error = ParamTypesFromRimuError;

    // In Rimu:
    // - An object defines a Struct (map of fields).
    // - A list defines a Union; each list item is an object defining one case.
    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Object(map) => {
                let mut out: IndexMap<String, Spanned<ParamField>> =
                    IndexMap::with_capacity(map.len());

                for (key, value) in map {
                    let field = match ParamField::from_rimu_spanned(value) {
                        Ok(field) => field,
                        Err(error) => {
                            return Err(ParamTypesFromRimuError::StructEntry {
                                key: key.clone(),
                                error: Box::new(error),
                            })
                        }
                    };
                    out.insert(key, field);
                }

                Ok(ParamTypes::Struct(out))
            }
            Value::List(items) => {
                let mut cases: Vec<IndexMap<String, Spanned<ParamField>>> =
                    Vec::with_capacity(items.len());

                for (index, spanned_item) in items.into_iter().enumerate() {
                    let (inner, span) = spanned_item.clone().take();
                    let Value::Object(case_map) = inner else {
                        return Err(ParamTypesFromRimuError::UnionItemNotAnObject { index, span });
                    };

                    let mut case_out: IndexMap<String, Spanned<ParamField>> =
                        IndexMap::with_capacity(case_map.len());

                    for (key, value) in case_map {
                        let field = match ParamField::from_rimu_spanned(value) {
                            Ok(field) => field,
                            Err(error) => {
                                return Err(ParamTypesFromRimuError::UnionItemEntry {
                                    index,
                                    key: key.clone(),
                                    error: Box::new(error),
                                })
                            }
                        };
                        case_out.insert(key, field);
                    }

                    cases.push(case_out);
                }

                Ok(ParamTypes::Union(cases))
            }
            _ => Err(ParamTypesFromRimuError::NotAnObjectOrList),
        }
    }
}

#[derive(Debug, Clone, Error, Display)]
pub enum ValidateValueError {
    /// Value does not match expected type
    TypeMismatch {
        expected_type: Box<Spanned<ParamType>>,
        got_value: Box<Spanned<Value>>,
    },
    /// Invalid list item at index {index}: {error:?}
    ListItem {
        index: usize,
        error: Box<ValidateValueError>,
    },
    /// Invalid object entry for key "{key}": {error:?}
    ObjectEntry {
        key: String,
        error: Box<ValidateValueError>,
    },
}

#[derive(Debug, Clone, Error, Display)]
pub enum ParamValidationError {
    /// Missing required parameter "{key}"
    MissingParam {
        key: String,
        expected_type: Box<Spanned<ParamType>>,
    },
    /// Unknown parameter "{key}"
    UnknownParam {
        key: String,
        value: Box<Spanned<Value>>,
    },
    /// Invalid parameter "{key}": {error:?}
    InvalidParam {
        key: String,
        error: Box<ValidateValueError>,
    },
    /// Parameter union did not match any case
    UnionNoMatch {
        case_errors: Vec<ParamValidationErrors>,
    },
}

#[derive(Debug, Clone, Error, Display)]
#[displaydoc("Parameter validation failed")]
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

fn validate_type(
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
                if let Err(error) = validate_type(item, item_value) {
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
                if let Err(error) = validate_type(value_type, entry_value) {
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

fn validate_struct(
    fields: &IndexMap<String, Spanned<ParamField>>,
    values: &ParamValues,
) -> Result<(), ParamValidationErrors> {
    let mut errors: Vec<ParamValidationError> = Vec::new();

    // Requiredness and per-field validation.
    for (key, spanned_field) in fields.iter() {
        let (field, field_span) = spanned_field.clone().take();
        let spanned_type = Spanned::new(field.typ().clone(), field_span);

        match values.0.get(key) {
            Some(spanned_value) => {
                if let Err(error) = validate_type(&spanned_type, spanned_value) {
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
        if !fields.contains_key(key) {
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

// For Struct: validate all fields.
// For Union: succeed if any one case validates; otherwise return all case errors.
pub fn validate(
    param_types: &Option<Spanned<ParamTypes>>,
    param_values: &Spanned<ParamValues>,
) -> Result<(), ParamValidationErrors> {
    let Some(param_types) = param_types else {
        return Ok(());
    };
    let param_types = param_types.inner();
    let param_values = param_values.inner();
    match param_types {
        ParamTypes::Struct(map) => validate_struct(map, param_values),
        ParamTypes::Union(cases) => {
            if cases.is_empty() {
                return Err(ParamValidationErrors {
                    errors: vec![ParamValidationError::UnionNoMatch {
                        case_errors: vec![],
                    }],
                });
            }

            let mut all_case_errors: Vec<ParamValidationErrors> = Vec::with_capacity(cases.len());

            for case in cases {
                match validate_struct(case, param_values) {
                    Ok(()) => return Ok(()),
                    Err(errs) => all_case_errors.push(errs),
                }
            }

            Err(ParamValidationErrors {
                errors: vec![ParamValidationError::UnionNoMatch {
                    case_errors: all_case_errors,
                }],
            })
        }
    }
}
