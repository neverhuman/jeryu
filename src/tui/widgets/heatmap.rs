//! Owner: Interactive TUI subsystem — heatmap grid widget (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::heatmap`.
//! Invariants: Pure rendering. The caller passes a 2-D row-major matrix of
//! `f64` values in `[0.0, 1.0]`; out-of-range values are clamped. Color
//! ramps come from the palette — no built-in color maps to keep the file
//! small and deterministic.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::theme::palette::Palette;

/// Cell glyph ladder, low-to-high intensity.
const RAMP: &[char] = &[' ', '·', '▒', '▓', '█'];

pub struct HeatmapProps<'a> {
    pub title: &'a str,
    /// Row labels (top-to-bottom). Empty => unlabeled.
    pub row_labels: &'a [&'a str],
    /// Column labels (left-to-right). Empty => unlabeled.
    pub col_labels: &'a [&'a str],
    /// Row-major matrix. `matrix[r][c]` ∈ `[0.0, 1.0]` (clamped at render).
    pub matrix: &'a [Vec<f64>],
}

pub fn render(f: &mut Frame, props: &HeatmapProps, area: Rect, palette: &Palette) {
    let block = Block::default()
        .title(format!(" {} ", props.title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette.stale));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.width < 4 || inner.height < 2 || props.matrix.is_empty() {
        return;
    }

    let row_label_w: u16 = props
        .row_labels
        .iter()
        .map(|s| s.chars().count() as u16)
        .max()
        .unwrap_or(0)
        .min(12);
    let header_h: u16 = if props.col_labels.is_empty() { 0 } else { 1 };

    // Header line of col labels (truncated to single chars).
    if header_h == 1 {
        let mut spans: Vec<Span> = Vec::new();
        spans.push(Span::raw(" ".repeat(row_label_w as usize + 1)));
        for label in props.col_labels {
            let ch = label.chars().next().unwrap_or(' ');
            spans.push(Span::styled(
                format!("{ch} "),
                Style::default().fg(palette.cache),
            ));
        }
        let hdr_area = Rect::new(inner.x, inner.y, inner.width, 1);
        f.render_widget(Paragraph::new(Line::from(spans)), hdr_area);
    }

    // Body rows.
    let body_y = inner.y + header_h;
    let body_h = inner.height.saturating_sub(header_h);
    for (r, row) in props.matrix.iter().take(body_h as usize).enumerate() {
        let mut spans: Vec<Span> = Vec::new();
        let label = props.row_labels.get(r).copied().unwrap_or("");
        let label = truncate(label, row_label_w as usize);
        spans.push(Span::styled(
            format!("{:<width$} ", label, width = row_label_w as usize),
            Style::default().fg(palette.cache),
        ));
        for v in row {
            let clamped = v.clamp(0.0, 1.0);
            let idx = (clamped * (RAMP.len() - 1) as f64).round() as usize;
            let ch = RAMP[idx.min(RAMP.len() - 1)];
            spans.push(Span::styled(
                format!("{ch} "),
                Style::default()
                    .fg(intensity_color(clamped, palette))
                    .add_modifier(if clamped > 0.8 {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ));
        }
        let row_area = Rect::new(inner.x, body_y + r as u16, inner.width, 1);
        f.render_widget(Paragraph::new(Line::from(spans)), row_area);
    }
}

fn intensity_color(v: f64, p: &Palette) -> Color {
    match v {
        x if x < 0.2 => p.stale,
        x if x < 0.4 => p.cache,
        x if x < 0.7 => p.running,
        x if x < 0.9 => p.warn,
        _ => p.crit,
    }
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn matrix(rows: usize, cols: usize) -> Vec<Vec<f64>> {
        (0..rows)
            .map(|r| {
                (0..cols)
                    .map(|c| (r * cols + c) as f64 / (rows * cols) as f64)
                    .collect()
            })
            .collect()
    }

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let m = matrix(6, 12);
        let rows = ["build", "lint", "test", "vti", "release", "smoke"];
        let cols = ["M", "T", "W", "T", "F", "S", "S", "M", "T", "W", "T", "F"];
        let props = HeatmapProps {
            title: "Cache fullness",
            row_labels: &rows,
            col_labels: &cols,
            matrix: &m,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 80, 12), &p))
            .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let m = matrix(12, 24);
        let props = HeatmapProps {
            title: "GC heat",
            row_labels: &[],
            col_labels: &[],
            matrix: &m,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 120, 18), &p))
            .unwrap();
    }

    #[test]
    fn intensity_color_buckets() {
        let p = Palette::dark();
        assert_eq!(intensity_color(0.0, &p), p.stale);
        assert_eq!(intensity_color(0.5, &p), p.running);
        assert_eq!(intensity_color(1.0, &p), p.crit);
    }
}
