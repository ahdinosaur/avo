use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TextStyle {
    pub foreground_color: Option<Color>,
    pub background_color: Option<Color>,
    pub is_bold: bool,
    pub is_italic: bool,
    pub is_underlined: bool,
    pub underline_color: Option<Color>,
    pub is_crossed_out: bool,
}

impl TextStyle {
    /// Begin a new default TextStyle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set foreground color.
    pub fn fg(mut self, color: Color) -> Self {
        self.foreground_color = Some(color);
        self
    }

    /// Set background color.
    pub fn bg(mut self, color: Color) -> Self {
        self.background_color = Some(color);
        self
    }

    /// Make text bold.
    pub fn bold(mut self) -> Self {
        self.is_bold = true;
        self
    }

    /// Make text italic.
    pub fn italic(mut self) -> Self {
        self.is_italic = true;
        self
    }

    /// Underline the text.
    pub fn underline(mut self) -> Self {
        self.is_underlined = true;
        self
    }

    /// Set underline color.
    pub fn underline_color(mut self, color: Color) -> Self {
        self.underline_color = Some(color);
        self
    }

    /// Cross out the text.
    pub fn crossed_out(mut self) -> Self {
        self.is_crossed_out = true;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
