//! Owner: Interactive TUI subsystem — workflow panel helpers
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::widget`
//! Invariants: Pure rendering helpers for banners, state panels, and footer.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::model::*;
use super::nav::WorkflowNav;
use crate::tui::theme::Theme;

pub(super) fn draw_empty_state(
    f: &mut Frame,
    area: Rect,
    _snapshot: &WorkflowSnapshot,
    theme: &Theme,
) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  No active workflow",
            theme.bold(theme.text_muted),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Waiting for a VTI plan or active pipeline.",
            theme.muted(),
        )),
        Line::from(Span::styled(
            "  Run `jeryu test select` or push a commit to generate a workflow.",
            theme.muted(),
        )),
    ];

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" [ 0:Workflow ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_subtle)),
        ),
        area,
    );
}

pub(super) fn draw_no_pr_state(f: &mut Frame, area: Rect, theme: &Theme) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  No active pull requests",
            theme.bold(theme.text_muted),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Open a PR to see the full delivery flow.",
            theme.muted(),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_subtle)),
        ),
        area,
    );
}

pub(super) fn draw_delivery_footer(
    f: &mut Frame,
    area: Rect,
    _delivery: &DeliverySnapshot,
    theme: &Theme,
) {
    let hint = " ↑↓←→ move · </> PR · []/PgUp/PgDn pan · f follow · b blocker · c crit · z zoom · Enter inspect · r rollback · ? help";
    let line = Line::from(Span::styled(hint, theme.muted()));
    f.render_widget(Paragraph::new(line), area);
}

pub(super) fn draw_summary_banner(
    f: &mut Frame,
    area: Rect,
    snap: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
) {
    let s = &snap.summary;
    let overall_color = if s.error > 0 {
        theme.fail
    } else if s.running > 0 {
        theme.running
    } else if s.blocked > 0 {
        theme.blocked
    } else if s.total == s.passed + s.cached + s.skipped {
        theme.ok
    } else {
        theme.waiting
    };

    let follow_badge = if nav.follow_active { " [FOLLOW] " } else { "" };

    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("  Workflow: {} ", snap.title),
                Style::default()
                    .fg(theme.text_inverse)
                    .bg(overall_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  mode:{} ", snap.mode), theme.secondary()),
            Span::styled(
                format!("conf:{:.0}% ", snap.confidence * 100.0),
                theme.bold(theme.ok),
            ),
            Span::styled(
                format!("progress:{:.0}%", s.overall_pct),
                theme.bold(overall_color),
            ),
            Span::styled(follow_badge.to_string(), theme.bold(theme.running)),
        ]),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            status_count("✓", s.passed, theme.ok, theme),
            Span::raw("  "),
            status_count("●", s.running, theme.running, theme),
            Span::raw("  "),
            status_count("○", s.waiting, theme.waiting, theme),
            Span::raw("  "),
            status_count("✗", s.error, theme.fail, theme),
            Span::raw("  "),
            status_count("⊘", s.skipped, theme.skipped, theme),
            Span::raw("  "),
            status_count("◈", s.cached, theme.vti_fire, theme),
        ]),
    ];

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" [ 0:Workflow ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(overall_color)),
        ),
        area,
    );
}

pub(super) fn draw_viewport_indicator(f: &mut Frame, area: Rect, nav: &WorkflowNav, theme: &Theme) {
    if nav.canvas_height <= 0 || area.height == 0 {
        return;
    }

    let y_pct = if nav.canvas_height > 0 {
        ((nav.viewport_y as f64 + area.height as f64 / 2.0) / nav.canvas_height as f64 * 100.0)
            .clamp(0.0, 100.0) as u16
    } else {
        0
    };

    let x_pct = if nav.canvas_width > 0 {
        ((nav.viewport_x as f64 + area.width as f64 / 2.0) / nav.canvas_width as f64 * 100.0)
            .clamp(0.0, 100.0) as u16
    } else {
        0
    };

    let indicator = format!(" ↕{}% ↔{}% ", y_pct, x_pct);
    let iw = indicator.len() as u16;

    if area.width > iw + 2 && area.height > 1 {
        let ix = area.x + area.width - iw - 1;
        let iy = area.y + area.height - 1;
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                indicator,
                Style::default().fg(theme.text_muted).bg(theme.bg_surface),
            ))),
            Rect::new(ix, iy, iw, 1),
        );
    }
}

fn status_count<'a>(glyph: &str, count: u32, color: Color, theme: &Theme) -> Span<'a> {
    Span::styled(
        format!("{} {}", glyph, count),
        if count > 0 {
            theme.bold(color)
        } else {
            theme.muted()
        },
    )
}
