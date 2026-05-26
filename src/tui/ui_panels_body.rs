use super::*;
use crate::tui::app::ReleaseSubPane;

#[path = "ui_panels_body_evidence.rs"]
mod body_evidence;
use body_evidence::draw_release_evidence_pane;

pub(crate) fn draw_release_tab(f: &mut Frame, app: &mut App, area: Rect) {
    // Top strip: sub-pane selector (1/2/3 or h/l to cycle).
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(8)])
        .split(area);

    focus::register_pane(app, PaneId::ReleaseSelector, split[0]);
    focus::register_drill_esc_hotspot(app, PaneId::ReleaseSelector, split[0]);
    draw_release_subpane_tabs(f, app, split[0]);

    match app.release_subpane {
        ReleaseSubPane::Pipeline => {
            focus::register_pane(app, PaneId::ReleasePipeline, split[1]);
            focus::register_drill_esc_hotspot(app, PaneId::ReleasePipeline, split[1]);
            draw_release_pipeline_pane(f, app, split[1])
        }
        ReleaseSubPane::Evidence => {
            focus::register_pane(app, PaneId::ReleaseInspector, split[1]);
            focus::register_drill_esc_hotspot(app, PaneId::ReleaseInspector, split[1]);
            draw_release_evidence_pane(f, app, split[1])
        }
        ReleaseSubPane::Rollback => {
            focus::register_pane(app, PaneId::ReleaseRollback, split[1]);
            focus::register_drill_esc_hotspot(app, PaneId::ReleaseRollback, split[1]);
            draw_release_rollback_pane(f, app, split[1])
        }
    }
}

fn draw_release_subpane_tabs(f: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span> = vec![Span::styled(
        format!(
            " release · project {} · veox-deploy ",
            crate::release::DEFAULT_RELEASE_PROJECT_ID
        ),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )];
    for (i, pane) in [
        ReleaseSubPane::Pipeline,
        ReleaseSubPane::Evidence,
        ReleaseSubPane::Rollback,
    ]
    .iter()
    .enumerate()
    {
        let style = if *pane == app.release_subpane {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            Style::default().fg(Color::Gray)
        };
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!(" [{}] {} ", i + 1, pane.label()),
            style,
        ));
    }
    spans.push(Span::raw("   "));
    spans.push(Span::styled(
        "(1/2/3 or h/l to cycle)",
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(
        Paragraph::new(Line::from(spans)).block(Block::default()),
        area,
    );
}

fn draw_release_pipeline_pane(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(6)])
        .split(area);
    let summary = release_surface_summary(app);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " project 48 · veox-deploy ",
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(" ", Style::default()),
            Span::styled(summary, Style::default().fg(Color::Green)),
        ])),
        rows[0],
    );

    let snap = &app.state.release_stages;

    // When release stages are populated from ops/releases/, show them.
    // Otherwise fall back to CI run history from recent_jobs.
    if snap.total() > 0 {
        draw_release_stages_columns(f, app, snap, rows[1]);
    } else {
        draw_ci_run_history_columns(f, app, rows[1]);
    }
}

/// Original release stages column view — shows cards from `ops/releases/*/release-attempt.json`.
fn draw_release_stages_columns(
    f: &mut Frame,
    app: &App,
    snap: &crate::tui::app::ReleaseStageSnapshot,
    area: Rect,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(area);

    let stages: [(&str, &Vec<crate::tui::app::ReleaseStageCard>, Color); 5] = [
        ("Plan", &snap.plan, Color::Blue),
        ("Build", &snap.build, Color::Cyan),
        ("Proof", &snap.proof, Color::Yellow),
        ("Canary", &snap.canary, Color::Magenta),
        ("Stable", &snap.stable, Color::Green),
    ];

    let chrome = focus::pane_chrome(app, PaneId::ReleasePipeline);
    for (i, (name, cards, color)) in stages.iter().enumerate() {
        let title_label = format!("{} [{}]", name, cards.len());
        let title = if i == 0 {
            chrome.title(&title_label)
        } else {
            crate::tui::focus::title_with_esc(&title_label, false)
        };
        let items: Vec<ListItem> = cards
            .iter()
            .map(|c| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", c.label), Style::default().fg(*color)),
                    Span::styled(format!("{} ", c.agent_id), Style::default().fg(Color::Gray)),
                    Span::styled(
                        format!("({}) ", c.age),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            })
            .collect();
        let list = List::new(items).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(chrome.border_style),
        );
        f.render_widget(list, cols[i]);
    }
}

