use rimu::{Function, Span, Spanned, Value};

use crate::{
    params::{IntoParamTypesError, IntoParamValuesError, ParamTypes, ParamValues},
    FromRimu,
};

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
pub struct BlockCallRef {
    pub id: Option<Spanned<String>>,
    pub module: Spanned<String>,
    pub params: Option<Spanned<ParamValues>>,
    pub before: Vec<Spanned<String>>,
    pub after: Vec<Spanned<String>>,
}

#[derive(Debug, Clone)]
pub enum IntoBlockCallRefError {
    NotAnObject,
    ModuleMissing,
    ModuleNotAString { span: Span },
    IdNotAString { span: Span },
    Params(Spanned<IntoParamValuesError>),
    BeforeNotAList { span: Span },
    BeforeItemNotAString { item_span: Span },
    AfterNotAList { span: Span },
    AfterItemNotAString { item_span: Span },
}

impl FromRimu for BlockCallRef {
    type Error = IntoBlockCallRefError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(mut object) = value else {
            return Err(IntoBlockCallRefError::NotAnObject);
        };

        let module = match object.swap_remove("module") {
            Some(sp) => {
                let (val, span) = sp.clone().take();
                match val {
                    Value::String(s) => Spanned::new(s, span),
                    _ => {
                        return Err(IntoBlockCallRefError::ModuleNotAString { span });
                    }
                }
            }
            None => return Err(IntoBlockCallRefError::ModuleMissing),
        };

        let id = object
            .swap_remove("id")
            .map(|sp| {
                let (val, span) = sp.clone().take();
                match val {
                    Value::String(s) => Ok(Spanned::new(s, span)),
                    _ => Err(IntoBlockCallRefError::IdNotAString { span }),
                }
            })
            .transpose()?;

        let params = object
            .swap_remove("params")
            .map(|sp| ParamValues::from_rimu_spanned(sp).map_err(IntoBlockCallRefError::Params))
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
                                    return Err(IntoBlockCallRefError::BeforeItemNotAString {
                                        item_span: ispan,
                                    })
                                }
                            }
                        }
                        out
                    }
                    _ => return Err(IntoBlockCallRefError::BeforeNotAList { span }),
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
                                    return Err(IntoBlockCallRefError::AfterItemNotAString {
                                        item_span: ispan,
                                    })
                                }
                            }
                        }
                        out
                    }
                    _ => return Err(IntoBlockCallRefError::AfterNotAList { span }),
                }
            }
        };

        Ok(BlockCallRef {
            id,
            module,
            params,
            before,
            after,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BlocksFunction(pub Function);

#[derive(Debug, Clone)]
pub enum IntoBlocksFunctionError {
    NotAFunction,
}

impl FromRimu for BlocksFunction {
    type Error = IntoBlocksFunctionError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Function(func) = value else {
            return Err(IntoBlocksFunctionError::NotAFunction);
        };
        Ok(BlocksFunction(func))
    }
}

#[derive(Debug, Clone)]
pub struct BlockDefinition {
    pub name: Option<Spanned<Name>>,
    pub version: Option<Spanned<Version>>,
    pub params: Option<Spanned<ParamTypes>>,
    /// setup: (params, system) => list of BlockCallRef
    pub setup: Spanned<BlocksFunction>,
}

#[derive(Debug, Clone)]
pub enum IntoBlockDefinitionError {
    NotAnObject,
    Name(Spanned<IntoNameError>),
    Version(Spanned<IntoVersionError>),
    Params(Spanned<IntoParamTypesError>),
    SetupMissing,
    SetupNotAFunction(Spanned<IntoBlocksFunctionError>),
}

impl FromRimu for BlockDefinition {
    type Error = IntoBlockDefinitionError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(mut object) = value else {
            return Err(IntoBlockDefinitionError::NotAnObject);
        };

        let name = object
            .swap_remove("name")
            .map(|name| Name::from_rimu_spanned(name).map_err(IntoBlockDefinitionError::Name))
            .transpose()?;

        let version = object
            .swap_remove("version")
            .map(|v| Version::from_rimu_spanned(v).map_err(IntoBlockDefinitionError::Version))
            .transpose()?;

        let params = object
            .swap_remove("params")
            .map(|params| {
                ParamTypes::from_rimu_spanned(params).map_err(IntoBlockDefinitionError::Params)
            })
            .transpose()?;

        let setup_sp = object
            .swap_remove("setup")
            .ok_or(IntoBlockDefinitionError::SetupMissing)?;
        let setup = BlocksFunction::from_rimu_spanned(setup_sp)
            .map_err(IntoBlockDefinitionError::SetupNotAFunction)?;

        Ok(BlockDefinition {
            name,
            version,
            params,
            setup,
        })
    }
}

pub type SpannedBlockDefinition = Spanned<BlockDefinition>;
