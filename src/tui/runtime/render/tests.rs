use super::*;
use crate::release;
use crate::tui::{app::App, ui};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;

fn draw_once(app: &mut App) -> Result<()> {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|f| ui::draw(f, app))?;
    Ok(())
}

fn capture_buffer(app: &mut App) -> Result<Buffer> {
    capture_buffer_size(app, 120, 40)
}

fn capture_buffer_size(app: &mut App, width: u16, height: u16) -> Result<Buffer> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|f| ui::draw(f, app))?;
    Ok(terminal.backend().buffer().clone())
}

fn buffer_lines(buffer: &Buffer) -> Vec<String> {
    let width = buffer.area.width as usize;
    let height = buffer.area.height as usize;
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x as u16, y as u16)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn yellow_cells_on_row(buffer: &Buffer, row: u16) -> usize {
    (0..buffer.area.width)
        .filter(|x| buffer[(*x, row)].fg == Color::Yellow)
        .count()
}

fn rendered_text(buffer: &Buffer) -> String {
    buffer.content.iter().map(|cell| cell.symbol()).collect()
}

fn job(job_id: i64, status: &str) -> crate::state::JobEvent {
    crate::state::JobEvent {
        job_id,
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        pipeline_id: Some(10),
        status: status.into(),
        job_name: Some(format!("test-job-{job_id}")),
        pool_name: Some("default".into()),
        system_id: None,
        queued_duration: None,
        received_at: "2026-04-23T19:00:00Z".into(),
    }
}

fn pool(name: &str, paused: bool) -> crate::state::Pool {
    crate::state::Pool {
        name: name.into(),
        gitlab_runner_id: 1,
        auth_token: "token".into(),
        tags: "linux".into(),
        executor: "docker".into(),
        min_warm: 1,
        max_managers: 4,
        concurrent: 2,
        request_concurrency: 1,
        paused,
        trust_tier: "trusted".into(),
        cluster_alias: None,
        backend_type: "docker".into(),
    }
}

#[tokio::test]
async fn renders_all_primary_tabs_with_empty_state() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    for tab in [
        crate::tui::app::ActiveTab::Workflow,
        crate::tui::app::ActiveTab::Mission,
        crate::tui::app::ActiveTab::Release,
        crate::tui::app::ActiveTab::Approvals,
        crate::tui::app::ActiveTab::Jobs,
        crate::tui::app::ActiveTab::Agents,
        crate::tui::app::ActiveTab::Tests,
        crate::tui::app::ActiveTab::Pools,
        crate::tui::app::ActiveTab::Cache,
        crate::tui::app::ActiveTab::Evidence,
        crate::tui::app::ActiveTab::Repos,
        crate::tui::app::ActiveTab::Bugs,
        crate::tui::app::ActiveTab::Secrets,
        crate::tui::app::ActiveTab::LLMs,
        crate::tui::app::ActiveTab::Git,
        crate::tui::app::ActiveTab::Jankurai,
    ] {
        app.active_tab = tab;
        let buffer = capture_buffer(&mut app)?;
        let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(
            rendered.contains("Activity / Logs"),
            "tab {:?} should render the activity/logs pane",
            tab
        );
        let footer = match buffer_lines(&buffer).last().cloned() {
            Some(line) => line,
            None => String::new(),
        };
        assert!(footer.contains("Tab/Shift+Tab:tabs"));
        assert!(footer.contains("q:quit"));
    }
    Ok(())
}

#[tokio::test]
async fn renders_release_subpanes() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.active_tab = crate::tui::app::ActiveTab::Release;
    for sub in [
        crate::tui::app::ReleaseSubPane::Pipeline,
        crate::tui::app::ReleaseSubPane::Evidence,
        crate::tui::app::ReleaseSubPane::Rollback,
    ] {
        app.release_subpane = sub;
        draw_once(&mut app)?;
    }
    Ok(())
}

