mod fragment;
mod line;
mod paragraph;
mod text;
mod tree;

use std::fmt::Debug;
use std::fmt::Display;

use serde::Deserialize;
use serde::Serialize;

pub use self::fragment::*;
pub use self::line::*;
pub use self::paragraph::*;
pub use self::text::*;
pub use self::tree::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewNode {
    Fragment(Fragment),
    Line(Line),
    Paragraph(Paragraph),
    Tree(Tree),
}

impl Display for ViewNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewNode::Fragment(fragment) => todo!(),
            ViewNode::Line(line) => todo!(),
            ViewNode::Paragraph(paragraph) => todo!(),
            ViewNode::Tree(tree) => Display::fmt(tree, f),
        }
    }
}
