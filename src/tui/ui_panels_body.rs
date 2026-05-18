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

    focus::register_focus_pane(app, PaneId::ReleaseSelector, split[0]);
    draw_release_subpane_tabs(f, app, split[0]);

    match app.release_subpane {
        ReleaseSubPane::Pipeline => {
            focus::register_focus_pane(app, PaneId::ReleasePipeline, split[1]);
            draw_release_pipeline_pane(f, app, split[1])
        }
        ReleaseSubPane::Evidence => {
            focus::register_focus_pane(app, PaneId::ReleaseInspector, split[1]);
            draw_release_evidence_pane(f, app, split[1])
        }
        ReleaseSubPane::Rollback => {
            focus::register_focus_pane(app, PaneId::ReleaseRollback, split[1]);
            draw_release_rollback_pane(f, app, split[1])
        }
    }
}

fn draw_release_subpane_tabs(f: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span> = vec![Span::styled(
        " release ",
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
        Paragraph::new(Line::from(spans)).block(focus::pane_block(
            app,
            PaneId::ReleaseSelector,
            " release ",
        )),
        area,
    );
}

fn draw_release_pipeline_pane(f: &mut Frame, app: &App, area: Rect) {
    let snap = &app.state.release_stages;
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

    for (i, (name, cards, color)) in stages.iter().enumerate() {
        let title = format!(" {} [{}] ", name, cards.len());
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
        let list = List::new(items).block(focus::pane_block(app, PaneId::ReleasePipeline, title));
        f.render_widget(list, cols[i]);
    }
}

fn draw_release_rollback_pane(f: &mut Frame, app: &App, area: Rect) {
    let _ = app;
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
    let list = List::new(items).block(focus::pane_block(
        app,
        PaneId::ReleaseRollback,
        " [ Rollback ladder ] ",
    ));
    f.render_widget(list, area);
}

pub(crate) fn draw_release_inspector(f: &mut Frame, app: &App, area: Rect) {
    let block = focus::pane_block(app, PaneId::ReleaseInspector, " [ Inspector ] ");
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
        "No release attempt.\n\nActions available:\n  n/a".to_string()
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
    focus::register_focus_pane(app, PaneId::JobsRunnerFeed, cols[0]);
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

    focus::register_focus_pane(app, PaneId::JobsProgress, right_rows[0]);
    focus::register_focus_pane(app, PaneId::JobsMatrix, right_rows[1]);
    focus::register_focus_pane(app, PaneId::JobsInspector, right_rows[2]);

    draw_pipeline_progress(f, app, right_rows[0]);
    draw_job_matrix(f, app, right_rows[1]);
    draw_job_inspector_panel(f, app, right_rows[2]);
}

// ---------------------------------------------------------------------------
// TUI v2 — Live Runner Feed
// ---------------------------------------------------------------------------

#[path = "ui_panels_body_runtime_extra.rs"]
mod ui_panels_body_runtime_extra;
pub(crate) use ui_panels_body_runtime_extra::*;
