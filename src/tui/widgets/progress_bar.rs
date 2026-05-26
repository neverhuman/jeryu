//! Owner: Interactive TUI subsystem — progress bar widget (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::progress_bar`.
//! Invariants: Pure rendering. Uses `crate::tui::theme::progress::ProgressBar`
//! for the underlying clamp + color mapping; this widget adds layout and an
//! optional confidence-band overlay.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::theme::palette::Palette;
use crate::tui::theme::progress::ProgressBar;

pub struct ProgressBarProps<'a> {
    pub label: &'a str,
    pub bar: ProgressBar,
    /// Optional confidence-band lower/upper [0.0, 1.0]. Drawn as dimmed cells
    /// overlapping the main bar.
    pub band: Option<(f64, f64)>,
}

pub fn render(f: &mut Frame, props: &ProgressBarProps, area: Rect, palette: &Palette) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let label_width = (props.label.chars().count() as u16 + 2).min(area.width);
    let bar_width = area.width.saturating_sub(label_width).saturating_sub(8);
    if bar_width == 0 {
        return;
    }

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(
        format!("{:<width$} ", props.label, width = label_width as usize - 1),
        Style::default().fg(palette.stale),
    ));

    // Bar body
    let bar_str = props.bar.render(bar_width);
    let bar_style = props.bar.style(palette);
    spans.push(Span::styled(bar_str, bar_style));

    // Percentage suffix
    let pct = (props.bar.value * 100.0).round() as u32;
    spans.push(Span::styled(
        format!(" {:>3}%", pct),
        Style::default().fg(palette.cache),
    ));

    f.render_widget(Paragraph::new(Line::from(spans)), area);

    // Confidence-band overlay (drawn on the next row if available, dimmed).
    if let Some((lo, hi)) = props.band {
        if area.height >= 2 {
            let lo = lo.clamp(0.0, 1.0);
            let hi = hi.clamp(lo, 1.0);
            let lo_cell = (lo * bar_width as f64).round() as u16;
            let hi_cell = (hi * bar_width as f64).round() as u16;
            let mut s = String::with_capacity(bar_width as usize);
            for i in 0..bar_width {
                if i >= lo_cell && i < hi_cell {
                    s.push('▔');
                } else {
                    s.push(' ');
                }
            }
            let band_area = Rect::new(
                area.x + label_width,
                area.y + 1,
                bar_width,
                1,
            );
            let band_style = Style::default().fg(palette.warn).add_modifier(Modifier::DIM);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(s, band_style))),
                band_area,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::progress::Confidence;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let bar = ProgressBar::new(0.62, Confidence::Meas);
        let props = ProgressBarProps {
            label: "build progress",
            bar,
            band: Some((0.55, 0.7)),
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 60, 2), &p))
            .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let bar = ProgressBar::new(0.95, Confidence::Heur);
        let props = ProgressBarProps {
            label: "release readiness",
            bar,
            band: None,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 100, 1), &p))
            .unwrap();
    }
}
