//! Owner: Interactive TUI subsystem — Delivery PR rail (horizontal chip list)
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::pr_rail`
//! Invariants: Render-only; hit-test returns the PR index under a column.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::model::*;
use crate::tui::theme::Theme;

/// Width of each PR chip including 1 column of spacing.
pub const CHIP_W: u16 = 30;
const TITLE_MAX: usize = 22;

pub fn draw_pr_rail(f: &mut Frame, area: Rect, snap: &DeliverySnapshot, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let block = Block::default()
        .title(" PRs ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_subtle));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if snap.pull_requests.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(" no active PRs", theme.muted()))),
            inner,
        );
        return;
    }

    let spans = build_chip_spans(snap, theme);
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn build_chip_spans<'a>(snap: &'a DeliverySnapshot, theme: &Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::with_capacity(snap.pull_requests.len() * 2 + 1);
    spans.push(Span::raw(" "));
    for (idx, pr) in snap.pull_requests.iter().enumerate() {
        let is_selected = idx == snap.selected_pr_idx;
        let color = color_for(pr.status, theme);

        let chip_text = format!(
            "[{} #{} {}]",
            pr.status.glyph(),
            pr.number,
            pr.short_title(TITLE_MAX)
        );
        let mut style = if is_selected {
            Style::default()
                .fg(theme.text_inverse)
                .bg(color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };
        if pr.draft {
            style = style.add_modifier(Modifier::DIM);
        }
        spans.push(Span::styled(chip_text, style));
        spans.push(Span::raw(" "));
    }
    spans
}

fn color_for(status: PrStatus, theme: &Theme) -> ratatui::style::Color {
    match status {
        PrStatus::Draft => theme.text_muted,
        PrStatus::Open => theme.text_primary,
        PrStatus::Running => theme.running,
        PrStatus::Merged => theme.ok,
        PrStatus::Blocked => theme.fail,
        PrStatus::Closed => theme.text_muted,
    }
}

/// Hit-test: given an X column relative to the rail's interior, return the
/// PR index whose chip occupies that column, if any. Used by mouse click.
pub fn pr_at_column(snap: &DeliverySnapshot, x_in_inner: u16) -> Option<usize> {
    // Layout mirrors build_chip_spans: 1 leading space, then chips separated by 1 space.
    let mut cursor: usize = 1; // leading space
    for (idx, pr) in snap.pull_requests.iter().enumerate() {
        let chip_len = chip_width(pr);
        let start = cursor;
        let end = cursor + chip_len;
        if (x_in_inner as usize) >= start && (x_in_inner as usize) < end {
            return Some(idx);
        }
        cursor = end + 1; // 1 column of spacing
    }
    None
}

fn chip_width(pr: &PullRequestView) -> usize {
    // [G #N TITLE] — 4 chars of literal plus glyph + number digits + title
    let title = pr.short_title(TITLE_MAX);
    let num_digits = pr.number.to_string().len();
    let glyph_w = pr.status.glyph().chars().count();
    // Literal: '[' + glyph + ' #' + num + ' ' + title + ']'
    1 + glyph_w + 2 + num_digits + 1 + title.chars().count() + 1
}

#[cfg(test)]
mod tests {
    use super::super::delivery::build_demo_delivery;
    use super::*;

    #[test]
    fn hit_test_returns_first_pr_for_leading_column() {
        let snap = build_demo_delivery();
        // After 1-space lead, x=1 starts the first chip; pick a few cells in.
        assert_eq!(pr_at_column(&snap, 2), Some(0));
    }

    #[test]
    fn hit_test_picks_later_pr_for_later_column() {
        let snap = build_demo_delivery();
        let pr1_w = chip_width(&snap.pull_requests[0]);
        let pr2_w = chip_width(&snap.pull_requests[1]);
        // 1 lead + pr1_w + 1 spacing → second chip starts here.
        let second_start = 1 + pr1_w + 1;
        let mid_of_second = second_start + pr2_w / 2;
        assert_eq!(pr_at_column(&snap, mid_of_second as u16), Some(1));
    }

    #[test]
    fn hit_test_off_the_end_is_none() {
        let snap = build_demo_delivery();
        assert_eq!(pr_at_column(&snap, 9999), None);
    }
}