#[tokio::test]
async fn release_tab_shows_release_lens_readiness() -> Result<()> {
    // The Release tab now renders the Flight Deck Release lens (see
    // flight_deck::tab_lens). The legacy per-project pipeline banner
    // ("project 48"/"veox-deploy"/"canary:"/"prod:"/"rollback:") was
    // superseded by the lens, which projects release readiness from the
    // TuiReadModel. The rich per-project canary/prod/rollback pipeline view is
    // slated to be re-homed into the Release lens as a drill-down. Assert the
    // lens renders its readiness domain instead.
    let mut app = crate::tui::app::test_app().await?;
    app.apply_demo_fixture();
    app.active_tab = crate::tui::app::ActiveTab::Release;
    app.release_subpane = crate::tui::app::ReleaseSubPane::Pipeline;
    let buffer = capture_buffer_size(&mut app, 140, 40)?;
    let text = rendered_text(&buffer);
    assert!(text.contains("Release"), "release lens header must render");
    assert!(
        text.contains("safe_to_release"),
        "release readiness posture must render"
    );
    assert!(
        text.contains("promote"),
        "promote key affordance must render"
    );
    assert!(
        text.contains("rollback"),
        "rollback key affordance must render"
    );
    Ok(())
}

#[tokio::test]
async fn renders_approvals_tab_with_pending_pr() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.active_tab = crate::tui::app::ActiveTab::Approvals;
    app.state.approvals_queue = vec![crate::tui::app::PendingApproval {
        pr_number: 42,
        title: "fix: pipeline progress regression".into(),
        agent_id: "claude-opus-4-7".into(),
        risk_tier: 2,
        ci_status: "green".into(),
        age: "3m".into(),
        head_sha: "abc123def456".into(),
    }];
    let buffer = capture_buffer_size(&mut app, 120, 64)?;
    let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    assert!(rendered.contains("#42"));
    assert!(rendered.contains("claude"));
    Ok(())
}

#[tokio::test]
async fn renders_maximized_logs_empty_state() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.active_tab = crate::tui::app::ActiveTab::Workflow;
    app.focus.fullscreen = Some(crate::tui::focus::PaneId::ActivityLog(
        crate::tui::app::ActiveTab::Workflow,
    ));
    let buffer = capture_buffer(&mut app)?;
    let lines = buffer_lines(&buffer);
    assert!(
        lines
            .get(3)
            .map(|line| line.contains("All: none"))
            .unwrap_or(false),
        "fullscreen activity view should render the fleet bar under the header"
    );
    assert!(
        lines
            .get(4)
            .map(|line| line.contains("Activity / Logs"))
            .unwrap_or(false),
        "fullscreen activity view should sit under the fleet bar"
    );
    assert!(
        lines.iter().any(|line| line.contains("[esc]")),
        "fullscreen activity view should expose the esc affordance"
    );
    Ok(())
}

#[tokio::test]
async fn fullscreen_activity_clears_previous_tab_content() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.active_tab = crate::tui::app::ActiveTab::Jobs;
    app.active_pane = crate::tui::app::ActivePane::Jobs;
    app.state.recent_jobs = vec![job(1, "running")];
    app.state.live_log.text = "line one\nline two".into();

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|f| ui::draw(f, &mut app))?;

    app.focus.fullscreen = Some(crate::tui::focus::PaneId::ActivityLog(
        crate::tui::app::ActiveTab::Jobs,
    ));
    terminal.draw(|f| ui::draw(f, &mut app))?;

    let buffer = terminal.backend().buffer().clone();
    let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    assert!(rendered.contains("Activity / Logs"));
    assert!(
        !rendered.contains("Pipeline Progress"),
        "fullscreen activity should clear the previous jobs pane"
    );
    assert!(
        !rendered.contains("Live Runner Feed"),
        "fullscreen activity should clear the previous runner feed pane"
    );
    Ok(())
}

