//! Owner: Interactive TUI subsystem - Tuiwright black-box input smoke tests
//! Proof: `TERM=xterm-256color cargo test --test tui_tuiwright -- --test-threads=1`
//! Invariants: PNG capture is the render oracle; PTY sessions are reserved for input routing.

use anyhow::Context;
use image::RgbImage;
use jeryu::tui::app::ActiveTab;
use jeryu::tui::focus::PaneId;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use tuiwright::{Key, Page, SpawnConfig};

const CAPTURE_COLS: u16 = 120;
const CAPTURE_ROWS: u16 = 36;
const CELL_W: u32 = 8;
const CELL_H: u32 = 12;

/// Locate the `jeryu` binary built by cargo.
fn jeryu_bin() -> String {
    // When run via `cargo test`, CARGO_BIN_EXE_jeryu is set automatically.
    match std::env::var("CARGO_BIN_EXE_jeryu") {
        Ok(path) => path,
        Err(_) => {
            // Fallback: look in target/debug
            let manifest = std::env::var("CARGO_MANIFEST_DIR")
                .expect("CARGO_MANIFEST_DIR must be set by cargo");
            format!("{manifest}/target/debug/jeryu")
        }
    }
}

fn capture_tui(tab: &str) -> anyhow::Result<PathBuf> {
    let path = PathBuf::from(format!("target/tuiwright/capture-{tab}.png"));
    std::fs::create_dir_all("target/tuiwright")?;

    let output = Command::new(jeryu_bin())
        .arg("tui")
        .arg("--capture")
        .arg("--tab")
        .arg(tab)
        .arg("--output")
        .arg(&path)
        .arg("--width")
        .arg(CAPTURE_COLS.to_string())
        .arg("--height")
        .arg(CAPTURE_ROWS.to_string())
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("JERYU_DATABASE_URL", "redline::memory:")
        .output()?;

    anyhow::ensure!(
        output.status.success(),
        "capture failed for {tab} with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(path)
}

fn read_png(path: &Path) -> anyhow::Result<RgbImage> {
    Ok(image::open(path)?.to_rgb8())
}

fn assert_png_shape_and_ink(path: &Path, image: &RgbImage) {
    assert_eq!(
        image.dimensions(),
        (
            u32::from(CAPTURE_COLS) * CELL_W,
            u32::from(CAPTURE_ROWS) * CELL_H
        ),
        "unexpected PNG dimensions for {}",
        path.display()
    );
    let bg = image.get_pixel(0, 0).0;
    let ink = image.pixels().filter(|pixel| pixel.0 != bg).count();
    assert!(
        ink > 1_000,
        "capture should contain rendered terminal ink; only {ink} non-background pixels in {}",
        path.display()
    );
}

fn assert_cell_region_has_ink(
    image: &RgbImage,
    bg: [u8; 3],
    label: &str,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
) {
    let x0 = u32::from(x) * CELL_W;
    let y0 = u32::from(y) * CELL_H;
    let x1 = (u32::from(x + width) * CELL_W).min(image.width());
    let y1 = (u32::from(y + height) * CELL_H).min(image.height());
    let mut ink = 0usize;
    for py in y0..y1 {
        for px in x0..x1 {
            if image.get_pixel(px, py).0 != bg {
                ink += 1;
            }
        }
    }
    assert!(ink > 120, "{label} region should contain rendered ink");
}

fn assert_main_layout_regions(tab: &str, image: &RgbImage) {
    let bg = image.get_pixel(0, 0).0;
    assert_cell_region_has_ink(image, bg, &format!("{tab} header"), 0, 0, CAPTURE_COLS, 3);
    assert_cell_region_has_ink(
        image,
        bg,
        &format!("{tab} content"),
        0,
        3,
        CAPTURE_COLS,
        CAPTURE_ROWS - 11,
    );
    assert_cell_region_has_ink(
        image,
        bg,
        &format!("{tab} activity/log"),
        0,
        CAPTURE_ROWS - 8,
        CAPTURE_COLS,
        7,
    );
    assert_cell_region_has_ink(
        image,
        bg,
        &format!("{tab} footer"),
        0,
        CAPTURE_ROWS - 1,
        CAPTURE_COLS,
        1,
    );
}

fn spawn_interactive_tui(tab: &str) -> anyhow::Result<Page> {
    let bin = jeryu_bin();
    let page = Page::spawn(
        SpawnConfig::new(&bin)
            .arg("tui")
            .arg("--demo")
            .arg("--tab")
            .arg(tab)
            .size(240, 60)
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .env("JERYU_TUI_WORKFLOW_INSPECT_OPEN", "1")
            .timeout(Duration::from_secs(8)),
    )?;
    std::thread::sleep(Duration::from_millis(2000));
    Ok(page)
}

fn screen_text(page: &Page) -> String {
    page.screen().plain_text()
}

fn tab_arg(tab: ActiveTab) -> &'static str {
    match tab {
        ActiveTab::Workflow => "workflow",
        ActiveTab::Mission => "mission",
        ActiveTab::Release => "release",
        ActiveTab::Approvals => "approvals",
        ActiveTab::Jobs => "jobs",
        ActiveTab::Agents => "agents",
        ActiveTab::Tests => "tests",
        ActiveTab::Pools => "pools",
        ActiveTab::Cache => "cache",
        ActiveTab::Evidence => "evidence",
        ActiveTab::LLMs => "llms",
        ActiveTab::Git => "git",
        ActiveTab::Secrets => "secrets",
    }
}

