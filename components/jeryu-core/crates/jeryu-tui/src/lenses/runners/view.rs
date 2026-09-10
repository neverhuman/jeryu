//! Runners lens view — operator Pools/Health.
//!
//! Invariants: pure draw. Reads the [`PoolHealthInput`] / [`RunnersLensInput`]
//! projections; no backend I/O. Renders a fleet-totals header, the ranked
//! bottleneck/health banner (from `PoolActivity::bottlenecks()` + `health()`), a
//! per-pool grid (POOL/UTIL/SLOTS/JOBS/RUNNERS/PAUSED), and the per-node grid.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use jeryu_readmodel::{HealthLevel, TuiReadModel};

use super::data::{PoolHealthInput, PoolHealthRow, RunnerNodeRow, RunnersLensInput};

/// Draw the Pools/Health lens directly from the read model. Projects both the
/// per-pool [`PoolHealthInput`] and the per-node [`RunnersLensInput`].
pub fn draw_from_model(f: &mut Frame, model: &TuiReadModel, area: Rect) {
    let pools = PoolHealthInput::from_read_model(model);
    let nodes = RunnersLensInput::from_read_model(model);
    draw(f, &pools, &nodes, area);
}

pub fn draw(f: &mut Frame, pools: &PoolHealthInput, nodes: &RunnersLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // fleet-totals header
            Constraint::Length(3), // bottleneck / health banner
            Constraint::Min(0),    // per-pool grid
            Constraint::Length(7), // per-node grid
            Constraint::Length(3), // footer / keys
        ])
        .split(area);

    draw_header(f, pools, chunks[0]);
    draw_banner(f, pools, chunks[1]);
    draw_pool_grid(f, pools, chunks[2]);
    draw_node_grid(f, nodes, chunks[3]);
    draw_footer(f, pools, chunks[4]);
}

fn health_style(health: HealthLevel) -> Style {
    match health {
        HealthLevel::Critical => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        HealthLevel::Degraded => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        HealthLevel::Warning => Style::default().fg(Color::Yellow),
        HealthLevel::Healthy => Style::default().fg(Color::Green),
        HealthLevel::Unknown => Style::default().fg(Color::DarkGray),
    }
}

fn draw_header(f: &mut Frame, input: &PoolHealthInput, area: Rect) {
    let t = &input.totals;
    let text = if input.health == HealthLevel::Unknown {
        format!(
            "Pools/Health — capacity unknown · {} pools · {} repos · {} queued · {} running · {} failed",
            t.pools, t.repos, t.queued_jobs, t.running_jobs, t.failed_jobs,
        )
    } else {
        format!(
            "Pools/Health — {} pools · {} repos · {}% util · {} queued · {} running · {} failed · {} online · {} stuck",
            t.pools,
            t.repos,
            input.fleet_utilization_pct(),
            t.queued_jobs,
            t.running_jobs,
            t.failed_jobs,
            t.online_runners,
            t.stuck_runners,
        )
    };
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Pools/Health — Runner Fleet "),
        ),
        area,
    );
}

fn draw_banner(f: &mut Frame, input: &PoolHealthInput, area: Rect) {
    let style = health_style(input.health);
    let mut spans = vec![Span::styled(
        format!("[{}] ", input.health.label()),
        style.add_modifier(Modifier::BOLD),
    )];
    spans.push(Span::styled(input.banner_line(), style));
    if input.bottlenecks.len() > 1 {
        spans.push(Span::styled(
            format!("  (+{} more)", input.bottlenecks.len() - 1),
            Style::default().fg(Color::DarkGray),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans))
            .block(Block::default().borders(Borders::ALL).title(" Health ")),
        area,
    );
}

fn pool_util_style(row: &PoolHealthRow) -> Style {
    if row.paused {
        Style::default().fg(Color::DarkGray)
    } else if row.saturated {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if row.utilization_pct >= 80 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    }
}

