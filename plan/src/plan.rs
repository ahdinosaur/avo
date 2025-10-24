#![allow(dead_code)]

use avo_params::{ParamTypes, ParamTypesFromRimuError, ParamValues, ParamValuesFromRimuError};
use displaydoc::Display;
use rimu::{Function, Span, Spanned, Value};
use rimu_interop::FromRimu;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Name(pub String);

#[derive(Debug, Clone, Error, Display)]
pub enum IntoNameError {
    /// Expected a string for plan name
    NotAString,
}

impl FromRimu for Name {
    type Error = IntoNameError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::String(string) = value else {
            return Err(IntoNameError::NotAString);
        };
        Ok(Name(string))
    }
}

#[derive(Debug, Clone)]
pub struct Version(pub String);

#[derive(Debug, Clone, Error, Display)]
pub enum IntoVersionError {
    /// Expected a string for plan version
    NotAString,
}

impl FromRimu for Version {
    type Error = IntoVersionError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::String(string) = value else {
            return Err(IntoVersionError::NotAString);
        };
        Ok(Version(string))
    }
}

/// A single module call from setup's returned list.
/// Example:
///   { module: "@core/pkg", id: "install-nvim", params: { package: "nvim" } }
#[derive(Debug, Clone)]
pub struct PlanAction {
    pub id: Option<Spanned<String>>,
    pub module: Spanned<String>,
    pub params: Option<Spanned<ParamValues>>,
    pub before: Vec<Spanned<String>>,
    pub after: Vec<Spanned<String>>,
}

impl PlanAction {
    pub fn is_core_module(module: &Spanned<String>) -> Option<&str> {
        module.inner().strip_prefix("@core/")
    }
}

#[derive(Debug, Clone, Error, Display)]
pub enum IntoPlanActionError {
    /// Expected an object for plan action
    NotAnObject,
    /// Missing property: "module"
    ModuleMissing,
    /// Property "module" must be a string
    ModuleNotAString { span: Span },
    /// Property "id" must be a string
    IdNotAString { span: Span },
    /// Failed to convert "params" into ParamValues: {0:?}
    Params(Spanned<ParamValuesFromRimuError>),
    /// Property "before" must be a list
    BeforeNotAList { span: Span },
    /// "before" list item must be a string
    BeforeItemNotAString { item_span: Span },
    /// Property "after" must be a list
    AfterNotAList { span: Span },
    /// "after" list item must be a string
    AfterItemNotAString { item_span: Span },
}

impl FromRimu for PlanAction {
    type Error = IntoPlanActionError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(mut object) = value else {
            return Err(IntoPlanActionError::NotAnObject);
        };

        let module = match object.swap_remove("module") {
            Some(sp) => {
                let (value, span) = sp.clone().take();
                match value {
                    Value::String(s) => Spanned::new(s, span),
                    _ => {
                        return Err(IntoPlanActionError::ModuleNotAString { span });
                    }
                }
            }
            None => return Err(IntoPlanActionError::ModuleMissing),
        };

        let id = object
            .swap_remove("id")
            .map(|sp| {
                let (value, span) = sp.clone().take();
                match value {
                    Value::String(s) => Ok(Spanned::new(s, span)),
                    _ => Err(IntoPlanActionError::IdNotAString { span }),
                }
            })
            .transpose()?;

        let params = object
            .swap_remove("params")
            .map(|sp| ParamValues::from_rimu_spanned(sp).map_err(IntoPlanActionError::Params))
            .transpose()?;

        let before = match object.swap_remove("before") {
            None => Vec::new(),
            Some(value) => {
                let (value, span) = value.clone().take();
                match value {
                    Value::List(items) => {
                        let mut out = Vec::with_capacity(items.len());
                        for item in items {
                            let (item_value, item_span) = item.clone().take();
                            match item_value {
                                Value::String(s) => out.push(Spanned::new(s, item_span)),
                                _ => {
                                    return Err(IntoPlanActionError::BeforeItemNotAString {
                                        item_span,
                                    });
                                }
                            }
                        }
                        out
                    }
                    _ => return Err(IntoPlanActionError::BeforeNotAList { span }),
                }
            }
        };

        let after = match object.swap_remove("after") {
            None => Vec::new(),
            Some(value) => {
                let (value, span) = value.clone().take();
                match value {
                    Value::List(items) => {
                        let mut out = Vec::with_capacity(items.len());
                        for item in items {
                            let (item_value, item_span) = item.clone().take();
                            match item_value {
                                Value::String(s) => out.push(Spanned::new(s, item_span)),
                                _ => {
                                    return Err(IntoPlanActionError::AfterItemNotAString {
                                        item_span,
                                    });
                                }
                            }
                        }
                        out
                    }
                    _ => return Err(IntoPlanActionError::AfterNotAList { span }),
                }
            }
        };

        Ok(PlanAction {
            id,
            module,
            params,
            before,
            after,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SetupFunction(pub Function);

#[derive(Debug, Clone, Error, Display)]
pub enum IntoSetupFunctionError {
    /// Expected a function for "setup"
    NotAFunction,
}

impl FromRimu for SetupFunction {
    type Error = IntoSetupFunctionError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Function(func) = value else {
            return Err(IntoSetupFunctionError::NotAFunction);
        };
        Ok(SetupFunction(func))
    }
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub name: Option<Spanned<Name>>,
    pub version: Option<Spanned<Version>>,
    pub params: Option<Spanned<ParamTypes>>,
    /// setup: (params, system) => list of PlanAction
    pub setup: Spanned<SetupFunction>,
}

#[derive(Debug, Clone, Error, Display)]
pub enum IntoPlanError {
    /// Expected an object for plan
    NotAnObject,
    /// Invalid plan name: {0:?}
    Name(Spanned<IntoNameError>),
    /// Invalid plan version: {0:?}
    Version(Spanned<IntoVersionError>),
    /// Invalid plan params: {0:?}
    Params(Spanned<ParamTypesFromRimuError>),
    /// Missing property: "setup"
    SetupMissing,
    /// "setup" is not a function: {0:?}
    SetupNotAFunction(Spanned<IntoSetupFunctionError>),
}

impl FromRimu for Plan {
    type Error = IntoPlanError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(mut object) = value else {
            return Err(IntoPlanError::NotAnObject);
        };

        let name = object
            .swap_remove("name")
            .map(|name| Name::from_rimu_spanned(name).map_err(IntoPlanError::Name))
            .transpose()?;

        let version = object
            .swap_remove("version")
            .map(|v| Version::from_rimu_spanned(v).map_err(IntoPlanError::Version))
            .transpose()?;

        let params = object
            .swap_remove("params")
            .map(|params| ParamTypes::from_rimu_spanned(params).map_err(IntoPlanError::Params))
            .transpose()?;

        let setup_sp = object
            .swap_remove("setup")
            .ok_or(IntoPlanError::SetupMissing)?;
        let setup =
            SetupFunction::from_rimu_spanned(setup_sp).map_err(IntoPlanError::SetupNotAFunction)?;

        Ok(Plan {
            name,
            version,
            params,
            setup,
        })
    }
}