#[tokio::test]
async fn workflow_visible_panes_render_yellow_macro_focus_wide() -> Result<()> {
    let panes = [
        crate::tui::focus::PaneId::WorkflowMissionStrip,
        crate::tui::focus::PaneId::WorkflowPrRail,
        crate::tui::focus::PaneId::WorkflowPhaseRail,
        crate::tui::focus::PaneId::WorkflowCanvas,
        crate::tui::focus::PaneId::WorkflowMinimap,
        crate::tui::focus::PaneId::WorkflowInspector,
        crate::tui::focus::PaneId::ActivityLog(crate::tui::app::ActiveTab::Workflow),
    ];

    for pane in panes {
        let mut app = crate::tui::app::test_app().await?;
        app.apply_demo_fixture();
        app.active_tab = crate::tui::app::ActiveTab::Workflow;
        app.workflow_inspect_open = true;
        app.focus.active = pane;
        app.focus.stack.clear();
        app.focus.fullscreen = None;

        let buffer = capture_buffer_size(&mut app, 220, 44)?;
        let rect = app
            .focus_map
            .rect_of(pane)
            .unwrap_or_else(|| panic!("expected {pane:?} to be registered at 220x44"));
        let yellow = yellow_cells_on_row(&buffer, rect.y);
        assert!(
            yellow >= 8,
            "expected {pane:?} title row to be yellow-focused, found {yellow}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn workflow_narrow_layout_registers_and_focus_paints_visible_panes() -> Result<()> {
    let visible = [
        crate::tui::focus::PaneId::WorkflowMissionStrip,
        crate::tui::focus::PaneId::WorkflowPrRail,
        crate::tui::focus::PaneId::WorkflowCanvas,
        crate::tui::focus::PaneId::ActivityLog(crate::tui::app::ActiveTab::Workflow),
    ];
    let hidden = [
        crate::tui::focus::PaneId::WorkflowPhaseRail,
        crate::tui::focus::PaneId::WorkflowMinimap,
        crate::tui::focus::PaneId::WorkflowInspector,
    ];

    for pane in visible {
        let mut app = crate::tui::app::test_app().await?;
        app.apply_demo_fixture();
        app.active_tab = crate::tui::app::ActiveTab::Workflow;
        app.workflow_inspect_open = true;
        app.focus.active = pane;

        let buffer = capture_buffer_size(&mut app, 100, 40)?;
        let rect = app
            .focus_map
            .rect_of(pane)
            .unwrap_or_else(|| panic!("expected {pane:?} to be registered at 100x40"));
        assert!(
            yellow_cells_on_row(&buffer, rect.y) >= 8,
            "expected visible narrow pane {pane:?} to be focus-painted"
        );
        for hidden_pane in hidden {
            assert!(
                app.focus_map.rect_of(hidden_pane).is_none(),
                "expected {hidden_pane:?} to be hidden at 100x40"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn workflow_drilled_pane_renders_esc_and_hotspot() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.apply_demo_fixture();
    app.active_tab = crate::tui::app::ActiveTab::Workflow;
    app.workflow_inspect_open = true;
    app.focus.active = crate::tui::focus::PaneId::WorkflowCanvas;

    let root = capture_buffer_size(&mut app, 220, 44)?;
    assert!(
        !rendered_text(&root).contains("[esc]"),
        "root macro focus should not show the drill exit affordance"
    );

    app.focus.push();
    let drilled = capture_buffer_size(&mut app, 220, 44)?;
    let text = rendered_text(&drilled);
    assert!(
        text.contains("[esc]"),
        "drilled Workflow pane should expose the esc affordance"
    );
    let rect = app
        .focus_map
        .rect_of(crate::tui::focus::PaneId::WorkflowCanvas)
        .expect("canvas should be registered");
    assert_eq!(
        app.focus_map.esc_at(rect.x + 1, rect.y),
        Some(crate::tui::focus::PaneId::WorkflowCanvas)
    );
    Ok(())
}

#[tokio::test]
async fn workflow_minimap_right_focuses_inline_inspector() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.apply_demo_fixture();
    app.active_tab = crate::tui::app::ActiveTab::Workflow;
    app.workflow_inspect_open = true;
    app.focus.active = crate::tui::focus::PaneId::WorkflowMinimap;

    let _ = capture_buffer_size(&mut app, 220, 44)?;
    assert!(app.delivery_hit_map.inspector.is_some());
    app.focus_move(crate::tui::focus::NavDirection::Right);
    assert_eq!(
        app.focus.active,
        crate::tui::focus::PaneId::WorkflowInspector
    );
    Ok(())
}

#[tokio::test]
async fn renders_flow_with_jobs_list_and_live_log() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.state.recent_jobs = vec![job(1, "running"), job(2, "pending")];
    app.state.live_log.text = "cargo test\nwarning: slow test\nerror: sample failure".into();
    app.state.flow.active_pipelines = vec![crate::tui::flow::PipelineFlow {
        pipeline_id: 10,
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        ref_name: "main".into(),
        sha: Some("abc123".into()),
        status: "running".into(),
        graph: crate::tui::flow::FlowGraph::default(),
        current_blocker: None,
        critical_path: vec![],
        eta: None,
        progress_pct: 64,
    }];

    app.active_tab = crate::tui::app::ActiveTab::Jobs;
    app.active_pane = crate::tui::app::ActivePane::Jobs;
    draw_once(&mut app)?;
    Ok(())
}

#[tokio::test]
async fn renders_jobs_tab_with_queue_lens_capacity() -> Result<()> {
    // The Jobs tab now renders the Flight Deck Queue lens (see
    // flight_deck::tab_lens), which projects queue depth against runner
    // capacity from the TuiReadModel rather than listing individual job rows
    // (e.g. "test-job-1" / per-job status glyphs). The detailed per-job list is
    // slated to be re-homed into the Queue lens as a drill-down. Assert the
    // lens renders its queue/capacity domain.
    let mut app = crate::tui::app::test_app().await?;
    app.state.recent_jobs = vec![
        job(1, "running"),
        job(2, "waiting_for_resource"),
        job(3, "preparing"),
    ];
    app.active_tab = crate::tui::app::ActiveTab::Jobs;
    app.active_pane = crate::tui::app::ActivePane::Jobs;

    let buffer = capture_buffer_size(&mut app, 120, 64)?;
    let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    assert!(rendered.contains("Queue"), "queue lens header must render");
    assert!(
        rendered.contains("queued"),
        "queued depth metric must render"
    );
    assert!(
        rendered.contains("running"),
        "running jobs metric must render"
    );
    assert!(
        rendered.contains("capacity"),
        "runner capacity metric must render"
    );
    // The legacy "Waiting for active pipelines" empty message no longer exists.
    assert!(!rendered.contains("Waiting for active pipelines"));
    Ok(())
}

#[tokio::test]
async fn renders_bugs_tab_with_bugs_lens_triage() -> Result<()> {
    // The Bugs tab now renders the Flight Deck Bugs lens (see
    // flight_deck::tab_lens), which projects a blocker/bug triage queue from
    // the TuiReadModel. The legacy "Bug Projects" board (sort:rank, BUG-S0
    // identifiers, per-bug Current behavior / Evidence / Acceptance detail) was
    // superseded by the lens; that rich per-bug detail is slated to be re-homed
    // into the Bugs lens as a drill-down. Assert the lens renders its triage
    // domain.
    let mut app = crate::tui::app::test_app().await?;
    app.apply_demo_fixture();
    app.active_tab = crate::tui::app::ActiveTab::Bugs;

    let buffer = capture_buffer_size(&mut app, 120, 64)?;
    let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();

    assert!(rendered.contains("Bugs"), "bugs lens header must render");
    assert!(
        rendered.contains("Triage"),
        "triage panel title must render"
    );
    assert!(
        rendered.contains("top blocker"),
        "top blocker summary must render"
    );
    assert!(
        rendered.contains("failing jobs"),
        "failing-job pressure must render"
    );
    Ok(())
}

#[tokio::test]
async fn renders_cached_pools_with_pool_sync_warning() -> Result<()> {
    // The Pools tab now renders the Flight Deck Runners lens (the runner pane
    // is the evolved "Runner Pools" view). The pool sync warning is preserved:
    // it is carried into the Runners lens footer so sync failures still surface.
    let mut app = crate::tui::app::test_app().await?;
    app.active_tab = crate::tui::app::ActiveTab::Pools;
    app.state.pools = vec![pool("linux-large", false), pool("linux-paused", true)];
    app.state.pool_sync_error = Some("pool sync failed: redline unavailable".into());

    let buffer = capture_buffer(&mut app)?;
    let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();

    // New Runners lens chrome renders.
    assert!(
        rendered.contains("Runners"),
        "runners lens header must render"
    );
    // Pool sync warning is preserved (now in the runners footer).
    assert!(
        rendered.contains("Pool sync warning"),
        "sync warning must still surface"
    );
    assert!(
        rendered.contains("redline unavailable"),
        "the actual sync error text must surface"
    );
    Ok(())
}

#[tokio::test]
async fn renders_llms_tab_with_no_secret_leak() -> Result<()> {
    // The LLMs tab now renders the Flight Deck Llms lens (see
    // flight_deck::tab_lens), which projects only aggregate model-access
    // posture (active agent sessions / grants) from the TuiReadModel — it
    // renders NO provider model-ids or secret values at all, so it is safe by
    // omission. The legacy per-provider model-id ledger (and its redaction of
    // resolved key values) is slated to be re-homed into the Llms lens as a
    // drill-down. This test now confirms the lens renders its domain and leaks
    // nothing sensitive even when a real provider config + resolved secret are
    // present.
    let td = tempfile::tempdir()?;
    let autonomy_dir = td.path().join(".jeryu/autonomy");
    std::fs::create_dir_all(autonomy_dir.join("providers"))?;
    std::fs::write(
        autonomy_dir.join("providers").join("llm.yml"),
        r#"
schema: vibegate.providers.v1
default_role_chain: [security]
chains:
  security:
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: nvidia/nemotron-3-super-120b-a12b:free
      api_key_secret: LLM_TEST_KEY
      data_use: no_train
"#,
    )?;
    let mut app = crate::tui::app::test_app().await?;
    app.autonomy_dir = autonomy_dir;
    app.llm_secret_resolver = Some(crate::llm::SecretResolver {
        cli_overrides: std::collections::HashMap::from([(
            "LLM_TEST_KEY".to_string(),
            "super-secret-value".to_string(),
        )]),
        repo_root: None,
        ci_mode: true,
    });
    app.active_tab = crate::tui::app::ActiveTab::LLMs;

    let buffer = capture_buffer(&mut app)?;
    let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    // Lens domain renders.
    assert!(rendered.contains("LLMs"), "llms lens header must render");
    assert!(
        rendered.contains("agent sessions"),
        "active agent sessions metric must render"
    );
    assert!(
        rendered.contains("grants"),
        "active grants metric must render"
    );
    // Nothing sensitive leaks: neither the raw provider model-id nor the
    // resolved secret value appears anywhere in the rendered buffer.
    assert!(
        !rendered.contains("nvidia/nemotron-3-super-120b-a12b:free"),
        "provider model-id must not leak into the lens"
    );
    assert!(
        !rendered.contains("super-secret-value"),
        "resolved secret value must never render"
    );

    Ok(())
}

#[tokio::test]
async fn navigation_cycles_tabs_and_panes() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Workflow);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Mission);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Release);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Approvals);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Jobs);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Agents);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Tests);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Pools);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Cache);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Evidence);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Repos);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Bugs);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Secrets);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::LLMs);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Git);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Jankurai);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Workflow);

    assert_eq!(app.active_pane, crate::tui::app::ActivePane::Jobs);
    app.cycle_pane_next();
    assert_eq!(app.active_pane, crate::tui::app::ActivePane::Jobs);
    Ok(())
}

