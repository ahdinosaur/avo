mod fragment;
mod line;
mod paragraph;
mod text;

pub use self::fragment::*;
pub use self::line::*;
pub use self::paragraph::*;
pub use self::text::*;

#[derive(Debug, Clone)]
pub enum ViewNode {
    Fragment(Fragment),
    Line(Line),
    Paragraph(Paragraph),
}
