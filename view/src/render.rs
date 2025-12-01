use crate::ViewNode;

pub trait Render {
    fn render(&self) -> ViewNode;
}
