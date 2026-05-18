//! Owner: Interactive TUI subsystem — VTI proof and test intelligence view
//! Proof: `cargo nextest run -p jeryu -- tui::widgets::vti_proof`
//! Invariants: VTI view is read-only; it renders from TestPlanView and VtiStatus.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::api::snapshot::{TestPlanView, TestSelection, ValidationDecision};
use crate::tui::theme::Theme;

/// Render the full VTI proof view — called from the Tests tab.
pub fn render_vti_proof(f: &mut Frame, area: Rect, plan: &TestPlanView, theme: &Theme) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Summary banner
            Constraint::Length(6), // Stats gauges
            Constraint::Min(8),    // Test lists
        ])
        .split(area);

    // ── Summary Banner ──────────────────────────────────────────────
    let decision_color = match plan.decision {
        ValidationDecision::Valid => theme.ok,
        ValidationDecision::Invalid => theme.fail,
        ValidationDecision::Escalate => theme.warning,
        ValidationDecision::Unknown => theme.text_muted,
    };

    let banner_lines = vec![
        Line::from(vec![
            Span::styled(
                format!("  VTI Decision: {} ", plan.decision.label()),
                Style::default()
                    .fg(theme.text_inverse)
                    .bg(decision_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  Confidence: {:.0}%", plan.confidence * 100.0),
                theme.bold(decision_color),
            ),
            Span::styled(
                format!("  Time saved: {}s", plan.total_time_saved_secs),
                Style::default().fg(theme.vti_fire),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Ref: ", theme.muted()),
            Span::styled(&plan.ref_name, theme.secondary()),
            Span::styled("  Base: ", theme.muted()),
            Span::styled(
                plan.base_sha.get(..8).unwrap_or(&plan.base_sha),
                theme.secondary(),
            ),
            Span::styled(" → ", theme.muted()),
            Span::styled(
                plan.head_sha.get(..8).unwrap_or(&plan.head_sha),
                theme.primary(),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  Changed files: {}", plan.changed_files.len()),
                theme.secondary(),
            ),
            if plan.global_invalidators_touched {
                Span::styled("  ⚠ GLOBAL INVALIDATOR TOUCHED", theme.bold(theme.warning))
            } else {
                Span::raw("")
            },
        ]),
    ];

    f.render_widget(
        Paragraph::new(banner_lines).block(
            Block::default()
                .title(" [ VTI Test Intelligence ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(decision_color)),
        ),
        rows[0],
    );

    // ── Stats Gauges ────────────────────────────────────────────────
    let gauge_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(rows[1]);

    let total = plan.selected_tests.len() + plan.skipped_tests.len() + plan.accelerated_tests.len();
    let total_f = total.max(1) as f64;

    render_stat_tile(
        f,
        gauge_cols[0],
        "Selected",
        &format!("{}", plan.selected_tests.len()),
        plan.selected_tests.len() as f64 / total_f,
        theme.ok,
        theme,
    );
    render_stat_tile(
        f,
        gauge_cols[1],
        "Skipped",
        &format!("{}", plan.skipped_tests.len()),
        plan.skipped_tests.len() as f64 / total_f,
        theme.skipped,
        theme,
    );
    render_stat_tile(
        f,
        gauge_cols[2],
        "🔥 Accelerated",
        &format!("{}", plan.accelerated_tests.len()),
        plan.accelerated_tests.len() as f64 / total_f,
        theme.vti_fire,
        theme,
    );
    render_stat_tile(
        f,
        gauge_cols[3],
        "Miss Rate",
        &format!(
            "24h:{} 7d:{}",
            plan.selector_misses_24h, plan.selector_misses_7d
        ),
        plan.selector_misses_24h as f64 / 100.0,
        if plan.selector_misses_24h > 5 {
            theme.fail
        } else {
            theme.ok
        },
        theme,
    );

    // ── Test Lists ──────────────────────────────────────────────────
    let list_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40), // Selected
            Constraint::Percentage(30), // Skipped
            Constraint::Percentage(30), // Accelerated
        ])
        .split(rows[2]);

    render_test_list(
        f,
        list_cols[0],
        "Selected Tests",
        &plan.selected_tests,
        "[SEL]",
        theme.ok,
        theme,
    );
    render_test_list(
        f,
        list_cols[1],
        "Skipped Tests",
        &plan.skipped_tests,
        "[SKIP]",
        theme.skipped,
        theme,
    );
    render_test_list(
        f,
        list_cols[2],
        "🔥 Accelerated",
        &plan.accelerated_tests,
        "[🔥]",
        theme.vti_fire,
        theme,
    );
}

fn render_stat_tile(
    f: &mut Frame,
    area: Rect,
    title: &str,
    value: &str,
    ratio: f64,
    color: Color,
    theme: &Theme,
) {
    let _pct = (ratio * 100.0).min(100.0) as u16;
    let lines = vec![
        Line::from(Span::styled(format!("  {}", value), theme.bold(color))),
        Line::from(Span::styled(
            format!(
                "  {}",
                crate::tui::widgets::sparkline::spark_str(&[ratio * 100.0], 1,)
            ),
            Style::default().fg(color),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color)),
        ),
        area,
    );
}

fn render_test_list(
    f: &mut Frame,
    area: Rect,
    title: &str,
    tests: &[TestSelection],
    badge: &str,
    color: Color,
    theme: &Theme,
) {
    let block = Block::default()
        .title(format!(" {} ({}) ", title, tests.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if tests.is_empty() {
        f.render_widget(Paragraph::new(Span::styled("  —", theme.muted())), inner);
        return;
    }

    let max = inner.height as usize;
    let lines: Vec<Line> = tests
        .iter()
        .take(max)
        .map(|t| {
            let conf = format!("{:.0}%", t.confidence * 100.0);
            let flake = match t.flake_probability {
                Some(f) => format!(" flk:{:.0}%", f * 100.0),
                None => String::new(),
            };
            let dur = match t.estimated_duration_secs {
                Some(d) => format!(" ~{}s", d),
                None => String::new(),
            };
            Line::from(vec![
                Span::styled(format!(" {} ", badge), theme.bold(color)),
                Span::styled(
                    super::truncate_label(&t.test_name, inner.width.saturating_sub(20) as usize),
                    theme.primary(),
                ),
                Span::styled(format!(" {}{}{}", conf, dur, flake), theme.muted()),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_plan_renders_cleanly() {
        let plan = TestPlanView::default();
        assert_eq!(plan.decision, ValidationDecision::Unknown);
        assert!(plan.selected_tests.is_empty());
    }
}
