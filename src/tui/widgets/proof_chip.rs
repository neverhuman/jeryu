//! Owner: Interactive TUI subsystem — proof chip widget (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::proof_chip`.
//! Invariants: Pure rendering. Combines a `ProofBadge` from theme::badges
//! with a short, clickable-style proof ID (e.g. "proof:abc123"). The chip
//! is intended for use inline within inspector cards or attention rows.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::theme::badges::ProofBadge;
use crate::tui::theme::palette::Palette;

pub struct ProofChipProps<'a> {
    pub badge: ProofBadge,
    /// Display ID — typically the abbreviated proof reference (e.g. last 8 chars).
    pub id: &'a str,
    /// If false, the underline modifier is omitted (used in read-only contexts).
    pub linked: bool,
}

pub fn render(f: &mut Frame, props: &ProofChipProps, area: Rect, palette: &Palette) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let badge_style = props.badge.style(palette);
    let mut id_style = Style::default().fg(palette.evidence);
    if props.linked {
        id_style = id_style.add_modifier(Modifier::UNDERLINED);
    }
    let line = Line::from(vec![
        Span::styled(format!("[{}]", props.badge.label()), badge_style),
        Span::styled(" ", Style::default()),
        Span::styled(format!("proof:{}", props.id), id_style),
    ]);
    f.render_widget(Paragraph::new(line), area);
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
        let props = ProofChipProps {
            badge: ProofBadge::Meas,
            id: "f3a9b1c2",
            linked: true,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 30, 1), &p))
            .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let props = ProofChipProps {
            badge: ProofBadge::Miss,
            id: "—",
            linked: false,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 30, 1), &p))
            .unwrap();
    }
}
