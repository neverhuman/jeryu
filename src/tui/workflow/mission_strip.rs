//! Owner: Interactive TUI subsystem — Delivery mission strip
//! Proof: rendered indirectly by `cargo nextest run -p jeryu -- tui::workflow`
//! Invariants: Render-only; no state mutation.
//!
//! The persistent banner that answers "what's shipping right now?".
//! Two lines: the selected PR's identity + current blocker/critical info,
//! and a fleet rollup across all open PRs.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::intelligence::{
    compute_critical_path, compute_downstream_impact, compute_first_blocker, compute_ship_readiness,
};
use super::model::*;
use crate::tui::theme::Theme;

pub fn draw_mission_strip(f: &mut Frame, area: Rect, snap: &DeliverySnapshot, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let banner_color = banner_color_for(snap, theme);
    let lines = build_lines(snap, theme, banner_color);

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" [ 0:Delivery — CI Mission Control ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(banner_color)),
        ),
        area,
    );
}

fn banner_color_for(snap: &DeliverySnapshot, theme: &Theme) -> ratatui::style::Color {
    let f = &snap.fleet_summary;
    if f.blocked > 0 {
        theme.fail
    } else if f.prod_in_flight {
        theme.running
    } else if f.canary_in_flight {
        theme.vti_fire
    } else if f.running > 0 {
        theme.running
    } else if f.open_prs == 0 {
        theme.waiting
    } else {
        theme.ok
    }
}

fn build_lines<'a>(
    snap: &'a DeliverySnapshot,
    theme: &Theme,
    banner_color: ratatui::style::Color,
) -> Vec<Line<'a>> {
    let mut lines = Vec::with_capacity(2);

    // Line 1: selected PR identity + ship readiness + blocker + critical path.
    if let Some(pr) = snap.selected() {
        let title = pr.short_title(60);
        let phase_label = pr.phase.title();
        let status_glyph = pr.status.glyph();
        let status_label = pr.status.label();
        let ship_pct = compute_ship_readiness(&pr.snapshot);
        let mut spans = vec![
            Span::styled(
                format!(" {} PR #{} ", status_glyph, pr.number),
                Style::default()
                    .fg(theme.text_inverse)
                    .bg(banner_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(title, theme.bold(theme.text_primary)),
            Span::raw("  ·  "),
            Span::styled(format!("by {}", pr.author), theme.muted()),
            Span::raw("  ·  "),
            Span::styled(status_label.to_string(), theme.bold(banner_color)),
            Span::raw("  ·  "),
            Span::styled(format!("at {}", phase_label), theme.secondary()),
            Span::raw("  ·  "),
            Span::styled(format!("ship {:.0}%", ship_pct), theme.bold(banner_color)),
        ];
        // Blocker (intelligence): label + downstream count.
        if let Some(blocker) = compute_first_blocker(&pr.snapshot) {
            let downstream = compute_downstream_impact(&pr.snapshot, &blocker.id);
            spans.push(Span::raw("  ·  "));
            spans.push(Span::styled(
                format!(
                    "blocker: {} (blocks {})",
                    blocker.label.chars().take(28).collect::<String>(),
                    downstream
                ),
                theme.bold(theme.fail),
            ));
        }
        // Critical-path tail readout (only when work remains).
        let critical = compute_critical_path(&pr.snapshot);
        if let Some(tail) = critical.last()
            && let Some(node) = pr.snapshot.node(tail)
            && pr.status != PrStatus::Merged
        {
            let total_eta: u64 = critical
                .iter()
                .filter_map(|id| pr.snapshot.node(id))
                .filter_map(|n| n.eta_secs)
                .sum();
            spans.push(Span::raw("  ·  "));
            spans.push(Span::styled(
                format!(
                    "crit: {} (~{}s)",
                    node.label.chars().take(22).collect::<String>(),
                    total_eta
                ),
                theme.bold(theme.warning),
            ));
        }
        lines.push(Line::from(spans));
    } else {
        lines.push(Line::from(Span::styled(
            " no active pull requests".to_string(),
            theme.muted(),
        )));
    }

    // Line 2: fleet rollup.
    let f = &snap.fleet_summary;
    let mut roll = vec![
        Span::raw(" "),
        Span::styled(
            format!("OPEN {} ", f.open_prs),
            theme.bold(theme.text_primary),
        ),
        Span::styled(format!("· RUN {} ", f.running), theme.bold(theme.running)),
        Span::styled(format!("· BLOCK {} ", f.blocked), theme.bold(theme.fail)),
        Span::styled(
            format!("· MERGED {} ", f.merged_today),
            theme.bold(theme.ok),
        ),
        Span::styled(
            format!("· READY {} ", f.ready_to_ship),
            theme.bold(theme.ok),
        ),
    ];
    if f.canary_in_flight {
        roll.push(Span::styled(
            "· CANARY ◉ ".to_string(),
            theme.bold(theme.vti_fire),
        ));
    }
    if f.prod_in_flight {
        roll.push(Span::styled(
            "· PROD ◉ ".to_string(),
            theme.bold(theme.running),
        ));
    }
    if let Some(url) = &f.canary_url {
        roll.push(Span::raw("· "));
        roll.push(Span::styled(format!("canary={}", url), theme.muted()));
    }
    lines.push(Line::from(roll));

    lines
}
