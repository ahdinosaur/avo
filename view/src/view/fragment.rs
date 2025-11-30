use crate::ViewNode;

#[derive(Debug, Clone)]
pub struct Fragment {
    pub children: Vec<ViewNode>,
}

impl Fragment {
    fn new(children: Vec<ViewNode>) -> Self {
        Self { children }
    }
}

impl From<Vec<ViewNode>> for Fragment {
    fn from(value: Vec<ViewNode>) -> Self {
        Fragment::new(value)
    }
}

impl From<Fragment> for ViewNode {
    fn from(value: Fragment) -> Self {
        ViewNode::Fragment(value)
    }
}

impl From<Vec<ViewNode>> for ViewNode {
    fn from(value: Vec<ViewNode>) -> Self {
        ViewNode::Fragment(value.into())
    }
}
