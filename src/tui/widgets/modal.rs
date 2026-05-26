//! Owner: Interactive TUI subsystem — generic modal frame widget (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::modal`.
//! Invariants: Pure rendering. The modal is a centered Clear + bordered
//! block with three rows: title, body (caller-supplied lines), footer.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::tui::theme::palette::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalKind {
    Info,
    Warn,
    Danger,
    Proof,
}

impl ModalKind {
    fn border_color(self, p: &Palette) -> Color {
        match self {
            Self::Info => p.running,
            Self::Warn => p.warn,
            Self::Danger => p.crit,
            Self::Proof => p.evidence,
        }
    }
}

pub struct ModalProps<'a> {
    pub kind: ModalKind,
    pub title: &'a str,
    pub body: &'a [Line<'a>],
    pub footer: &'a str,
    /// Preferred (width, height). Will be clamped to fit `area`.
    pub size: (u16, u16),
}

pub fn render(f: &mut Frame, props: &ModalProps, area: Rect, palette: &Palette) {
    let w = area.width.min(props.size.0).max(20);
    let h = area.height.min(props.size.1).max(6);
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let overlay = Rect::new(x, y, w, h);
    f.render_widget(Clear, overlay);

    let block = Block::default()
        .title(format!(" {} ", props.title))
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(props.kind.border_color(palette))
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(overlay);
    f.render_widget(block, overlay);

    if inner.width < 4 || inner.height < 3 {
        return;
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    f.render_widget(
        Paragraph::new(props.body.to_vec()).wrap(Wrap { trim: true }),
        sections[0],
    );

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {} ", props.footer),
            Style::default().fg(palette.stale).add_modifier(Modifier::DIM),
        ))),
        sections[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn sample_body() -> Vec<Line<'static>> {
        vec![
            Line::from(Span::raw("  Confirm canary rollout?")),
            Line::from(Span::raw("")),
            Line::from(Span::raw("  Target: release-2026.05.26")),
            Line::from(Span::raw("  Risk:   R4 (production-adjacent)")),
        ]
    }

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let body = sample_body();
        let props = ModalProps {
            kind: ModalKind::Warn,
            title: "Confirm",
            body: &body,
            footer: "[y] confirm  [n] cancel",
            size: (60, 12),
        };
        term.draw(|f| render(f, &props, f.area(), &p)).unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let body = sample_body();
        let props = ModalProps {
            kind: ModalKind::Danger,
            title: "Destructive",
            body: &body,
            footer: "[y] confirm  [n] cancel",
            size: (72, 16),
        };
        term.draw(|f| render(f, &props, f.area(), &p)).unwrap();
    }

    #[test]
    fn modal_border_colors_are_kind_specific() {
        let p = Palette::dark();
        assert_eq!(ModalKind::Info.border_color(&p), p.running);
        assert_eq!(ModalKind::Warn.border_color(&p), p.warn);
        assert_eq!(ModalKind::Danger.border_color(&p), p.crit);
        assert_eq!(ModalKind::Proof.border_color(&p), p.evidence);
    }
}