fn draw_pool_grid(f: &mut Frame, input: &PoolHealthInput, area: Rect) {
    if input.pools.is_empty() {
        f.render_widget(
            Paragraph::new(
                "No runner pools in the rollup yet. Per-pool fleet health appears here once \
                 the scheduler/registry read populates pool_activity.",
            )
            .block(Block::default().borders(Borders::ALL).title(" Pools ")),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("POOL"),
        Cell::from("UTIL"),
        Cell::from("SLOTS"),
        Cell::from("JOBS (q/r/f)"),
        Cell::from("RUNNERS"),
        Cell::from("TRUST"),
        Cell::from("STATE"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input
        .pools
        .iter()
        .map(|pool| pool_row(pool, input.health == HealthLevel::Unknown))
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Min(12),
            Constraint::Length(6),
            Constraint::Length(13),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(9),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Pools "));

    f.render_widget(table, area);
}

fn pool_row(p: &PoolHealthRow, capacity_unknown: bool) -> Row<'_> {
    let state = if capacity_unknown {
        "UNKNOWN"
    } else if p.paused {
        "PAUSED"
    } else if p.saturated {
        "SATURATED"
    } else {
        "active"
    };
    let runners = if capacity_unknown {
        "unknown".to_string()
    } else if p.stuck_runners > 0 {
        format!("{} ({}stuck)", p.online_runners, p.stuck_runners)
    } else {
        format!("{} online", p.online_runners)
    };
    let (utilization, slots, trust, style) = if capacity_unknown {
        (
            "—".to_string(),
            "unknown".to_string(),
            "unknown".to_string(),
            health_style(HealthLevel::Unknown),
        )
    } else {
        (
            format!("{}%", p.utilization_pct),
            format!("{}/{}idle", p.active_slots, p.idle_slots),
            p.trust_tier.clone(),
            pool_util_style(p),
        )
    };
    Row::new(vec![
        Cell::from(p.pool.clone()),
        Cell::from(Span::styled(utilization, style)),
        Cell::from(slots),
        Cell::from(format!(
            "{}/{}/{}",
            p.queued_jobs, p.running_jobs, p.failed_jobs
        )),
        Cell::from(runners),
        Cell::from(trust),
        Cell::from(state),
    ])
}

fn status_style(row: &RunnerNodeRow) -> Style {
    match row.status_word() {
        "STUCK" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        "busy" => Style::default().fg(Color::Yellow),
        "idle" => Style::default().fg(Color::Gray),
        "probing" => Style::default().fg(Color::DarkGray),
        _ => Style::default().fg(Color::Green),
    }
}

fn draw_node_grid(f: &mut Frame, input: &RunnersLensInput, area: Rect) {
    if input.nodes.is_empty() {
        f.render_widget(
            Paragraph::new("No per-node runner telemetry yet.")
                .block(Block::default().borders(Borders::ALL).title(" Nodes ")),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("NODE"),
        Cell::from("STATUS"),
        Cell::from("POOL"),
        Cell::from("TAGS"),
        Cell::from("CONTACT"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input
        .nodes
        .iter()
        .map(|n| {
            Row::new(vec![
                Cell::from(n.label.clone()),
                Cell::from(Span::styled(n.status_word().to_string(), status_style(n))),
                Cell::from(n.pool.clone()),
                Cell::from(n.tags.join(",")),
                Cell::from(n.last_contact.clone()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Min(12),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Nodes "));

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, input: &PoolHealthInput, area: Rect) {
    let stuck = input.totals.stuck_runners;
    let line = if input.health == HealthLevel::Unknown {
        Line::from(format!(
            "capacity unknown · cursor={} · Keys: d drain · p pause · s scale · e evidence",
            input.event_cursor
        ))
    } else if stuck > 0 {
        Line::from(Span::styled(
            format!(
                "⚠ {stuck} runner(s) STUCK — capacity at risk · cursor={} · Keys: d drain · p pause · s scale",
                input.event_cursor
            ),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(format!(
            "{} · cursor={} · Keys: d drain · p pause · s scale · e evidence",
            if input.health == HealthLevel::Healthy {
                "fleet healthy"
            } else {
                "fleet needs attention"
            },
            input.event_cursor
        ))
    };
    f.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::{
        PoolActivity, PoolRollup, RepoActivity, TagDemand, TuiReadModel, sample_read_model,
    };
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(w: u16, h: u16, model: &TuiReadModel) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_from_model(f, model, f.area()))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    fn degraded_model() -> TuiReadModel {
        let mut saturated = PoolRollup::new("trusted");
        saturated.tags = vec!["oci".into()];
        saturated.active_slots = 2;
        saturated.running_jobs = 2;
        saturated.queued_jobs = 5;
        saturated.online_runners = 2;
        saturated.stuck_runners = 1;
        TuiReadModel {
            pool_activity: PoolActivity {
                repos: vec![RepoActivity {
                    repo: "neverhuman/jeryu".into(),
                    queued_jobs: 5,
                    running_jobs: 2,
                    ..RepoActivity::default()
                }],
                pools: vec![saturated],
                unplaceable: vec![TagDemand {
                    tags: vec!["gpu".into()],
                    count: 3,
                }],
                freshness: None,
            },
            ..TuiReadModel::default()
        }
    }

    #[test]
    fn renders_empty_fleet_at_80x24() {
        let out = ink(80, 24, &TuiReadModel::default());
        assert!(out.contains("Pools/Health"));
        assert!(out.contains("UNKNOWN"));
        assert!(out.contains("No runner pools"));
        assert!(out.contains("capacity unknown"));
        assert!(!out.to_lowercase().contains("fleet healthy"));
        assert!(!out.contains("% util"));
        assert!(!out.contains("0 online"));
    }

    #[test]
    fn renders_degraded_pool_grid_and_banner() {
        let out = ink(120, 40, &degraded_model());
        assert!(out.contains("trusted"));
        assert!(out.contains("SATURATED"));
        assert!(out.contains("CRITICAL"));
        // Tag-starvation ranks first (Critical).
        assert!(out.contains("no pool serves it"));
        assert!(out.contains("STUCK"));
    }

    #[test]
    fn renders_node_grid_when_present() {
        // The sample read model carries a healthy `oci-runner-1` node.
        let out = ink(120, 40, &sample_read_model());
        assert!(out.contains("oci-runner-1"));
        assert!(out.contains("online"));
    }

    #[test]
    fn renders_at_220x60_without_panic() {
        let _ = ink(220, 60, &degraded_model());
    }

    fn unknown_capacity_model() -> TuiReadModel {
        TuiReadModel {
            pool_activity: PoolActivity {
                freshness: Some(jeryu_readmodel::SourceFreshness {
                    source: jeryu_readmodel::SourceKind::Broker,
                    state: jeryu_readmodel::FreshnessState::Unknown,
                    observed_at: None,
                    age_ms: None,
                    cursor: None,
                    ttl_ms: None,
                    confidence: 0.0,
                    last_error: None,
                    degraded_reason: Some("runner capacity registry is not connected".into()),
                }),
                ..PoolActivity::default()
            },
            ..TuiReadModel::default()
        }
    }

    #[test]
    fn renders_unknown_capacity_with_observed_jobs() {
        let mut model = unknown_capacity_model();
        let mut observed = PoolRollup::new("build");
        observed.queued_jobs = 5;
        observed.running_jobs = 2;
        observed.failed_jobs = 3;
        model.pool_activity.pools = vec![observed, PoolRollup::new("idle-pool")];
        model.pool_activity.repos = vec![RepoActivity {
            repo: "owner/repo".into(),
            ..RepoActivity::default()
        }];
        // Independent per-node observations remain visible even though the
        // aggregate registered capacity cannot be verified.
        model.runners = sample_read_model().runners;
        let out = ink(180, 40, &model);
        assert!(out.contains("capacity unknown"));
        assert!(out.contains("5 queued"));
        assert!(out.contains("2 running"));
        assert!(out.contains("3 failed"));
        assert!(out.contains("build"));
        assert!(out.contains("idle-pool"));
        assert!(out.contains("5/2/3"));
        assert!(out.contains("0/0/0"));
        assert!(out.contains("UNKNOWN"));
        assert!(out.contains("oci-runner-1"));
        assert!(!out.contains("SATURATED"));
        assert!(!out.to_lowercase().contains("fleet healthy"));
        assert!(!out.contains("% util"));
        assert!(!out.contains("0%"));
        assert!(!out.contains("0 online"));
        assert!(!out.contains("0/0idle"));
    }

    #[test]
    fn renders_unknown_capacity_without_observations() {
        let out = ink(140, 24, &unknown_capacity_model());
        assert!(out.contains("capacity unknown"));
        assert!(out.contains("UNKNOWN"));
        assert!(out.contains("No runner pools"));
        assert!(out.contains("No per-node runner telemetry"));
        assert!(!out.to_lowercase().contains("fleet healthy"));
        assert!(!out.contains("SATURATED"));
        assert!(!out.contains("% util"));
        assert!(!out.contains("0 online"));
    }

    #[test]
    fn renders_known_capacity_and_healthy_pool() {
        let mut model = TuiReadModel::default();
        let mut pool = PoolRollup::new("build");
        pool.active_slots = 4;
        pool.running_jobs = 1;
        pool.online_runners = 4;
        model.pool_activity.pools = vec![pool];
        let out = ink(180, 40, &model);
        assert!(out.contains("25% util"));
        assert!(out.contains("4 online"));
        assert!(out.contains("4/3idle"));
        assert!(out.contains("0/1/0"));
        assert!(out.contains("Fleet healthy"));
        assert!(out.contains("fleet healthy"));
        assert!(!out.contains("capacity unknown"));
        assert!(!out.contains("SATURATED"));
    }
}
