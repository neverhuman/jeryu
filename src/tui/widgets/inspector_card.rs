//! Owner: Interactive TUI subsystem — inspector card widget (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::inspector_card`.
//! Invariants: Pure rendering. The card is a generic right-pane container —
//! label + key/value lines + action chips. Domain-specific inspectors compose
//! it; this widget owns layout only.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::tui::theme::palette::Palette;

#[derive(Debug, Clone, Copy)]
pub struct KeyValue<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct ActionChip<'a> {
    pub key: &'a str,
    pub label: &'a str,
}

pub struct InspectorCardProps<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub fields: &'a [KeyValue<'a>],
    pub actions: &'a [ActionChip<'a>],
}

pub fn render(f: &mut Frame, props: &InspectorCardProps, area: Rect, palette: &Palette) {
    let block = Block::default()
        .title(format!(" [ {} ] ", props.title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette.running));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.width < 4 || inner.height < 2 {
        return;
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(inner);

    // Subtitle
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {} ", props.subtitle),
            Style::default()
                .fg(palette.stale)
                .add_modifier(Modifier::ITALIC),
        ))),
        sections[0],
    );

    // Fields
    let lines: Vec<Line> = props
        .fields
        .iter()
        .map(|f| {
            Line::from(vec![
                Span::styled(
                    format!("  {:<14}", f.key),
                    Style::default().fg(palette.cache),
                ),
                Span::styled(f.value.to_string(), Style::default().fg(palette.running)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), sections[1]);

    // Action chips
    let mut chips: Vec<Span> = Vec::new();
    for (i, a) in props.actions.iter().enumerate() {
        if i > 0 {
            chips.push(Span::raw("  "));
        }
        chips.push(Span::styled(
            format!("[{}]", a.key),
            Style::default()
                .fg(palette.warn)
                .add_modifier(Modifier::BOLD),
        ));
        chips.push(Span::styled(
            format!(" {}", a.label),
            Style::default().fg(palette.stale),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(chips)), sections[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn sample() -> InspectorCardProps<'static> {
        InspectorCardProps {
            title: "JOB 14445",
            subtitle: "build-main · pipeline 84902",
            fields: &[
                KeyValue {
                    key: "State",
                    value: "running",
                },
                KeyValue {
                    key: "Runner",
                    value: "runner-prod-7",
                },
                KeyValue {
                    key: "Duration",
                    value: "4m 22s",
                },
            ],
            actions: &[
                ActionChip {
                    key: "Enter",
                    label: "open logs",
                },
                ActionChip {
                    key: "x",
                    label: "cancel",
                },
            ],
        }
    }

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        term.draw(|f| render(f, &sample(), Rect::new(0, 0, 40, 18), &p))
            .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        term.draw(|f| render(f, &sample(), Rect::new(0, 0, 60, 30), &p))
            .unwrap();
    }
}
