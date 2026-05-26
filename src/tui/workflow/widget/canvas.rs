//! Owner: Interactive TUI subsystem — workflow widget canvas (public entry points).
//! Proof: `cargo nextest run -p jeryu --lib tui::workflow::`
//! Invariants: Widget is pure rendering; it never mutates workflow state.
//! Layout: split from monolithic `widget.rs` (1063 LOC) per TUI_RESET_PLAN_FINAL.md §3.2.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::super::hit_map::DeliveryHitMap;
use super::super::minimap::draw_minimap_with_chrome;
use super::super::mission_strip::draw_mission_strip_with_chrome;
use super::super::model::*;
use super::super::nav::{BANNER_H, WorkflowNav};
use super::super::phase_rail::draw_phase_rail_with_chrome;
use super::super::pr_rail::draw_pr_rail_with_chrome;
use super::super::regions::{DeliveryRegions, compute_regions};
use super::hit_map::draw_dag_canvas_with_hits;
use super::layout::{draw_canvas_frame, visible};
use super::render::draw_dag_canvas;
use crate::tui::{focus::PaneChrome, theme::Theme};

#[derive(Debug, Clone, Copy, Default)]
pub struct DeliveryChrome {
    pub mission: Option<PaneChrome>,
    pub pr_rail: Option<PaneChrome>,
    pub phase_rail: Option<PaneChrome>,
    pub canvas: Option<PaneChrome>,
    pub minimap: Option<PaneChrome>,
}

/// Draw the full workflow tab: summary banner + scrollable DAG with edges.
/// Legacy entry point retained for the single-workflow code path.
pub fn draw_workflow_tab(
    f: &mut Frame,
    area: Rect,
    snapshot: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
) {
    if snapshot.phases.is_empty() {
        draw_empty_state(f, area, snapshot, theme);
        return;
    }

    // --- Summary banner (always visible at top) ---
    let banner_h = BANNER_H.min(area.height);
    let banner_area = Rect::new(area.x, area.y, area.width, banner_h);
    draw_summary_banner(f, banner_area, snapshot, nav, theme);

    // --- Scrollable DAG area below banner ---
    let dag_y = area.y + banner_h;
    let dag_h = area.height.saturating_sub(banner_h);
    if dag_h == 0 {
        return;
    }
    let dag_area = Rect::new(area.x, dag_y, area.width, dag_h);
    draw_dag_canvas(f, dag_area, snapshot, nav, theme, tick);
}

/// Render the Delivery view — mission strip, PR rail, phase rail, DAG canvas,
/// minimap, and footer for the currently selected PR. Populates `hit_map`
/// with the region rects so the mouse handler can dispatch clicks.
#[allow(clippy::too_many_arguments)]
pub fn draw_delivery_tab(
    f: &mut Frame,
    area: Rect,
    delivery: &DeliverySnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
    hit_map: &mut DeliveryHitMap,
) {
    draw_delivery_tab_with_chrome(
        f,
        area,
        delivery,
        nav,
        theme,
        tick,
        hit_map,
        DeliveryChrome::default(),
        crate::repo_fleet::RepoFilter::All,
    );
}