/// CI run history columns — shows recent job runs bucketed into pipeline stages.
/// Each column shows pass/fail/running indicators so operators can see where
/// active work and bottlenecks reside.
fn draw_ci_run_history_columns(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(area);

    // Bucket recent jobs into stages by matching job name / pool name patterns.
    let jobs = &app.state.recent_jobs;
    let mut plan_jobs = Vec::new();
    let mut build_jobs = Vec::new();
    let mut proof_jobs = Vec::new();
    let mut canary_jobs = Vec::new();
    let mut stable_jobs = Vec::new();

    for job in jobs {
        let name = job
            .job_name
            .as_deref()
            .or(job.pool_name.as_deref())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.contains("lint")
            || name.contains("plan")
            || name.contains("format")
            || name.contains("check")
            || name.contains("clippy")
        {
            plan_jobs.push(job);
        } else if name.contains("build")
            || name.contains("compile")
            || name.contains("cargo")
            || name.contains("package")
        {
            build_jobs.push(job);
        } else if name.contains("test")
            || name.contains("proof")
            || name.contains("verify")
            || name.contains("audit")
            || name.contains("nextest")
        {
            proof_jobs.push(job);
        } else if name.contains("canary")
            || name.contains("staging")
            || name.contains("preview")
            || name.contains("deploy-dev")
        {
            canary_jobs.push(job);
        } else if name.contains("deploy")
            || name.contains("release")
            || name.contains("stable")
            || name.contains("prod")
        {
            stable_jobs.push(job);
        } else {
            // Default unmatched jobs to the build column.
            build_jobs.push(job);
        }
    }

    let stage_cols: [(&str, &[&crate::state::JobEvent], Color); 5] = [
        ("Plan", &plan_jobs, Color::Blue),
        ("Build", &build_jobs, Color::Cyan),
        ("Proof", &proof_jobs, Color::Yellow),
        ("Canary", &canary_jobs, Color::Magenta),
        ("Stable", &stable_jobs, Color::Green),
    ];

    let chrome = focus::pane_chrome(app, PaneId::ReleasePipeline);
    for (i, (name, stage_jobs, color)) in stage_cols.iter().enumerate() {
        let passed = stage_jobs.iter().filter(|j| j.status == "success").count();
        let failed = stage_jobs
            .iter()
            .filter(|j| j.status == "failed" || j.status == "cancelled")
            .count();
        let running = stage_jobs
            .iter()
            .filter(|j| {
                matches!(
                    j.status.as_str(),
                    "running" | "pending" | "created" | "waiting_for_resource"
                )
            })
            .count();
        let total = stage_jobs.len();

        let title_label = format!("{} [{}]", name, total);
        let title = if i == 0 {
            chrome.title(&title_label)
        } else {
            crate::tui::focus::title_with_esc(&title_label, false)
        };

        // Build item list: summary line + individual runs
        let mut items: Vec<ListItem> = Vec::new();

        // Summary bar
        if total > 0 {
            let mut summary_spans = vec![Span::raw(" ")];
            if passed > 0 {
                summary_spans.push(Span::styled(
                    format!("✓{} ", passed),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            if failed > 0 {
                summary_spans.push(Span::styled(
                    format!("✗{} ", failed),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ));
            }
            if running > 0 {
                summary_spans.push(Span::styled(
                    format!("●{} ", running),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            items.push(ListItem::new(Line::from(summary_spans)));
        }

        // Individual job runs (most recent first, already sorted)
        for job in stage_jobs.iter().take(12) {
            let status_icon = match job.status.as_str() {
                "success" => Span::styled(" ✓ ", Style::default().fg(Color::Green)),
                "failed" => Span::styled(" ✗ ", Style::default().fg(Color::Red)),
                "cancelled" => Span::styled(" ○ ", Style::default().fg(Color::DarkGray)),
                "running" => Span::styled(" ● ", Style::default().fg(Color::Yellow)),
                "pending" | "created" => Span::styled(" ◌ ", Style::default().fg(Color::Gray)),
                _ => Span::styled(" ? ", Style::default().fg(Color::DarkGray)),
            };
            let job_label = job.job_name.as_deref().unwrap_or("job");
            // Truncate long names to fit the column
            let label_display: String = job_label.chars().take(18).collect();
            items.push(ListItem::new(Line::from(vec![
                status_icon,
                Span::styled(format!("{} ", label_display), Style::default().fg(*color)),
            ])));
        }

        if items.is_empty() {
            items.push(ListItem::new(Line::from(vec![Span::styled(
                " (no runs)",
                Style::default().fg(Color::DarkGray),
            )])));
        }

        let list = List::new(items).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(chrome.border_style),
        );
        f.render_widget(list, cols[i]);
    }
}

fn draw_release_rollback_pane(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(4)])
        .split(area);
    let summary = release_surface_summary(app);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " project 48 · veox-deploy ",
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(" ", Style::default()),
            Span::styled(summary, Style::default().fg(Color::Green)),
        ])),
        rows[0],
    );

    let ladder = crate::release::default_ladder();
    let items: Vec<ListItem> = ladder
        .iter()
        .map(|s| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" [{}] ", s.n),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{:<13} ", s.kind), Style::default().fg(Color::Cyan)),
                Span::raw(s.description.clone()),
            ]))
        })
        .collect();
    let chrome = focus::pane_chrome(app, PaneId::ReleaseRollback);
    let list = List::new(items).block(
        Block::default()
            .title(chrome.title("Rollback ladder"))
            .borders(Borders::ALL)
            .border_style(chrome.border_style),
    );
    f.render_widget(list, rows[1]);
}

