//! Owner: Interactive TUI subsystem — workflow DAG renderer
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::widget`
//! Invariants: Widget is pure rendering; it never mutates workflow state.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::hit_map::DeliveryHitMap;
use super::minimap::draw_minimap;
use super::mission_strip::draw_mission_strip;
use super::model::*;
use super::nav::{
    BANNER_H, EDGE_GUTTER_H, NODE_CARD_H, NODE_CARD_W, PHASE_HEADER_H, WorkflowNav, WorkflowZoom,
};
use super::phase_rail::draw_phase_rail;
use super::pr_rail::draw_pr_rail;
use super::regions::{DeliveryRegions, compute_regions};
use crate::tui::theme::Theme;

/// Height of one full phase row on the virtual canvas
/// (header + card body + edge gutter below).
/// Height of one full phase row on the virtual canvas.
const _PHASE_ROW_H: i32 = PHASE_HEADER_H as i32 + NODE_CARD_H as i32 + EDGE_GUTTER_H as i32;

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
    let regions = compute_regions(area);
    hit_map.mission = visible(regions.mission);
    hit_map.pr_rail = visible(regions.pr_rail);
    hit_map.phase_rail = visible(regions.phase_rail);
    hit_map.canvas = visible(regions.canvas);
    hit_map.minimap = visible(regions.minimap);
    hit_map.cards.clear();

    if DeliveryRegions::is_visible(regions.mission) {
        draw_mission_strip(f, regions.mission, delivery, theme);
    }
    if DeliveryRegions::is_visible(regions.pr_rail) {
        draw_pr_rail(f, regions.pr_rail, delivery, theme);
    }
    if DeliveryRegions::is_visible(regions.phase_rail) {
        draw_phase_rail(f, regions.phase_rail, delivery, theme);
    }
    if DeliveryRegions::is_visible(regions.canvas) {
        if let Some(pr) = delivery.selected() {
            if pr.snapshot.phases.is_empty() {
                draw_empty_state(f, regions.canvas, &pr.snapshot, theme);
            } else {
                draw_dag_canvas_with_hits(
                    f,
                    regions.canvas,
                    &pr.snapshot,
                    nav,
                    theme,
                    tick,
                    hit_map,
                );
            }
        } else {
            draw_no_pr_state(f, regions.canvas, theme);
        }
    }
    if DeliveryRegions::is_visible(regions.minimap) {
        draw_minimap(f, regions.minimap, delivery, nav, theme);
    }
    if DeliveryRegions::is_visible(regions.footer) {
        draw_delivery_footer(f, regions.footer, delivery, theme);
    }
}

fn visible(r: Rect) -> Option<Rect> {
    if r.width == 0 || r.height == 0 {
        None
    } else {
        Some(r)
    }
}

/// Hit-map-aware DAG canvas. Mirrors `draw_dag_canvas` but pushes each
/// rendered card's rect into `hit_map` for mouse hit-testing.
#[allow(clippy::too_many_arguments)]
pub fn draw_dag_canvas_with_hits(
    f: &mut Frame,
    dag_area: Rect,
    snapshot: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
    hit_map: &mut DeliveryHitMap,
) {
    let dag_h = dag_area.height;
    if dag_h == 0 {
        return;
    }

    for (pi, phase) in snapshot.phases.iter().enumerate() {
        let virtual_y = nav.phase_virtual_y(pi);
        let phase_h = PHASE_HEADER_H as i32 + NODE_CARD_H as i32;
        let screen_y = virtual_y - nav.viewport_y;
        if screen_y + phase_h + EDGE_GUTTER_H as i32 <= 0 || screen_y >= dag_h as i32 {
            continue;
        }
        let render_y = dag_area.y as i32 + screen_y;
        if render_y >= 0 && (render_y as u16) < dag_area.y + dag_area.height {
            let clipped_y = render_y.max(dag_area.y as i32) as u16;
            let max_bottom = dag_area.y + dag_area.height;
            let clipped_h =
                ((render_y + phase_h).min(max_bottom as i32) - clipped_y as i32).max(0) as u16;
            if clipped_h > 0 {
                let phase_rect = Rect::new(dag_area.x, clipped_y, dag_area.width, clipped_h);
                draw_phase_row_with_hits(
                    f, phase_rect, pi, phase, snapshot, nav, theme, tick, hit_map,
                );
            }
        }
        if pi + 1 < snapshot.phases.len() {
            let gutter_y = render_y + phase_h;
            if gutter_y >= dag_area.y as i32
                && (gutter_y as u16) + EDGE_GUTTER_H <= dag_area.y + dag_area.height
            {
                let gutter_rect = Rect::new(
                    dag_area.x,
                    gutter_y as u16,
                    dag_area.width,
                    EDGE_GUTTER_H.min(dag_area.y + dag_area.height - gutter_y as u16),
                );
                draw_edge_gutter(f, gutter_rect, pi, snapshot, nav, theme);
            }
        }
    }
    draw_viewport_indicator(f, dag_area, nav, theme);
}

#[allow(clippy::too_many_arguments)]
fn draw_phase_row_with_hits(
    f: &mut Frame,
    area: Rect,
    phase_idx: usize,
    phase: &WorkflowPhase,
    snap: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
    hit_map: &mut DeliveryHitMap,
) {
    let card_w = NODE_CARD_W;
    let spacing = 2u16;

    // Phase header line.
    let phase_title = format!(" {} ", phase.title);
    let header_style = Style::default().fg(theme.border_subtle);
    let dashes: String =
        "─".repeat(area.width.saturating_sub(phase_title.len() as u16 + 2) as usize);
    let header_line = Line::from(vec![
        Span::styled("─", header_style),
        Span::styled(
            phase_title.clone(),
            Style::default()
                .fg(theme.text_secondary)
                .add_modifier(Modifier::DIM),
        ),
        Span::styled(dashes, header_style),
    ]);
    if area.height > 0 {
        f.render_widget(
            Paragraph::new(header_line),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }
    let cards_y = area.y + 1;
    let cards_h = area.height.saturating_sub(1);
    if cards_h == 0 {
        return;
    }
    for (ni, node_id) in phase.node_ids.iter().enumerate() {
        if let Some(node) = snap.node(node_id) {
            let vx = (ni as i32) * (card_w as i32 + spacing as i32);
            let screen_x = vx - nav.viewport_x;
            if screen_x + card_w as i32 <= 0 || screen_x >= area.width as i32 {
                continue;
            }
            let render_x = (area.x as i32 + screen_x).max(area.x as i32) as u16;
            let available_w = (area.x + area.width).saturating_sub(render_x);
            let cw = card_w.min(available_w);
            if cw == 0 {
                continue;
            }
            let card_rect = Rect::new(render_x, cards_y, cw, cards_h.min(NODE_CARD_H - 1));
            let is_selected = phase_idx == nav.phase_idx && ni == nav.node_idx;
            draw_node_card(f, card_rect, node, is_selected, snap, theme, tick, nav.zoom);
            hit_map.cards.push((card_rect, phase_idx, ni));
        }
    }
}

/// Render the scrollable DAG canvas inside `dag_area`. Reused by both the
/// legacy workflow tab and the new Delivery view.
pub fn draw_dag_canvas(
    f: &mut Frame,
    dag_area: Rect,
    snapshot: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
) {
    let dag_h = dag_area.height;
    if dag_h == 0 {
        return;
    }

    // Render phases with viewport clipping.
    for (pi, phase) in snapshot.phases.iter().enumerate() {
        let virtual_y = nav.phase_virtual_y(pi); // DAG-relative
        let phase_h = PHASE_HEADER_H as i32 + NODE_CARD_H as i32;

        // Check if this phase is visible in the viewport.
        let screen_y = virtual_y - nav.viewport_y;
        if screen_y + phase_h + EDGE_GUTTER_H as i32 <= 0 || screen_y >= dag_h as i32 {
            continue; // Entirely off-screen.
        }

        // Compute the visible portion of this phase.
        let render_y = dag_area.y as i32 + screen_y;
        if render_y >= 0 && (render_y as u16) < dag_area.y + dag_area.height {
            let clipped_y = render_y.max(dag_area.y as i32) as u16;
            let max_bottom = dag_area.y + dag_area.height;
            let clipped_h =
                ((render_y + phase_h).min(max_bottom as i32) - clipped_y as i32).max(0) as u16;
            if clipped_h > 0 {
                let phase_rect = Rect::new(dag_area.x, clipped_y, dag_area.width, clipped_h);
                draw_phase_row(f, phase_rect, pi, phase, snapshot, nav, theme, tick);
            }
        }

        // Draw edge gutter below this phase (if there's a next phase).
        if pi + 1 < snapshot.phases.len() {
            let gutter_y = render_y + phase_h;
            if gutter_y >= dag_area.y as i32
                && (gutter_y as u16) + EDGE_GUTTER_H <= dag_area.y + dag_area.height
            {
                let gutter_rect = Rect::new(
                    dag_area.x,
                    gutter_y as u16,
                    dag_area.width,
                    EDGE_GUTTER_H.min(dag_area.y + dag_area.height - gutter_y as u16),
                );
                draw_edge_gutter(f, gutter_rect, pi, snapshot, nav, theme);
            }
        }
    }

    // --- Viewport position indicator ---
    draw_viewport_indicator(f, dag_area, nav, theme);
}

fn draw_no_pr_state(f: &mut Frame, area: Rect, theme: &Theme) {
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

fn draw_delivery_footer(f: &mut Frame, area: Rect, _delivery: &DeliverySnapshot, theme: &Theme) {
    let hint = " ↑↓←→ move · </> PR · []/PgUp/PgDn pan · f follow · b blocker · c crit · z zoom · Enter inspect · r rollback · ? help";
    let line = Line::from(Span::styled(hint, theme.muted()));
    f.render_widget(Paragraph::new(line), area);
}

fn draw_empty_state(f: &mut Frame, area: Rect, _snapshot: &WorkflowSnapshot, theme: &Theme) {
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

#[allow(clippy::too_many_arguments)]
fn draw_phase_row(
    f: &mut Frame,
    area: Rect,
    phase_idx: usize,
    phase: &WorkflowPhase,
    snap: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
) {
    let _node_count = phase.node_ids.len().max(1);

    // Compute card width: fixed NODE_CARD_W, laid out with spacing.
    let card_w = NODE_CARD_W;
    let spacing = 2u16;

    // Phase header line.
    let phase_title = format!(" {} ", phase.title);
    let header_style = Style::default().fg(theme.border_subtle);
    // Build a dashed line header.
    let dashes: String =
        "─".repeat(area.width.saturating_sub(phase_title.len() as u16 + 2) as usize);
    let header_line = Line::from(vec![
        Span::styled("─", header_style),
        Span::styled(
            phase_title.clone(),
            Style::default()
                .fg(theme.text_secondary)
                .add_modifier(Modifier::DIM),
        ),
        Span::styled(dashes, header_style),
    ]);
    if area.height > 0 {
        f.render_widget(
            Paragraph::new(header_line),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }

    // Render node cards below the header.
    let cards_y = area.y + 1;
    let cards_h = area.height.saturating_sub(1);
    if cards_h == 0 {
        return;
    }

    for (ni, node_id) in phase.node_ids.iter().enumerate() {
        if let Some(node) = snap.node(node_id) {
            // Virtual X position for this card.
            let vx = (ni as i32) * (card_w as i32 + spacing as i32);
            let screen_x = vx - nav.viewport_x;

            // Skip if entirely off-screen horizontally.
            if screen_x + card_w as i32 <= 0 || screen_x >= area.width as i32 {
                continue;
            }

            let render_x = (area.x as i32 + screen_x).max(area.x as i32) as u16;
            let available_w = (area.x + area.width).saturating_sub(render_x);
            let cw = card_w.min(available_w);
            if cw == 0 {
                continue;
            }

            let card_rect = Rect::new(render_x, cards_y, cw, cards_h.min(NODE_CARD_H - 1));
            let is_selected = phase_idx == nav.phase_idx && ni == nav.node_idx;
            draw_node_card(f, card_rect, node, is_selected, snap, theme, tick, nav.zoom);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_node_card(
    f: &mut Frame,
    area: Rect,
    node: &WorkflowNode,
    is_selected: bool,
    snap: &WorkflowSnapshot,
    theme: &Theme,
    tick: u64,
    zoom: WorkflowZoom,
) {
    let status_color = node_color(node.status, theme);
    let stalled = is_stalled(node, chrono::Utc::now());

    // Border style: selected → bright accent; stalled running → amber pulse;
    // running → green pulse; otherwise → status color.
    let border_style = if is_selected {
        Style::default()
            .fg(theme.border_accent)
            .add_modifier(Modifier::BOLD)
    } else if stalled {
        let pulse = if (tick / 2).is_multiple_of(2) {
            Modifier::BOLD
        } else {
            Modifier::empty()
        };
        Style::default().fg(theme.warning).add_modifier(pulse)
    } else if node.status == WorkflowStatus::Running {
        let pulse = if (tick / 4).is_multiple_of(2) {
            Modifier::BOLD
        } else {
            Modifier::empty()
        };
        Style::default().fg(status_color).add_modifier(pulse)
    } else {
        Style::default().fg(status_color)
    };

    let vti_badge = match node.vti_status.as_ref() {
        Some(v) => v.badge(),
        None => "",
    };
    let cache_badge = match node.cache_verdict.as_ref() {
        Some(c) => c.badge(),
        None => "",
    };

    // Accent prefix from the node kind (agent, auto-merge, promote, etc.).
    let accent = node.kind.glyph();
    let title_max = area.width.saturating_sub(22) as usize;
    let title = format!(
        " {} {} {} {}{}{}{}",
        node.status.glyph(),
        if accent.is_empty() { " " } else { accent },
        node.status.label(),
        crate::tui::widgets::truncate_label(&node.label, title_max),
        if node.critical_path { " [CRIT]" } else { "" },
        if is_selected { " [SEL]" } else { "" },
        if stalled { " [STALL]" } else { "" },
    );

    let mut lines = Vec::new();

    // Command line — hidden in Overview/Dense zoom levels.
    if zoom == WorkflowZoom::Cards
        && let Some(cmd) = &node.command
    {
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                crate::tui::widgets::truncate_label(cmd, area.width.saturating_sub(4) as usize)
            ),
            theme.muted(),
        )));
    }

    // Badges + progress.
    let mut badge_spans = vec![Span::styled("  ", Style::default())];
    if !vti_badge.is_empty() {
        badge_spans.push(Span::styled(
            format!("{} ", vti_badge),
            theme.bold(theme.vti_fire),
        ));
    }
    if !cache_badge.is_empty() {
        badge_spans.push(Span::styled(
            format!("{} ", cache_badge),
            theme.bold(theme.ok),
        ));
    }
    if let Some(pct) = node.progress_pct {
        badge_spans.push(Span::styled(
            format!("{} {}%", progress_bar(pct, 10), pct),
            theme.bold(status_color),
        ));
    }
    if let Some(eta) = node.eta_secs {
        badge_spans.push(Span::styled(format!(" eta:{}s", eta), theme.muted()));
    }
    if node.kind.is_rollback_eligible()
        && matches!(node.status, WorkflowStatus::Ran | WorkflowStatus::Running)
    {
        badge_spans.push(Span::styled(" [ROLLBACK]", theme.bold(theme.warning)));
    }
    if matches!(node.kind, WorkflowNodeKind::AgentReview { .. }) {
        badge_spans.push(Span::styled(" agent", theme.bold(theme.agent)));
    }
    // Intelligence chip: failed/blocked nodes show "blocks N · reason".
    if matches!(node.status, WorkflowStatus::Error | WorkflowStatus::Blocked) {
        let downstream =
            crate::tui::workflow::intelligence::compute_downstream_impact(snap, &node.id);
        let reason_excerpt = node.reason.as_deref().map_or_else(String::new, |r| {
            let max = area.width.saturating_sub(20) as usize;
            r.chars().take(max.max(6)).collect::<String>()
        });
        let chip = if downstream > 0 {
            format!(" ⚠ blocks {}", downstream)
        } else {
            " ⚠".into()
        };
        badge_spans.push(Span::styled(chip, theme.bold(theme.fail)));
        if !reason_excerpt.is_empty() {
            badge_spans.push(Span::raw(" "));
            badge_spans.push(Span::styled(reason_excerpt, theme.muted()));
        }
    }
    // Overview zoom hides badges entirely; Cards/Dense always show them.
    if zoom != WorkflowZoom::Overview && badge_spans.len() > 1 {
        lines.push(Line::from(badge_spans));
    }

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        ),
        area,
    );
}

/// Draw ASCII dependency edges in the gutter between two adjacent phases.
fn draw_edge_gutter(
    f: &mut Frame,
    area: Rect,
    phase_idx: usize,
    snap: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let card_w = NODE_CARD_W;
    let spacing = 2u16;

    let current_phase = match snap.phases.get(phase_idx) {
        Some(p) => p,
        None => return,
    };
    let next_phase = match snap.phases.get(phase_idx + 1) {
        Some(p) => p,
        None => return,
    };

    // For each node in the next phase, find its dependencies in the current phase
    // and draw connectors.
    let buf = f.buffer_mut();

    for (ni, next_nid) in next_phase.node_ids.iter().enumerate() {
        let next_node = match snap.node(next_nid) {
            Some(n) => n,
            None => continue,
        };

        // Target X center for this next-phase node.
        let next_vx = (ni as i32) * (card_w as i32 + spacing as i32) + card_w as i32 / 2;
        let next_sx = next_vx - nav.viewport_x;
        if next_sx < 0 || next_sx >= area.width as i32 {
            continue;
        }
        let target_x = area.x + next_sx as u16;

        // Find parent nodes in the current phase.
        for (pi, parent_nid) in current_phase.node_ids.iter().enumerate() {
            if !next_node.deps.contains(parent_nid) {
                // Also check edges for stage-order dependencies.
                let has_edge = snap
                    .edges
                    .iter()
                    .any(|e| e.from == *parent_nid && e.to == *next_nid);
                if !has_edge {
                    continue;
                }
            }

            let parent_vx = (pi as i32) * (card_w as i32 + spacing as i32) + card_w as i32 / 2;
            let parent_sx = parent_vx - nav.viewport_x;
            if parent_sx < 0 || parent_sx >= area.width as i32 {
                continue;
            }
            let source_x = area.x + parent_sx as u16;

            let edge_color = edge_color_for(next_node, theme);
            let style = Style::default().fg(edge_color);

            // Draw vertical line from source down.
            let y0 = area.y;
            let y_mid = area.y + area.height / 2;
            let y_end = area.y + area.height.saturating_sub(1);

            // Vertical drop from parent.
            if source_x < area.x + area.width && y0 < area.y + area.height {
                set_cell(buf, source_x, y0, "│", style, area);
            }

            // Horizontal connector.
            if source_x != target_x && y_mid < area.y + area.height {
                let (left, right) = if source_x < target_x {
                    (source_x, target_x)
                } else {
                    (target_x, source_x)
                };

                // Corner at source.
                if source_x < target_x {
                    set_cell(buf, source_x, y_mid, "└", style, area);
                } else {
                    set_cell(buf, source_x, y_mid, "┘", style, area);
                }

                // Horizontal line.
                for x in (left + 1)..right {
                    set_cell(buf, x, y_mid, "─", style, area);
                }

                // Corner at target.
                if source_x < target_x {
                    set_cell(buf, target_x, y_mid, "┐", style, area);
                } else {
                    set_cell(buf, target_x, y_mid, "┌", style, area);
                }

                // Vertical drop to target.
                for y in (y_mid + 1)..=y_end {
                    set_cell(buf, target_x, y, "│", style, area);
                }
            } else {
                // Straight vertical.
                for y in (y0 + 1)..=y_end {
                    set_cell(buf, source_x, y, "│", style, area);
                }
            }

            // Arrow head at the bottom.
            set_cell(buf, target_x, y_end, "▼", style, area);
        }
    }
}

/// Safely set a cell in the buffer, clipped to the given area.
fn set_cell(
    buf: &mut ratatui::buffer::Buffer,
    x: u16,
    y: u16,
    symbol: &str,
    style: Style,
    clip: Rect,
) {
    if x >= clip.x && x < clip.x + clip.width && y >= clip.y && y < clip.y + clip.height {
        let cell = &mut buf[(x, y)];
        cell.set_symbol(symbol);
        cell.set_style(style);
    }
}

/// Render a block-character progress bar (e.g. `███░░░░ 41%` style).
fn progress_bar(pct: u16, width: usize) -> String {
    let pct = pct.min(100) as usize;
    let filled = pct * width / 100;
    let empty = width - filled;
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

/// Determine whether a running node has overrun its ETA budget (eta*1.5,
/// 90s floor) using the same logic as `intelligence::detect_stalls`.
fn is_stalled(node: &WorkflowNode, now: chrono::DateTime<chrono::Utc>) -> bool {
    if !matches!(node.status, WorkflowStatus::Running) {
        return false;
    }
    let Some(started) = node.started_at else {
        return false;
    };
    let elapsed = (now - started).num_seconds().max(0) as u64;
    let budget = node
        .eta_secs
        .map(|e| ((e as f64) * 1.5) as u64)
        .unwrap_or(90)
        .max(90);
    elapsed > budget
}

/// Choose edge color based on the target node's status.
fn edge_color_for(node: &WorkflowNode, theme: &Theme) -> Color {
    match node.status {
        WorkflowStatus::Running => theme.running,
        WorkflowStatus::Error => theme.fail,
        WorkflowStatus::Blocked => theme.blocked,
        WorkflowStatus::Ran => theme.ok,
        WorkflowStatus::Waiting => theme.border_subtle,
        WorkflowStatus::Skipped => theme.skipped,
        WorkflowStatus::Cached => theme.vti_fire,
        WorkflowStatus::Unknown => theme.text_muted,
    }
}

/// Draw a small viewport position indicator in the bottom-right corner.
fn draw_viewport_indicator(f: &mut Frame, area: Rect, nav: &WorkflowNav, theme: &Theme) {
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

fn node_color(status: WorkflowStatus, theme: &Theme) -> Color {
    match status {
        WorkflowStatus::Ran => theme.ok,
        WorkflowStatus::Running => theme.running,
        WorkflowStatus::Error => theme.fail,
        WorkflowStatus::Waiting => theme.waiting,
        WorkflowStatus::Skipped => theme.skipped,
        WorkflowStatus::Cached => theme.vti_fire,
        WorkflowStatus::Blocked => theme.blocked,
        WorkflowStatus::Unknown => theme.text_muted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::workflow::builder::build_demo_snapshot;

    #[test]
    fn node_color_maps_all_statuses() {
        let theme = Theme::dark();
        // Ensure every status maps without panic.
        for s in &[
            WorkflowStatus::Waiting,
            WorkflowStatus::Running,
            WorkflowStatus::Ran,
            WorkflowStatus::Error,
            WorkflowStatus::Skipped,
            WorkflowStatus::Cached,
            WorkflowStatus::Blocked,
            WorkflowStatus::Unknown,
        ] {
            let _ = node_color(*s, &theme);
        }
    }

    #[test]
    fn demo_snapshot_has_phases() {
        let snap = build_demo_snapshot();
        assert!(!snap.phases.is_empty());
        assert!(!snap.nodes.is_empty());
    }
}
