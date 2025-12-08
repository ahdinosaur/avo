use std::fmt::Display;

use crate::View;

pub trait Render {
    fn render(&self) -> View;
}

impl<T> Render for T
where
    T: Display,
{
    fn render(&self) -> View {
        View::Line(self.to_string().into())
    }
}
