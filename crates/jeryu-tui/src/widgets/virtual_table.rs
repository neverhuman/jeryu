//! Virtualized table widget (faithful port).
//!
//! Invariants: pure rendering. The widget never owns rows — the caller passes a
//! slice already trimmed to the visible window, plus the absolute total so the
//! scrollbar can be drawn correctly. This keeps 10k-row lists O(visible).

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::theme::palette::Palette;

/// Column descriptor. `width` is the absolute character budget.
#[derive(Debug, Clone, Copy)]
pub struct Column<'a> {
    pub title: &'a str,
    pub width: u16,
}

/// One rendered row.
#[derive(Debug, Clone)]
pub struct Row<'a> {
    pub cells: Vec<&'a str>,
}

impl<'a> Row<'a> {
    pub fn new(cells: Vec<&'a str>) -> Self {
        Self { cells }
    }
}

/// Virtualized table view-model.
pub struct VirtualTableProps<'a> {
    pub title: &'a str,
    pub columns: &'a [Column<'a>],
    /// Rows in the current viewport, already trimmed to `visible_rows`.
    pub visible_rows: &'a [Row<'a>],
    /// Absolute count of all rows (for scrollbar math).
    pub total_rows: usize,
    /// Index of selected row within `visible_rows` (None = no selection).
    pub selected: Option<usize>,
    /// Absolute index of the first visible row (top of viewport).
    pub viewport_top: usize,
}

pub fn render(f: &mut Frame, props: &VirtualTableProps, area: Rect, palette: &Palette) {
    let block = Block::default()
        .title(format!(
            " {} ({}/{}) ",
            props.title,
            props.visible_rows.len(),
            props.total_rows
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette.stale));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.width < 4 || inner.height < 2 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    // Header
    let header_spans: Vec<Span> = props
        .columns
        .iter()
        .map(|c| {
            Span::styled(
                format!(
                    "{:<width$}",
                    truncate(c.title, c.width as usize),
                    width = c.width as usize
                ),
                Style::default()
                    .fg(palette.cache)
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect();
    f.render_widget(Paragraph::new(Line::from(header_spans)), rows[0]);

    // Body
    let max_rows = rows[1].height as usize;
    let mut body_lines: Vec<Line> = Vec::with_capacity(max_rows);
    for (idx, row) in props.visible_rows.iter().take(max_rows).enumerate() {
        let selected = props.selected == Some(idx);
        let style = if selected {
            Style::default()
                .fg(palette.running)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else if (props.viewport_top + idx) % 2 == 1 {
            Style::default().fg(palette.stale)
        } else {
            Style::default()
        };
        let spans: Vec<Span> = props
            .columns
            .iter()
            .enumerate()
            .map(|(ci, col)| {
                let cell = row.cells.get(ci).copied().unwrap_or("");
                Span::styled(
                    format!(
                        "{:<width$}",
                        truncate(cell, col.width as usize),
                        width = col.width as usize
                    ),
                    style,
                )
            })
            .collect();
        body_lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(body_lines), rows[1]);
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else if max > 1 {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    } else {
        s.chars().take(max).collect()
    }
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
        let cols = [
            Column {
                title: "ID",
                width: 8,
            },
            Column {
                title: "Name",
                width: 24,
            },
            Column {
                title: "State",
                width: 16,
            },
        ];
        let rows = vec![
            Row::new(vec!["123", "build-main", "running"]),
            Row::new(vec!["124", "lint-strict", "passed"]),
            Row::new(vec!["125", "release-canary", "blocked"]),
        ];
        let props = VirtualTableProps {
            title: "Jobs",
            columns: &cols,
            visible_rows: &rows,
            total_rows: 10_000,
            selected: Some(1),
            viewport_top: 42,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 80, 12), &p))
            .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let cols = [
            Column {
                title: "ID",
                width: 8,
            },
            Column {
                title: "Name",
                width: 40,
            },
            Column {
                title: "State",
                width: 20,
            },
            Column {
                title: "Age",
                width: 10,
            },
        ];
        let rows: Vec<Row> = (0..20)
            .map(|_| Row::new(vec!["—", "—", "—", "—"]))
            .collect();
        let props = VirtualTableProps {
            title: "Jobs",
            columns: &cols,
            visible_rows: &rows,
            total_rows: 9_999,
            selected: None,
            viewport_top: 0,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 120, 30), &p))
            .unwrap();
    }

    #[test]
    fn truncate_marks_overflow_with_ellipsis() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 6), "hello…");
        assert_eq!(truncate("", 4), "");
        assert_eq!(truncate("x", 0), "");
    }
}
