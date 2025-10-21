use std::path::PathBuf;

use rimu::{Function, Spanned, Value};

use crate::params::{ParamTypes, ParamValues};

pub struct Name(String);

pub enum IntoNameError {
    NotAString,
}

impl TryFrom<Value> for Name {
    type Error = IntoNameError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let Value::String(string) = value else {
            return Err(IntoNameError::NotAString);
        };
        Ok(Name(string))
    }
}

pub struct BlockCallRef {
    op: Spanned<PathBuf>,
    params: Spanned<ParamValues>,
}

pub struct BlocksFunction(Function);
// Box<dyn Fn(ParamValues) -> Spanned<Vec<BlockCallRef>>>;

pub struct BlockDefinition {
    name: Option<Spanned<Name>>,
    params: Option<Spanned<ParamTypes>>,
    blocks: Spanned<BlocksFunction>,
}

pub enum IntoBlockDefinitionError {
    NotAnObject,
    Name(IntoNameError),
    Params(IntoParamsError),
    Blocks(IntoBlocksFunctionError),
}

impl TryFrom<Value> for BlockDefinition {
    type Error = IntoBlockDefinitionError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let Value::Object(object) = value else {
            return Err(IntoBlockDefinitionError::NotAnObject);
        };

        let name = object
            .get("name")
            .map(|name| {
                let (name, name_span) = name.take();
                let name: Name = name.try_into().map_err(IntoBlockDefinitionError::Name)?;
                Ok(Spanned::new(name, name_span))
            })
            .transpose()?;
    }
}

pub type SpannedBlockDefinition = Spanned<BlockDefinition>;
