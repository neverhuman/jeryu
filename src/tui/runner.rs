//! Owner: Interactive TUI subsystem - runtime entrypoints
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: TUI entry points preserve terminal cleanup and keep operational actions policy-gated.

use super::app::App;
use super::runtime::render::{cleanup_screenshot_terminal, parse_capture_tab, write_buffer_png};
use super::runtime::{
    input::{hydrate_smoke_state, run_loop},
    maintenance::cache_maintenance_loop,
};
use crate::state::TuiSession;
use crate::tui::flow::collect_once;
use anyhow::Result;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::path::Path;

pub async fn run_tui(
    store: TuiSession,
    docker_ctl: crate::docker::DockerCtl,
    client: crate::gitlab_client::GitlabClient,
    demo: bool,
) -> Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let maintenance_docker = docker_ctl.clone();
    tokio::spawn(async move {
        cache_maintenance_loop(maintenance_docker).await;
    });

    let mut app = App::new(store, docker_ctl, client);
    if demo {
        app.apply_demo_fixture();
    } else {
        // Wave 6.A: try to upgrade Mission Control's action surface to the
        // production adapter. Non-fatal: when the DB/token/key chain isn't
        // available we keep the default FakeActionAdapter so the cockpit
        // still renders and reports `Failed("no adapter wired")` per click.
        if let Err(e) = app.try_install_production_adapter().await {
            tracing::warn!(target: "tui.cockpit", err = %e, "action adapter stays fake");
        }
        hydrate_smoke_state(&mut app).await;
        app.start_background_sync();
    }

    let res = run_loop(&mut terminal, &mut app, demo).await;

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

pub async fn run_tui_once(
    store: TuiSession,
    docker_ctl: crate::docker::DockerCtl,
    client: crate::gitlab_client::GitlabClient,
) -> Result<()> {
    use ratatui::backend::TestBackend;

    let mut app = App::new(store, docker_ctl, client);
    hydrate_smoke_state(&mut app).await;
    seed_live_flow_snapshot(&mut app).await;

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|f| super::ui::draw(f, &mut app))?;
    println!(
        "jeryu TUI smoke render ok (live jobs: {})",
        app.state.recent_jobs.len()
    );
    Ok(())
}

pub async fn run_tui_screenshot(
    store: TuiSession,
    docker_ctl: crate::docker::DockerCtl,
    client: crate::gitlab_client::GitlabClient,
    tab: &str,
    hold_ms: u64,
) -> Result<()> {
    let mut app = App::new(store, docker_ctl, client);
    app.active_tab = parse_capture_tab(tab)?;
    app.apply_demo_fixture();

    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.draw(|f| super::ui::draw(f, &mut app))?;

    if let Ok(ready_file) = std::env::var("TUI_READY_FILE")
        && !ready_file.is_empty()
    {
        std::fs::write(std::path::Path::new(&ready_file), b"ready")?;
    }

    std::thread::sleep(std::time::Duration::from_millis(hold_ms));
    cleanup_screenshot_terminal(&mut terminal)
}

/// Render one deterministic TUI frame into a PNG file.
pub async fn capture_tui_png(
    store: TuiSession,
    docker_ctl: crate::docker::DockerCtl,
    client: crate::gitlab_client::GitlabClient,
    tab: &str,
    output: &Path,
    width: u16,
    height: u16,
) -> Result<()> {
    use ratatui::backend::TestBackend;

    let mut app = App::new(store, docker_ctl, client);
    app.active_tab = parse_capture_tab(tab)?;
    hydrate_smoke_state(&mut app).await;
    seed_live_flow_snapshot(&mut app).await;

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|f| super::ui::draw(f, &mut app))?;

    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    write_buffer_png(terminal.backend().buffer(), output)?;
    Ok(())
}

async fn seed_live_flow_snapshot(app: &mut App) {
    let flow_snap = collect_once(&app.store, &app.docker, &app.gitlab).await;
    let _ = app.flow_tx.send(flow_snap).await;
    app.tick().await;
}
