use super::*;
use crate::tui::{app::App, ui};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

fn draw_once(app: &mut App) -> Result<()> {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|f| ui::draw(f, app))?;
    Ok(())
}

fn capture_buffer(app: &mut App) -> Result<Buffer> {
    let backend = TestBackend::new(120, 40);
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

fn job(job_id: i64, status: &str) -> crate::state::JobEvent {
    crate::state::JobEvent {
        job_id,
        project_id: 2,
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
        crate::tui::app::ActiveTab::Secrets,
        crate::tui::app::ActiveTab::LLMs,
        crate::tui::app::ActiveTab::Git,
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
    let buffer = capture_buffer(&mut app)?;
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
            .map(|line| line.contains("Activity / Logs"))
            .unwrap_or(false),
        "fullscreen activity view should sit directly under the header"
    );
    assert!(
        lines.iter().any(|line| line.contains("[esc]")),
        "fullscreen activity view should expose the esc affordance"
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
        project_id: 2,
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
async fn renders_jobs_tab_with_live_jobs_and_no_empty_message() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.state.recent_jobs = vec![
        job(1, "running"),
        job(2, "waiting_for_resource"),
        job(3, "preparing"),
    ];
    app.active_tab = crate::tui::app::ActiveTab::Jobs;
    app.active_pane = crate::tui::app::ActivePane::Jobs;

    let buffer = capture_buffer(&mut app)?;
    let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    assert!(rendered.contains("test-job-1"));
    assert!(rendered.contains("○"));
    assert!(!rendered.contains("Waiting for active pipelines"));
    Ok(())
}

#[tokio::test]
async fn renders_cached_pools_with_pool_sync_warning() -> Result<()> {
    let mut app = crate::tui::app::test_app().await?;
    app.active_tab = crate::tui::app::ActiveTab::Pools;
    app.state.pools = vec![pool("linux-large", false), pool("linux-paused", true)];
    app.state.pool_sync_error = Some("pool sync failed: redline unavailable".into());

    let buffer = capture_buffer(&mut app)?;
    let rendered: String = buffer.content.iter().map(|cell| cell.symbol()).collect();

    assert!(rendered.contains("pools:1/2 stale"));
    assert!(rendered.contains("Runner Pools (2 cached) stale"));
    assert!(rendered.contains("linux-large"));
    assert!(rendered.contains("Pool sync stale"));
    assert!(!rendered.contains("Runner Pools (0)"));
    assert!(!rendered.contains("No pool selected."));
    Ok(())
}

#[tokio::test]
async fn renders_llms_tab_with_redacted_secret_values() -> Result<()> {
    let td = tempfile::tempdir()?;
    let autonomy_dir = td.path().join(".autonomy");
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
    assert!(rendered.contains("nvidia/nemotron-3-super-120b-a12b:free"));
    assert!(rendered.contains("cli"));
    assert!(rendered.contains("ready"));
    assert!(!rendered.contains("super-secret-value"));

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
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Secrets);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::LLMs);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Git);
    app.cycle_tab_next();
    assert_eq!(app.active_tab, crate::tui::app::ActiveTab::Workflow);

    assert_eq!(app.active_pane, crate::tui::app::ActivePane::Jobs);
    app.cycle_pane_next();
    assert_eq!(app.active_pane, crate::tui::app::ActivePane::Jobs);
    Ok(())
}