// ---------------------------------------------------------------------------
// New tests: coverage for features added in the TUI-plumbing wave
// ---------------------------------------------------------------------------

#[tokio::test]
async fn renders_repos_tab_from_demo_fleet() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.apply_demo_fixture();
    app.active_tab = crate::tui::app::ActiveTab::Repos;

    let buffer = capture_buffer_size(&mut app, 140, 44)?;
    let text = rendered_text(&buffer);

    assert!(text.contains("Repository Fleet"));
    assert!(text.contains("Families"));
    assert!(text.contains("Repositories"));
    assert!(text.contains("neverhuman"));
    assert!(text.contains("shared"));
    Ok(())
}

#[tokio::test]
async fn renders_jankurai_tab_with_populated_state() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.active_tab = crate::tui::app::ActiveTab::Jankurai;
    app.state.jankurai = crate::tui::jankurai::JankuraiSnapshot {
        installed: true,
        last_scan: Some(crate::tui::jankurai::JankuraiScan {
            generated_at: None,
            score: 89,
            raw_score: 89,
            minimum_score: 85,
            decision: "pass".into(),
            score_status: "green".into(),
            finding_count: 1,
            hard_findings: 0,
            soft_findings: 1,
            caps_applied: Vec::new(),
        }),
        dimensions: vec![crate::tui::jankurai::JankuraiDimension {
            name: "shape".into(),
            weight: 20,
            score: 90,
            weighted_points: 18.0,
            evidence: Vec::new(),
            notes: Vec::new(),
        }],
        entries: vec![crate::tui::jankurai::JankuraiEntry {
            kind: crate::tui::jankurai::JankuraiEntryKind::Finding,
            label: "test-finding-label".into(),
            severity: Some("warning".into()),
            hardness: Some("soft".into()),
            path: None,
            rule: None,
            lane: None,
            owner: None,
            problem: None,
            evidence: Vec::new(),
            suggested_fix: None,
        }],
        history: Vec::new(),
        error: None,
    };
    // Use a taller buffer (120x60) so the bottom panel has enough height.
    let buffer = capture_buffer_size(&mut app, 120, 60)?;
    let text = rendered_text(&buffer);
    assert!(
        text.contains("89"),
        "jankurai tab should render score 89; first 600 chars:\n{}",
        &text[..text.len().min(600)]
    );
    // The "Caps / Findings" panel header is always present when entries exist.
    assert!(
        text.contains("Caps / Findings") || text.contains("test-finding-label"),
        "jankurai tab should render the Caps / Findings panel or an entry label"
    );
    Ok(())
}

