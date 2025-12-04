use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub content: String,
    pub style: TextStyle,
}

impl Default for Span {
    fn default() -> Self {
        Self {
            content: String::new(),
            style: TextStyle::default(),
        }
    }
}

impl Span {
    /// Create a new Span with given content and default style.
    pub fn new<T: Into<String>>(content: T) -> Self {
        Self {
            content: content.into(),
            ..Default::default()
        }
    }

    /// Create a new Span with given content and style.
    pub fn new_styled<T: Into<String>>(content: T, style: TextStyle) -> Self {
        Self {
            content: content.into(),
            style,
        }
    }

    /// Set the style and return a new Span.
    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }
}

impl From<&str> for Span {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Span {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextStyle {
    pub foreground_color: Option<Color>,
    pub background_color: Option<Color>,
    pub is_bold: bool,
    pub is_italic: bool,
    pub is_underlined: bool,
    pub underline_color: Option<Color>,
    pub is_crossed_out: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            foreground_color: None,
            background_color: None,
            is_bold: false,
            is_italic: false,
            is_underlined: false,
            underline_color: None,
            is_crossed_out: false,
        }
    }
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
