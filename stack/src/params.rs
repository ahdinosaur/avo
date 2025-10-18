pub enum ParamType {
    Boolean,
}

pub enum ParamValue {
    Boolean(bool),
}

pub struct ParamTypes(HashMap<String, ParamType>);
pub struct ParamValues(HashMap<String, ParamValue>);
