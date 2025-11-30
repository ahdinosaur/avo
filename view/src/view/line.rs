use crate::{Alignment, Span, TextStyle, ViewNode};

#[derive(Debug, Clone)]
pub struct Line {
    pub spans: Vec<Span>,
    pub style: TextStyle,
    pub alignment: Option<Alignment>,
}

impl Line {
    /// Create a new `Line` with given spans and default style/alignment.
    pub fn new<S: Into<Vec<Span>>>(spans: S) -> Self {
        Self {
            spans: spans.into(),
            style: TextStyle::default(),
            alignment: None,
        }
    }

    /// Create a `Line` with a specific style.
    pub fn new_styled<S: Into<Vec<Span>>>(spans: S, style: TextStyle) -> Self {
        Self {
            spans: spans.into(),
            style,
            alignment: None,
        }
    }

    /// Set the style for the line in a builder pattern.
    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// Set the alignment for the line in a builder pattern.
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = Some(alignment);
        self
    }

    /// Add a span to the existing spans.
    pub fn push_span(mut self, span: Span) -> Self {
        self.spans.push(span);
        self
    }
}

impl From<Vec<Span>> for Line {
    fn from(value: Vec<Span>) -> Self {
        Line::new(value)
    }
}

impl From<Span> for Line {
    fn from(value: Span) -> Self {
        Line::new(vec![value])
    }
}

impl From<&str> for Line {
    fn from(value: &str) -> Self {
        Line::new(vec![Span::from(value)])
    }
}

impl From<String> for Line {
    fn from(value: String) -> Self {
        Line::new(vec![Span::from(value)])
    }
}

impl From<Line> for ViewNode {
    fn from(value: Line) -> Self {
        ViewNode::Line(value)
    }
}
