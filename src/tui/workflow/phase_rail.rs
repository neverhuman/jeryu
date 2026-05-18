//! Owner: Interactive TUI subsystem — Delivery phase rail (vertical index)
//! Proof: rendered indirectly via integration smoke
//! Invariants: Render-only; reads CanonicalPhase order, rolls up node statuses.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::model::*;
use crate::tui::theme::Theme;

pub fn draw_phase_rail(f: &mut Frame, area: Rect, delivery: &DeliverySnapshot, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let block = Block::default()
        .title(" Phase ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_subtle));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(pr) = delivery.selected() else {
        return;
    };

    let lines: Vec<Line> = CanonicalPhase::ALL
        .iter()
        .map(|phase| rail_line(*phase, &pr.snapshot, theme, pr.phase == *phase))
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

fn rail_line<'a>(
    phase: CanonicalPhase,
    snap: &WorkflowSnapshot,
    theme: &Theme,
    is_current: bool,
) -> Line<'a> {
    let status = rollup(phase, snap);
    let color = status_color(status, theme);
    let short = phase.short();
    let glyph = status.glyph();

    let prefix = if is_current { "▸ " } else { "  " };
    let mut style = Style::default().fg(color);
    if is_current {
        style = style.add_modifier(Modifier::BOLD);
    }
    if status == WorkflowStatus::Waiting {
        style = style.add_modifier(Modifier::DIM);
    }

    Line::from(vec![
        Span::styled(prefix, theme.muted()),
        Span::styled(format!("{:<7} {}", short, glyph), style),
    ])
}

/// Roll up the status of all nodes tagged with this canonical phase.
fn rollup(phase: CanonicalPhase, snap: &WorkflowSnapshot) -> WorkflowStatus {
    let mut found_any = false;
    let mut has_error = false;
    let mut has_blocked = false;
    let mut has_running = false;
    let mut all_terminal = true;
    let mut all_skipped = true;
    let mut all_cached = true;
    for n in &snap.nodes {
        if n.tags.iter().any(|t| t == phase.slug()) {
            found_any = true;
            match n.status {
                WorkflowStatus::Error => has_error = true,
                WorkflowStatus::Blocked => has_blocked = true,
                WorkflowStatus::Running => has_running = true,
                _ => {}
            }
            if !n.status.is_terminal() {
                all_terminal = false;
            }
            if !matches!(n.status, WorkflowStatus::Skipped) {
                all_skipped = false;
            }
            if !matches!(n.status, WorkflowStatus::Cached) {
                all_cached = false;
            }
        }
    }
    if !found_any {
        return WorkflowStatus::Unknown;
    }
    if has_error {
        WorkflowStatus::Error
    } else if has_blocked {
        WorkflowStatus::Blocked
    } else if has_running {
        WorkflowStatus::Running
    } else if all_terminal && all_cached {
        WorkflowStatus::Cached
    } else if all_terminal && all_skipped {
        WorkflowStatus::Skipped
    } else if all_terminal {
        WorkflowStatus::Ran
    } else {
        WorkflowStatus::Waiting
    }
}

fn status_color(status: WorkflowStatus, theme: &Theme) -> Color {
    match status {
        WorkflowStatus::Ran => theme.ok,
        WorkflowStatus::Running => theme.running,
        WorkflowStatus::Error => theme.fail,
        WorkflowStatus::Blocked => theme.blocked,
        WorkflowStatus::Cached => theme.vti_fire,
        WorkflowStatus::Skipped => theme.skipped,
        WorkflowStatus::Waiting => theme.waiting,
        WorkflowStatus::Unknown => theme.text_muted,
    }
}
