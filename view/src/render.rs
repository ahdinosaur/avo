use crate::ViewNode;

pub trait Render {
    fn render(&self) -> impl Into<ViewNode>;
}
