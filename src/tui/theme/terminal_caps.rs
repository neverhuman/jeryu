use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCaps {
    pub unicode: bool,
    pub color: bool,
}

impl TerminalCaps {
    pub const fn unicode() -> Self {
        Self {
            unicode: true,
            color: true,
        }
    }

    pub const fn ascii() -> Self {
        Self {
            unicode: false,
            color: true,
        }
    }

    pub const fn no_color() -> Self {
        Self {
            unicode: true,
            color: false,
        }
    }

    pub const fn plain() -> Self {
        Self {
            unicode: false,
            color: false,
        }
    }

    pub fn color(self, semantic: Color) -> Color {
        if self.color { semantic } else { Color::Reset }
    }
}

impl Default for TerminalCaps {
    fn default() -> Self {
        Self::unicode()
    }
}
