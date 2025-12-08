mod fragment;
mod line;
mod paragraph;
mod text;

use std::fmt::Debug;
use std::fmt::Display;

use serde::Deserialize;
use serde::Serialize;

pub use self::fragment::*;
pub use self::line::*;
pub use self::paragraph::*;
pub use self::text::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum View {
    Fragment(Fragment),
    Line(Line),
    Paragraph(Paragraph),
}

impl Display for View {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            View::Fragment(_fragment) => todo!(),
            View::Line(_line) => todo!(),
            View::Paragraph(_paragraph) => todo!(),
        }
    }
}