fn release_surface_summary(app: &App) -> String {
    let Some(rel) = app.state.release_status.as_ref() else {
        return "canary: waiting · prod: waiting · rollback: ready".into();
    };
    let canary = match rel.attempt.canary_status.as_str() {
        "passed" | "e2e-passed" | "green" | "released" => "ready".to_string(),
        "running" | "canary-authorized" | "in-flight" => "in flight".to_string(),
        other => other.to_string(),
    };
    let prod = match rel.attempt.production_pipeline_status.as_deref() {
        Some("success") => "ready".to_string(),
        Some("running" | "pending" | "created" | "manual") => "in flight".to_string(),
        Some(other) => other.to_string(),
        None => "waiting".to_string(),
    };
    let rollback = if matches!(
        rel.attempt.canary_status.as_str(),
        "passed" | "e2e-passed" | "green" | "released"
    ) {
        "ready".to_string()
    } else {
        "waiting".to_string()
    };
    format!("canary: {canary} · prod: {prod} · rollback: {rollback}")
}

pub(crate) fn draw_release_inspector(f: &mut Frame, app: &App, area: Rect) {
    let chrome = focus::pane_chrome(app, PaneId::ReleaseInspector);
    let block = Block::default()
        .title(chrome.title("Inspector"))
        .borders(Borders::ALL)
        .border_style(chrome.border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let content = if let Some(ref rel) = app.state.release_status {
        let attempt = &rel.attempt;
        format!(
            "sha: {}\nref: {}\n\ncanary_url:\n{}\n\nnote:\n{}\n\neligibility:\n{}",
            attempt.sha.get(..12).unwrap_or(&attempt.sha),
            attempt.ref_name,
            rel.canary_public_url.as_deref().unwrap_or("n/a"),
            attempt.canary_note.as_deref().unwrap_or("(none)"),
            rel.eligibility,
        )
    } else {
        "No release attempt.\n\nStart one with:\n  jeryu release start".to_string()
    };

    f.render_widget(
        Paragraph::new(content)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

// ---------------------------------------------------------------------------
// Tab 3 — Jobs: flow board + jobs list + log preview
// ---------------------------------------------------------------------------

pub(crate) fn draw_jobs_tab(f: &mut Frame, app: &mut App, area: Rect) {
    // TUI v2 — Split layout: Live Feed (60%) | Progress+Matrix (40%)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60), // Left: Live Runner Feed
            Constraint::Percentage(40), // Right: Progress + Matrix + Inspector
        ])
        .split(area);

    // Left column: Live Runner Feed
    focus::register_pane(app, PaneId::JobsRunnerFeed, cols[0]);
    focus::register_drill_esc_hotspot(app, PaneId::JobsRunnerFeed, cols[0]);
    draw_live_runner_feed(f, app, cols[0]);

    // Right column: Pipeline Progress on top, Job Matrix below, Inspector at bottom
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12), // Pipeline progress
            Constraint::Min(8),     // Job matrix
            Constraint::Length(10), // Inspector
        ])
        .split(cols[1]);

    focus::register_pane(app, PaneId::JobsProgress, right_rows[0]);
    focus::register_pane(app, PaneId::JobsMatrix, right_rows[1]);
    focus::register_pane(app, PaneId::JobsInspector, right_rows[2]);
    focus::register_drill_esc_hotspot(app, PaneId::JobsProgress, right_rows[0]);
    focus::register_drill_esc_hotspot(app, PaneId::JobsMatrix, right_rows[1]);
    focus::register_drill_esc_hotspot(app, PaneId::JobsInspector, right_rows[2]);

    draw_pipeline_progress(f, app, right_rows[0]);
    draw_job_matrix(f, app, right_rows[1]);
    draw_job_inspector_panel(f, app, right_rows[2]);
}

// ---------------------------------------------------------------------------
// TUI v2 — Live Runner Feed
// ---------------------------------------------------------------------------

#[path = "ui_panels_body_live_feed.rs"]
mod ui_panels_body_live_feed;
pub(crate) use ui_panels_body_live_feed::*;
