mod fragment;
mod line;
mod paragraph;
mod text;
mod tree;

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