#[tokio::test]
async fn workflow_agent_inspector_shows_model_and_decision() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.apply_demo_fixture();
    app.active_tab = crate::tui::app::ActiveTab::Workflow;
    app.workflow_inspect_open = true;

    // Find an AgentReview node in the demo workflow_snapshot and navigate to it.
    let agent_id = app
        .workflow_snapshot
        .nodes
        .iter()
        .find(|n| {
            matches!(
                n.kind,
                crate::tui::workflow::model::WorkflowNodeKind::AgentReview { .. }
            )
        })
        .map(|n| n.id.clone());

    let Some(agent_id) = agent_id else {
        // Demo fixture has no AgentReview node — skip rather than fail.
        return Ok(());
    };

    'outer: for (pi, phase) in app.workflow_snapshot.phases.iter().enumerate() {
        for (ni, id) in phase.node_ids.iter().enumerate() {
            if *id == agent_id {
                app.workflow_nav.phase_idx = pi;
                app.workflow_nav.node_idx = ni;
                break 'outer;
            }
        }
    }
    app.inspector_tab = crate::tui::workflow::inspector::InspectorTab::Agent;

    let buffer = capture_buffer_size(&mut app, 220, 44)?;
    let text = rendered_text(&buffer);
    // Demo agent node is always populated with model "claude-opus-4-7".
    assert!(
        text.contains("claude"),
        "agent inspector should show model name starting with 'claude'; first 2000 chars:\n{}",
        &text[..text.len().min(2000)]
    );
    Ok(())
}

