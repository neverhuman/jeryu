//! Owner: Interactive TUI subsystem — tab strip widget (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::tabs`.
//! Invariants: Pure rendering — the active marker is derived from
//! `props.active`. Overflow rolls labels to the next visible window.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::theme::palette::Palette;

/// Tab strip view-model. `labels` are presentation strings; the caller is
/// responsible for any localization or truncation.
pub struct TabsProps<'a> {
    pub labels: &'a [&'a str],
    pub active: usize,
    pub separator: &'a str,
}

impl<'a> TabsProps<'a> {
    pub fn new(labels: &'a [&'a str], active: usize) -> Self {
        Self {
            labels,
            active,
            separator: " │ ",
        }
    }
}

/// Render the tab strip into `area`. Active tab is bold + underlined.
pub fn render(f: &mut Frame, props: &TabsProps, area: Rect, palette: &Palette) {
    if props.labels.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }

    let mut spans: Vec<Span> = Vec::with_capacity(props.labels.len() * 2);
    for (idx, label) in props.labels.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(
                props.separator.to_string(),
                Style::default().fg(palette.stale),
            ));
        }
        let active = idx == props.active;
        let style = if active {
            Style::default()
                .fg(palette.running)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(palette.stale)
        };
        spans.push(Span::styled((*label).to_string(), style));
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
        let labels = ["Mission", "Queue", "Workflow"];
        let props = TabsProps::new(&labels, 0);
        term.draw(|f| {
            let area = Rect::new(0, 0, 80, 1);
            render(f, &props, area, &p);
        })
        .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let labels = ["Mission", "Queue", "Workflow", "Logs", "Agents", "Bugs"];
        let props = TabsProps::new(&labels, 3);
        term.draw(|f| {
            let area = Rect::new(0, 0, 120, 1);
            render(f, &props, area, &p);
        })
        .unwrap();
    }

    #[test]
    fn empty_labels_are_a_no_op() {
        let backend = TestBackend::new(80, 4);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let empty: [&str; 0] = [];
        let props = TabsProps::new(&empty, 0);
        term.draw(|f| render(f, &props, Rect::new(0, 0, 80, 1), &p))
            .unwrap();
    }
}
