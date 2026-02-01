//! Status bar widget.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// Status bar severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    /// Informational.
    Info,
    /// Success.
    Success,
    /// Warning.
    Warning,
    /// Error.
    Error,
}

impl StatusLevel {
    /// Returns color for level.
    #[must_use]
    pub const fn color(self) -> Color {
        match self {
            Self::Info => Color::Cyan,
            Self::Success => Color::Green,
            Self::Warning => Color::Yellow,
            Self::Error => Color::Red,
        }
    }
}

/// Status bar widget.
#[derive(Debug, Clone)]
pub struct StatusBar {
    left: String,
    center: String,
    right: String,
    level: StatusLevel,
}

impl StatusBar {
    /// Creates empty status bar.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            left: String::new(),
            center: String::new(),
            right: String::new(),
            level: StatusLevel::Info,
        }
    }

    /// Sets left content.
    #[must_use]
    pub fn left(mut self, content: impl Into<String>) -> Self {
        self.left = content.into();
        self
    }

    /// Sets center content.
    #[must_use]
    pub fn center(mut self, content: impl Into<String>) -> Self {
        self.center = content.into();
        self
    }

    /// Sets right content.
    #[must_use]
    pub fn right(mut self, content: impl Into<String>) -> Self {
        self.right = content.into();
        self
    }

    /// Sets status level.
    #[must_use]
    pub const fn level(mut self, level: StatusLevel) -> Self {
        self.level = level;
        self
    }

    /// Creates info status bar.
    #[must_use]
    pub fn info(message: impl Into<String>) -> Self {
        Self::new().left(message).level(StatusLevel::Info)
    }

    /// Creates success status bar.
    #[must_use]
    pub fn success(message: impl Into<String>) -> Self {
        Self::new().left(message).level(StatusLevel::Success)
    }

    /// Creates warning status bar.
    #[must_use]
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new().left(message).level(StatusLevel::Warning)
    }

    /// Creates error status bar.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self::new().left(message).level(StatusLevel::Error)
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for &StatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let style = Style::default()
            .fg(self.level.color())
            .add_modifier(Modifier::BOLD);

        let width = area.width as usize;

        let left_len = self.left.len();
        let center_len = self.center.len();
        let right_len = self.right.len();

        let center_start = width.saturating_sub(center_len) / 2;
        let right_start = width.saturating_sub(right_len);

        let mut spans = Vec::new();

        spans.push(Span::styled(&self.left, style));

        let left_padding = center_start.saturating_sub(left_len);
        if left_padding > 0 {
            spans.push(Span::raw(" ".repeat(left_padding)));
        }

        if !self.center.is_empty() {
            spans.push(Span::styled(&self.center, style));
        }

        let current_len = left_len + left_padding + center_len;
        let right_padding = right_start.saturating_sub(current_len);
        if right_padding > 0 {
            spans.push(Span::raw(" ".repeat(right_padding)));
        }

        if !self.right.is_empty() {
            spans.push(Span::styled(&self.right, style));
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line);
        paragraph.render(area, buf);
    }
}