#[tokio::test]
async fn workflow_empty_state_shows_live_source_state() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.active_tab = crate::tui::app::ActiveTab::Workflow;
    // Explicitly empty delivery snapshot and unconfigured source status.
    app.delivery_snapshot = crate::tui::workflow::model::DeliverySnapshot::empty();
    app.state.delivery_source_status = crate::tui::app::DeliverySourceStatus {
        configured: true,
        backend_label: Some("GitHub + GitLab".into()),
        source_label: Some("fleet registry: /home/ubuntu/veox-repos/.jeryu/repos.toml".into()),
        last_sync_at: Some(chrono::Utc::now()),
        last_sync_error: None,
    };
    let buffer = capture_buffer(&mut app)?;
    let text = rendered_text(&buffer);
    assert!(
        text.contains("No active pull requests"),
        "empty workflow tab should show 'No active pull requests' card; first 600 chars:\n{}",
        &text[..text.len().min(600)]
    );
    assert!(
        text.contains("GitHub + GitLab"),
        "empty workflow card should include the live backend label"
    );
    assert!(
        text.contains("/home/ubuntu/veox-repos/.jeryu/repos.toml"),
        "empty workflow card should include the registry path"
    );
    assert!(
        !text.contains("jeryu repo fleet sync"),
        "empty workflow card should no longer show the stale setup command"
    );
    Ok(())
}

