use std::path::PathBuf;

use rimu::{Function, Spanned, Value};

use crate::{
    params::{IntoParamTypesError, ParamTypes, ParamValues},
    FromRimu,
};

#[derive(Debug, Clone)]
pub struct Name(String);

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
pub struct BlockCallRef {
    op: Spanned<PathBuf>,
    params: Spanned<ParamValues>,
}

#[derive(Debug, Clone)]
pub struct BlocksFunction(Function);

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
    name: Option<Spanned<Name>>,
    params: Option<Spanned<ParamTypes>>,
    blocks: Spanned<BlocksFunction>,
}

#[derive(Debug, Clone)]
pub enum IntoBlockDefinitionError {
    NotAnObject,
    Name(Spanned<IntoNameError>),
    Params(Spanned<IntoParamTypesError>),
    BlocksMissing,
    Blocks(Spanned<IntoBlocksFunctionError>),
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

        let params = object
            .swap_remove("params")
            .map(|params| {
                ParamTypes::from_rimu_spanned(params).map_err(IntoBlockDefinitionError::Params)
            })
            .transpose()?;

        let blocks_sp = object
            .swap_remove("blocks")
            .ok_or(IntoBlockDefinitionError::BlocksMissing)?;

        let blocks = BlocksFunction::from_rimu_spanned(blocks_sp)
            .map_err(|error| IntoBlockDefinitionError::Blocks(error))?;

        Ok(BlockDefinition {
            name,
            params,
            blocks,
        })
    }
}

pub type SpannedBlockDefinition = Spanned<BlockDefinition>;
