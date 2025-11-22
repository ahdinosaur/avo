#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceId(pub String);

impl ResourceId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
}

#[derive(Debug, Clone)]
pub enum ResourceSpec {
    Apt(super::resources::apt::AptSpec),
}

#[derive(Debug, Clone)]
pub enum ResourceTree {
    Branch {
        id: Option<ResourceId>,
        before: Vec<ResourceId>,
        after: Vec<ResourceId>,
        children: Vec<ResourceTree>,
    },
    Leaf {
        id: Option<ResourceId>,
        resource: ResourceSpec,
        before: Vec<ResourceId>,
        after: Vec<ResourceId>,
    },
}
