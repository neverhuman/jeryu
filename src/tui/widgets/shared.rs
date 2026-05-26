//! Shared pure widgets for Flight Deck lenses.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::tui::theme::{Badge, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalSize {
    Compact,
    Standard,
    Wide,
}

impl CanonicalSize {
    pub const fn area(self) -> Rect {
        match self {
            Self::Compact => Rect::new(0, 0, 32, 7),
            Self::Standard => Rect::new(0, 0, 64, 10),
            Self::Wide => Rect::new(0, 0, 96, 14),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SharedChrome {
    pub border: Style,
    pub body: Style,
    pub value: Style,
}

impl SharedChrome {
    pub fn from_theme(theme: &Theme, active: bool) -> Self {
        Self {
            border: theme.border_style_for(active),
            body: theme.primary(),
            value: theme.bold(theme.border_accent),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Panel<'a> {
    title: &'a str,
    badge: Option<Badge>,
    lines: Vec<Line<'a>>,
    empty: &'a str,
    chrome: SharedChrome,
}

impl<'a> Panel<'a> {
    pub fn new(title: &'a str, theme: &Theme) -> Self {
        Self {
            title,
            badge: None,
            lines: Vec::new(),
            empty: "No data",
            chrome: SharedChrome::from_theme(theme, false),
        }
    }

    pub fn active(mut self, active: bool, theme: &Theme) -> Self {
        self.chrome = SharedChrome::from_theme(theme, active);
        self
    }

    pub fn badge(mut self, badge: Badge) -> Self {
        self.badge = Some(badge);
        self
    }

    pub fn empty(mut self, empty: &'a str) -> Self {
        self.empty = empty;
        self
    }

    pub fn line(mut self, line: impl Into<Line<'a>>) -> Self {
        self.lines.push(line.into());
        self
    }
}

impl Widget for Panel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(self.title())
            .borders(Borders::ALL)
            .border_style(self.chrome.border);
        let inner = block.inner(area);
        block.render(area, buf);

        let lines = if self.lines.is_empty() {
            vec![Line::from(Span::styled(
                format!("  {}", self.empty),
                self.chrome.body,
            ))]
        } else {
            self.lines
        };
        Paragraph::new(lines)
            .style(self.chrome.body)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }
}

impl Panel<'_> {
    fn title(&self) -> String {
        match &self.badge {
            Some(badge) => format!(" [ {} {} ] ", self.title, badge.label),
            None => format!(" [ {} ] ", self.title),
        }
    }
}

#[derive(Debug, Clone)]
pub struct KeyValueRows<'a> {
    title: &'a str,
    rows: Vec<(&'a str, &'a str)>,
    badge: Option<Badge>,
    chrome: SharedChrome,
}

impl<'a> KeyValueRows<'a> {
    pub fn new(title: &'a str, theme: &Theme) -> Self {
        Self {
            title,
            rows: Vec::new(),
            badge: None,
            chrome: SharedChrome::from_theme(theme, false),
        }
    }

    pub fn badge(mut self, badge: Badge) -> Self {
        self.badge = Some(badge);
        self
    }

    pub fn row(mut self, key: &'a str, value: &'a str) -> Self {
        self.rows.push((key, value));
        self
    }
}

impl Widget for KeyValueRows<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let chrome = self.chrome;
        let lines = self
            .rows
            .into_iter()
            .map(|(key, value)| {
                Line::from(vec![
                    Span::styled(format!("  {key:<14}"), chrome.body),
                    Span::styled(value.to_string(), chrome.value),
                ])
            })
            .collect();
        let panel = Panel {
            title: self.title,
            badge: self.badge,
            lines,
            empty: "No rows",
            chrome,
        };
        panel.render(area, buf);
    }
}