/// Focus-aware Delivery view renderer. The optional chrome is supplied by
/// `ui.rs`; pure widget tests can use the default unfocused chrome.
/// `repo_filter` restricts which PRs are visible in the rail and clickable.
#[allow(clippy::too_many_arguments)]
pub fn draw_delivery_tab_with_chrome(
    f: &mut Frame,
    area: Rect,
    delivery: &DeliverySnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
    hit_map: &mut DeliveryHitMap,
    chrome: DeliveryChrome,
    repo_filter: crate::repo_fleet::RepoFilter<'_>,
) {
    let regions = compute_regions(area);
    hit_map.mission = visible(regions.mission);
    hit_map.pr_rail = visible(regions.pr_rail);
    hit_map.phase_rail = visible(regions.phase_rail);
    hit_map.canvas = visible(regions.canvas);
    hit_map.minimap = visible(regions.minimap);
    hit_map.cards.clear();

    if DeliveryRegions::is_visible(regions.mission) {
        draw_mission_strip_with_chrome(f, regions.mission, delivery, theme, chrome.mission);
    }
    if DeliveryRegions::is_visible(regions.pr_rail) {
        draw_pr_rail_with_chrome(
            f,
            regions.pr_rail,
            delivery,
            theme,
            chrome.pr_rail,
            repo_filter,
        );
    }
    if DeliveryRegions::is_visible(regions.phase_rail) {
        draw_phase_rail_with_chrome(f, regions.phase_rail, delivery, theme, chrome.phase_rail);
    }
    if DeliveryRegions::is_visible(regions.canvas) {
        let canvas_inner = draw_canvas_frame(f, regions.canvas, theme, chrome.canvas);
        if let Some(pr) = delivery.selected() {
            if pr.snapshot.phases.is_empty() {
                draw_empty_state(f, canvas_inner, &pr.snapshot, theme);
            } else {
                draw_dag_canvas_with_hits(f, canvas_inner, &pr.snapshot, nav, theme, tick, hit_map);
            }
        } else {
            draw_no_pr_state(f, canvas_inner, theme);
        }
    }
    if DeliveryRegions::is_visible(regions.minimap) {
        draw_minimap_with_chrome(f, regions.minimap, delivery, nav, theme, chrome.minimap);
    }
    if DeliveryRegions::is_visible(regions.footer) {
        draw_delivery_footer(f, regions.footer, delivery, theme);
    }
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

/// Render an explicit empty-state card. Shown by `ui.rs` when the delivery
/// snapshot is empty. It still reports backend/source health so operators can
/// tell whether live sync is configured or just temporarily empty.
pub fn draw_workflow_empty_state(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    status: &crate::tui::app::DeliverySourceStatus,
) {
    let backend = status
        .backend_label
        .as_deref()
        .unwrap_or("(backend unavailable)");
    let source = status.source_label.as_deref().unwrap_or("(not configured)");
    let last_sync = status
        .last_sync_at
        .map(|t| t.format("%Y-%m-%d %H:%M:%SZ").to_string())
        .unwrap_or_else(|| "(never)".into());
    let status_line = match (&status.last_sync_error, status.configured) {
        (Some(err), _) => format!("error: {err}"),
        (None, true) => "ok".into(),
        (None, false) => "(no fleet registry configured)".into(),
    };
    let headline = if status.configured {
        "  No active pull requests"
    } else {
        "  No pull requests configured"
    };

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(headline, theme.bold(theme.text_primary))),
        Line::from(""),
        Line::from(Span::styled(
            if status.configured {
                "  Open a merge request in one of the tracked repos to populate this rail."
            } else {
                "  Waiting for fleet registry and host backends to report live work."
            },
            theme.muted(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Backend:  ", theme.muted()),
            Span::styled(backend.to_string(), theme.secondary()),
        ]),
        Line::from(vec![
            Span::styled("  Source:    ", theme.muted()),
            Span::styled(source.to_string(), theme.secondary()),
        ]),
        Line::from(vec![
            Span::styled("  Last sync: ", theme.muted()),
            Span::styled(last_sync, theme.secondary()),
        ]),
        Line::from(vec![
            Span::styled("  Status:    ", theme.muted()),
            Span::styled(
                status_line,
                if status.last_sync_error.is_some() {
                    theme.bold(theme.fail)
                } else if status.configured {
                    theme.bold(theme.ok)
                } else {
                    theme.muted()
                },
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            if status.configured {
                "  Live PR/MR sync is active for the tracked fleet."
            } else {
                "  Live PR/MR sync is not configured yet."
            },
            if status.configured {
                theme.bold(theme.ok)
            } else {
                theme.muted()
            },
        )),
    ];
    while lines.len() < (area.height as usize).saturating_sub(2) {
        lines.push(Line::from(""));
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" [ Workflow ] ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border_subtle)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_delivery_footer(f: &mut Frame, area: Rect, _delivery: &DeliverySnapshot, theme: &Theme) {
    let hint = " ↑↓←→ move · </> PR · []/PgUp/PgDn pan · f follow · b blocker · c crit · z zoom · Enter inspect · r rollback · ? help";
    let line = Line::from(Span::styled(hint, theme.muted()));
    f.render_widget(Paragraph::new(line), area);
}

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

fn draw_summary_banner(
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