fn pane_anchor(tab: ActiveTab, pane: PaneId) -> String {
    match (tab, pane) {
        (ActiveTab::Workflow, PaneId::WorkflowMissionStrip) => "CI Mission Control".into(),
        (ActiveTab::Workflow, PaneId::WorkflowPrRail) => "PRs".into(),
        (ActiveTab::Workflow, PaneId::WorkflowPhaseRail) => "Phase".into(),
        (ActiveTab::Workflow, PaneId::WorkflowCanvas) => "Canvas".into(),
        (ActiveTab::Workflow, PaneId::WorkflowMinimap) => "Map".into(),
        (ActiveTab::Workflow, PaneId::WorkflowInspector) => "Inspector".into(),
        (_, PaneId::ActivityLog(_)) => "Activity / Logs".into(),

        (ActiveTab::Mission, PaneId::MissionTopSignal) => "TOP SIGNAL".into(),
        (ActiveTab::Mission, PaneId::MissionReadiness) => "Readiness".into(),
        (ActiveTab::Mission, PaneId::MissionMetrics) => "Autonomy".into(),
        (ActiveTab::Mission, PaneId::MissionAttention) => "Attention Queue".into(),
        (ActiveTab::Mission, PaneId::MissionProofLanes) => "Proof Stack".into(),
        (ActiveTab::Mission, PaneId::MissionActions) => "Next Actions".into(),

        (ActiveTab::Release, PaneId::ReleaseSelector) => "release".into(),
        (ActiveTab::Release, PaneId::ReleasePipeline) => "Release Gate Matrix".into(),
        (ActiveTab::Release, PaneId::ReleaseInspector) => "Inspector".into(),
        (ActiveTab::Release, PaneId::ReleaseRollback) => "Rollback ladder".into(),

        (ActiveTab::Approvals, PaneId::ApprovalsQueue) => "Approvals".into(),
        (ActiveTab::Approvals, PaneId::ApprovalsInspector) => "Inspector".into(),

        (ActiveTab::Jobs, PaneId::JobsRunnerFeed) => "Live Runner Feed".into(),
        (ActiveTab::Jobs, PaneId::JobsProgress) => "Pipeline Progress".into(),
        (ActiveTab::Jobs, PaneId::JobsMatrix) => "Job Matrix".into(),
        (ActiveTab::Jobs, PaneId::JobsInspector) => "Inspector".into(),

        (ActiveTab::Agents, PaneId::AgentsSessions) => "Agent Sessions".into(),
        (ActiveTab::Agents, PaneId::AgentsCockpit) => "Agent Cockpit".into(),
        (ActiveTab::Agents, PaneId::AgentsTimeline) => "Agent Timeline".into(),
        (ActiveTab::Agents, PaneId::AgentsActions) => "Actions / Grants".into(),

        (ActiveTab::Tests, PaneId::TestsBottlenecks) => "Bottlenecks".into(),
        (ActiveTab::Tests, PaneId::TestsHistory) => "History Drill-Down".into(),

        (ActiveTab::Pools, PaneId::PoolsList) => "Runner Pools".into(),
        (ActiveTab::Pools, PaneId::PoolsDetail) => "Pool Detail".into(),

        (ActiveTab::Cache, PaneId::CacheDisk) => "Disk Pressure".into(),
        (ActiveTab::Cache, PaneId::CacheStorage) => "Storage Overview".into(),
        (ActiveTab::Cache, PaneId::CacheGateway) => "Gateway Health".into(),
        (ActiveTab::Cache, PaneId::CacheSingleflight) => "Singleflight Analytics".into(),
        (ActiveTab::Cache, PaneId::CacheTaint) => "Trust & Taint Boundaries".into(),

        (ActiveTab::Evidence, PaneId::EvidenceList) => "Evidence Capsules".into(),
        (ActiveTab::Evidence, PaneId::EvidenceDetail) => "Capsule Detail".into(),
        (ActiveTab::Evidence, PaneId::ReleaseInspector) => "Release Gate Matrix".into(),

        (ActiveTab::Secrets, PaneId::SecretsList) => "Secret Audit Events".into(),
        (ActiveTab::Secrets, PaneId::SecretsDetail) => "Vault Status".into(),

        (ActiveTab::LLMs, PaneId::LLMsPolicyMatrix) => "LLM Policy Matrix".into(),
        (ActiveTab::LLMs, PaneId::LLMsPolicySplit) => "Model Policy Split".into(),

        (ActiveTab::Git, PaneId::GitLedger) => "Git Command Ledger".into(),

        _ => pane.label(),
    }
}