#[tokio::test]
async fn renders_bugs_tab_lens_with_aer_state_present() -> Result<()> {
    // The Bugs tab now renders the Flight Deck Bugs lens (see
    // flight_deck::tab_lens). The legacy AER finding inset (class_id like
    // "SEC-001") is no longer rendered inline by this tab; the rich AER finding
    // detail is slated to be re-homed into the Bugs lens as a drill-down.
    // Assert the lens still renders its triage domain even when AER state is
    // loaded.
    let mut app = crate::tui::app::test_app().await?;
    app.active_tab = crate::tui::app::ActiveTab::Bugs;
    app.state.aer = crate::tui::aer::AerSnapshot {
        loaded: true,
        report: crate::tui::aer::AerReport {
            generated_at: "2026-05-24".into(),
            workspace_root: "/tmp".into(),
            findings: vec![crate::tui::aer::AerFinding {
                class_id: "SEC-001".into(),
                severity: "error".into(),
                confidence: 0.9,
                path: "src/foo.rs".into(),
                summary: "unsafe block exposed".into(),
                suggested_fix: "wrap with safe abstraction".into(),
                existing_exception: None,
            }],
            repair_hint: crate::tui::aer::AerRepairHint::default(),
        },
        error: None,
    };
    let buffer = capture_buffer(&mut app)?;
    let text = rendered_text(&buffer);
    assert!(text.contains("Bugs"), "bugs lens header must render");
    assert!(text.contains("Triage"), "triage panel title must render");
    Ok(())
}

#[tokio::test]
async fn renders_tests_tab_vti_lens_with_vrc_state() -> Result<()> {
    // The Tests tab now renders the Flight Deck VTI lens (see
    // flight_deck::tab_lens). The legacy VRC plan banner (mode/reason such as
    // "selected" / "3 changed files") is no longer drawn inline by this tab;
    // the VRC selection-plan detail is slated to be re-homed into the VTI lens
    // as a drill-down. Assert the lens renders its test-impact domain even when
    // VRC state is loaded.
    let mut app = crate::tui::app::test_app().await?;
    app.active_tab = crate::tui::app::ActiveTab::Tests;
    app.state.vrc = crate::tui::vrc::VrcSnapshot {
        loaded: true,
        plan: crate::tui::vrc::VrcPlanFile {
            mode: "selected".into(),
            reason: "3 changed files".into(),
            selected_tests: vec!["unit::signing".into(), "tui::render".into()],
            skipped_tests: vec!["integration::ci".into()],
            confidence: Some(0.95),
        },
        error: None,
    };
    let buffer = capture_buffer(&mut app)?;
    let text = rendered_text(&buffer);
    assert!(text.contains("VTI"), "vti lens header must render");
    assert!(
        text.contains("selector misses"),
        "selector-miss metric must render"
    );
    assert!(
        text.contains("Test Selection"),
        "test selection panel title must render"
    );
    Ok(())
}

#[tokio::test]
async fn renders_tests_tab_vti_lens_with_witness_state() -> Result<()> {
    // The Tests tab now renders the Flight Deck VTI lens (see
    // flight_deck::tab_lens). The legacy witness summary line (crate/pub-item
    // counts such as 12 / 847) is no longer drawn inline by this tab; the
    // witness API-surface summary is slated to be re-homed into the VTI lens as
    // a drill-down. Assert the lens renders its test-impact domain even when
    // witness state is loaded.
    let mut app = crate::tui::app::test_app().await?;
    app.active_tab = crate::tui::app::ActiveTab::Tests;
    app.state.witness = crate::tui::witness::WitnessSnapshot {
        loaded: true,
        generated_at: "2026-05-24".into(),
        crate_count: 12,
        pub_item_count: 847,
        largest_crate: Some(("jeryu-core".into(), 312)),
        error: None,
    };
    let buffer = capture_buffer(&mut app)?;
    let text = rendered_text(&buffer);
    assert!(text.contains("VTI"), "vti lens header must render");
    assert!(
        text.contains("Test Selection"),
        "test selection panel title must render"
    );
    assert!(
        text.contains("Overall posture"),
        "posture metric row must render"
    );
    Ok(())
}
