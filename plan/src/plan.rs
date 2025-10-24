use avo_params::{ParamTypes, ParamTypesFromRimuError, ParamValues, ParamValuesFromRimuError};
use rimu::{Function, Span, Spanned, Value};
use rimu_interop::FromRimu;

#[derive(Debug, Clone)]
pub struct Name(pub String);

#[derive(Debug, Clone)]
pub enum IntoNameError {
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

#[derive(Debug, Clone)]
pub enum IntoVersionError {
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

#[derive(Debug, Clone)]
pub enum IntoPlanActionError {
    NotAnObject,
    ModuleMissing,
    ModuleNotAString { span: Span },
    IdNotAString { span: Span },
    Params(Spanned<ParamValuesFromRimuError>),
    BeforeNotAList { span: Span },
    BeforeItemNotAString { item_span: Span },
    AfterNotAList { span: Span },
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
                let (val, span) = sp.clone().take();
                match val {
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
                let (val, span) = sp.clone().take();
                match val {
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
            Some(sp) => {
                let (val, span) = sp.clone().take();
                match val {
                    Value::List(items) => {
                        let mut out = Vec::with_capacity(items.len());
                        for item in items {
                            let (ival, ispan) = item.clone().take();
                            match ival {
                                Value::String(s) => out.push(Spanned::new(s, ispan)),
                                _ => {
                                    return Err(IntoPlanActionError::BeforeItemNotAString {
                                        item_span: ispan,
                                    })
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
            Some(sp) => {
                let (val, span) = sp.clone().take();
                match val {
                    Value::List(items) => {
                        let mut out = Vec::with_capacity(items.len());
                        for item in items {
                            let (ival, ispan) = item.clone().take();
                            match ival {
                                Value::String(s) => out.push(Spanned::new(s, ispan)),
                                _ => {
                                    return Err(IntoPlanActionError::AfterItemNotAString {
                                        item_span: ispan,
                                    })
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

#[derive(Debug, Clone)]
pub enum IntoSetupFunctionError {
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

#[derive(Debug, Clone)]
pub enum IntoPlanError {
    NotAnObject,
    Name(Spanned<IntoNameError>),
    Version(Spanned<IntoVersionError>),
    Params(Spanned<ParamTypesFromRimuError>),
    SetupMissing,
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
