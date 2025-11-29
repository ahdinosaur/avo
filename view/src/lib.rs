#[derive(Debug, Clone)]
pub enum ViewNode {
    Line(Line),
    Paragraph(Paragraph),
}

#[derive(Debug, Clone)]
pub struct Paragraph {
    pub lines: Vec<Line>,
    pub alignment: Option<Alignment>,
    pub style: Style,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub spans: Vec<Span>,
    pub style: Style,
    pub alignment: Option<Alignment>,
}

#[derive(Debug, Clone)]
pub struct Span {
    pub content: String,
    pub style: Style,
}

#[derive(Debug, Clone)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub struct Style {
    pub foreground_color: Option<Color>,
    pub background_color: Option<Color>,
    pub is_bold: bool,
    pub is_italic: bool,
    pub is_underlined: bool,
    pub underline_color: Option<Color>,
    pub is_crossed_out: bool,
}

#[derive(Debug, Clone)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
}