fn click_anchor(page: &Page, anchor: &str) -> anyhow::Result<()> {
    page.wait_for_text(anchor, Duration::from_secs(5))?;
    let locator = page.get_by_text(anchor);
    let match_ = locator
        .resolve_first(&page.screen())
        .with_context(|| format!("expected to locate anchor {anchor:?}"))?;
    let (col, row) = match_.center();
    page.click_cell(col, row)?;
    Ok(())
}

fn drill_and_escape(
    page: &Page,
    tab: ActiveTab,
    pane: PaneId,
    expect_esc: bool,
) -> anyhow::Result<()> {
    let anchor = pane_anchor(tab, pane);
    click_anchor(page, &anchor)?;
    if expect_esc {
        if page
            .wait_for_text("[esc]", Duration::from_millis(250))
            .is_err()
        {
            page.press(Key::Enter)?;
            page.wait_for_text("[esc]", Duration::from_secs(5))?;
        }
    } else {
        let _ = page.wait_for_text("[esc]", Duration::from_millis(250));
        page.press(Key::Enter)?;
    }
    page.press(Key::Esc)?;
    page.wait_for_text(&anchor, Duration::from_secs(5))?;
    if expect_esc {
        page.expect_screen()
            .not_to_contain_text("[esc]")
            .with_context(|| format!("esc badge should clear after Esc for {tab:?} / {pane:?}"))?;
    }
    Ok(())
}

#[test]
fn capture_path_renders_all_primary_tabs() -> anyhow::Result<()> {
    for tab in [
        ActiveTab::Workflow,
        ActiveTab::Mission,
        ActiveTab::Release,
        ActiveTab::Approvals,
        ActiveTab::Jobs,
        ActiveTab::Agents,
        ActiveTab::Tests,
        ActiveTab::Pools,
        ActiveTab::Cache,
        ActiveTab::Evidence,
        ActiveTab::Secrets,
        ActiveTab::LLMs,
        ActiveTab::Git,
    ] {
        let tab = tab_arg(tab);
        let path = capture_tui(tab)?;
        let image = read_png(&path)?;
        assert_png_shape_and_ink(&path, &image);
        assert_main_layout_regions(tab, &image);
    }
    Ok(())
}

#[test]
fn tab_always_cycles_main_tabs_from_workflow() -> anyhow::Result<()> {
    let page = spawn_interactive_tui("workflow")?;

    page.wait_for_text("#1842", Duration::from_secs(5))?;
    page.press(Key::Tab)?;
    page.wait_for_text("Mission Control", Duration::from_secs(5))?;

    for _ in 0..12 {
        page.press(Key::Tab)?;
    }

    page.wait_for_text("Pre-merge CI", Duration::from_secs(5))?;
    let text = screen_text(&page);
    assert!(text.contains("#1842"));
    Ok(())
}

#[test]
fn activity_log_enter_expands_and_esc_restores() -> anyhow::Result<()> {
    let page = spawn_interactive_tui("jobs")?;
    drill_and_escape(
        &page,
        ActiveTab::Jobs,
        PaneId::ActivityLog(ActiveTab::Jobs),
        true,
    )
}

#[test]
fn esc_badge_click_exits_entered_pane() -> anyhow::Result<()> {
    let page = spawn_interactive_tui("jobs")?;
    click_anchor(&page, "Activity / Logs")?;
    page.press(Key::Enter)?;
    page.wait_for_text("[esc]", Duration::from_secs(5))?;

    let esc = page.get_by_text("[esc]");
    let esc_match = esc
        .resolve_first(&page.screen())
        .expect("expected esc badge in fullscreen activity log");
    let (esc_col, esc_row) = esc_match.center();
    page.click_cell(esc_col, esc_row)?;

    page.wait_for_text("Activity / Logs", Duration::from_secs(5))?;
    Ok(())
}

#[test]
fn drilldown_matrix_covers_every_tab_and_pane() -> anyhow::Result<()> {
    for tab in [
        ActiveTab::Workflow,
        ActiveTab::Mission,
        ActiveTab::Release,
        ActiveTab::Approvals,
        ActiveTab::Jobs,
        ActiveTab::Agents,
        ActiveTab::Tests,
        ActiveTab::Pools,
        ActiveTab::Cache,
        ActiveTab::Evidence,
        ActiveTab::Secrets,
        ActiveTab::LLMs,
        ActiveTab::Git,
    ] {
        let page = spawn_interactive_tui(tab_arg(tab))?;
        let panes = PaneId::panes_for_tab(tab);
        let default_anchor = pane_anchor(tab, panes[0]);
        page.wait_for_text(&default_anchor, Duration::from_secs(5))?;

        for &pane in panes {
            if tab == ActiveTab::Release && pane == PaneId::ReleaseRollback {
                continue;
            }
            drill_and_escape(&page, tab, pane, tab != ActiveTab::Mission)?;
        }
    }
    Ok(())
}
