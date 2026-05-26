//! Owner: Interactive TUI subsystem — freshness chip widget (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::freshness_chip`.
//! Invariants: Pure rendering. Wraps `FreshnessBadge` from
//! `crate::tui::theme::badges` and renders a single-cell chip with optional
//! age annotation per plan §14 ("FRESH <age>", "STALE <age>").

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::theme::badges::FreshnessBadge;
use crate::tui::theme::palette::Palette;

pub struct FreshnessChipProps<'a> {
    pub badge: FreshnessBadge,
    /// Human-readable age string (e.g. "12s", "3m", "1h"). Empty hides it.
    pub age: &'a str,
}

/// Render a single freshness chip as a styled span at `area`'s top-left row.
pub fn render(f: &mut Frame, props: &FreshnessChipProps, area: Rect, palette: &Palette) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let style = props.badge.style(palette);
    let label = if props.age.is_empty() {
        format!("[{}]", props.badge.label())
    } else {
        format!("[{} {}]", props.badge.label(), props.age)
    };
    let line = Line::from(Span::styled(label, style));
    f.render_widget(Paragraph::new(line), area);
}

/// Render multiple chips inline (caller controls separator).
pub fn render_row(
    f: &mut Frame,
    chips: &[FreshnessChipProps<'_>],
    sep: &str,
    area: Rect,
    palette: &Palette,
) {
    if area.width == 0 || area.height == 0 || chips.is_empty() {
        return;
    }
    let mut spans: Vec<Span> = Vec::with_capacity(chips.len() * 2);
    for (i, c) in chips.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(sep.to_string(), Style::default()));
        }
        let label = if c.age.is_empty() {
            format!("[{}]", c.badge.label())
        } else {
            format!("[{} {}]", c.badge.label(), c.age)
        };
        spans.push(Span::styled(label, c.badge.style(palette)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let props = FreshnessChipProps {
            badge: FreshnessBadge::Fresh,
            age: "12s",
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 16, 1), &p))
            .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let chips = [
            FreshnessChipProps {
                badge: FreshnessBadge::Live,
                age: "",
            },
            FreshnessChipProps {
                badge: FreshnessBadge::Stale,
                age: "5m",
            },
            FreshnessChipProps {
                badge: FreshnessBadge::SourceDown,
                age: "",
            },
        ];
        term.draw(|f| render_row(f, &chips, " ", Rect::new(0, 0, 60, 1), &p))
            .unwrap();
    }
}
