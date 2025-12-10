use serde::{Deserialize, Serialize};
use std::fmt::Display;

use crate::TextStyle;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Span {
    pub content: String,
    pub style: TextStyle,
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

impl Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.content.fmt(f)
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
