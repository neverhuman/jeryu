//! Owner: Interactive TUI subsystem — header strip widget (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::header`.
//! Invariants: Pure rendering — no I/O, no state mutation in draw. The header
//! shows brand on the left, tab strip in the middle, worst-freshness chip and
//! stream-mode badge on the right. All inputs are pre-computed by the caller.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::theme::badges::FreshnessBadge;
use crate::tui::theme::palette::Palette;

/// Stream status indicator shown in the right slot of the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamMode {
    Live,
    Poll,
    Degraded,
    Fixture,
}

impl StreamMode {
    fn label(self) -> &'static str {
        match self {
            Self::Live => "LIVE",
            Self::Poll => "[poll]",
            Self::Degraded => "DEGRADED",
            Self::Fixture => "FIXTURE",
        }
    }

    fn style(self, p: &Palette) -> Style {
        match self {
            Self::Live => Style::default().fg(p.running).add_modifier(Modifier::BOLD),
            Self::Poll => Style::default().fg(p.stale),
            Self::Degraded => Style::default().fg(p.warn).add_modifier(Modifier::BOLD),
            Self::Fixture => Style::default().fg(p.agent).add_modifier(Modifier::ITALIC),
        }
    }
}

/// Header view-model. Every field is pre-computed by selectors.
pub struct HeaderProps<'a> {
    pub brand: &'a str,
    pub tab_labels: &'a [&'a str],
    pub active_tab: usize,
    pub worst_freshness: FreshnessBadge,
    pub stream_mode: StreamMode,
}

/// Render the header strip into `area`.
pub fn render(f: &mut Frame, props: &HeaderProps, area: Rect, palette: &Palette) {
    let block = Block::default().borders(Borders::BOTTOM);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(props.brand.len() as u16 + 2),
            Constraint::Min(0),
            Constraint::Length(28),
        ])
        .split(inner);

    // Brand
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {} ", props.brand),
            Style::default().fg(palette.ok).add_modifier(Modifier::BOLD),
        ))),
        cols[0],
    );

    // Tab strip
    let tabs: Vec<Span> = props
        .tab_labels
        .iter()
        .enumerate()
        .flat_map(|(idx, label)| {
            let active = idx == props.active_tab;
            let style = if active {
                Style::default()
                    .fg(palette.running)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(palette.stale)
            };
            vec![
                Span::styled(format!(" {label} "), style),
                Span::styled("│", Style::default().fg(palette.stale)),
            ]
        })
        .collect();
    f.render_widget(Paragraph::new(Line::from(tabs)), cols[1]);

    // Right slot: freshness chip + stream-mode badge.
    let right = Line::from(vec![
        Span::styled(
            format!(" {} ", props.worst_freshness.label()),
            props.worst_freshness.style(palette),
        ),
        Span::raw(" "),
        Span::styled(
            format!(" {} ", props.stream_mode.label()),
            props.stream_mode.style(palette),
        ),
    ]);
    f.render_widget(Paragraph::new(right), cols[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn sample<'a>(tabs: &'a [&'a str]) -> HeaderProps<'a> {
        HeaderProps {
            brand: "JeRyu",
            tab_labels: tabs,
            active_tab: 1,
            worst_freshness: FreshnessBadge::Fresh,
            stream_mode: StreamMode::Live,
        }
    }

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let tabs = ["Mission", "Queue", "Workflow"];
        term.draw(|f| {
            let area = Rect::new(0, 0, 80, 2);
            render(f, &sample(&tabs), area, &p);
        })
        .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let tabs = ["Mission", "Queue", "Workflow", "Logs", "Agents"];
        term.draw(|f| {
            let area = Rect::new(0, 0, 120, 2);
            render(f, &sample(&tabs), area, &p);
        })
        .unwrap();
    }

    #[test]
    fn stream_mode_labels_are_pinned() {
        assert_eq!(StreamMode::Live.label(), "LIVE");
        assert_eq!(StreamMode::Poll.label(), "[poll]");
        assert_eq!(StreamMode::Degraded.label(), "DEGRADED");
        assert_eq!(StreamMode::Fixture.label(), "FIXTURE");
    }
}
