use std::path::PathBuf;

use rimu::{Function, Spanned, Value};

use crate::{
    params::{ParamTypes, ParamValues},
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
// Box<dyn Fn(ParamValues) -> Spanned<Vec<BlockCallRef>>>;

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
    Params(Spanned<IntoParamsError>),
    Blocks(IntoBlocksFunctionError),
}

impl FromRimu for BlockDefinition {
    type Error = IntoBlockDefinitionError;

    fn from_rimu(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(object) = value else {
            return Err(IntoBlockDefinitionError::NotAnObject);
        };

        let name = object
            .swap_remove("name")
            .map(|name| Name::from_rimu_spanned(name).map_err(IntoBlockDefinitionError::Name))
            .transpose()?;

        let params = object
            .swap_remove("params")
            .map(|name| {
                ParamTypes::from_rimu_spanned(name).map_err(IntoBlockDefinitionError::Params)
            })
            .transpose()?;
    }
}

pub type SpannedBlockDefinition = Spanned<BlockDefinition>;
