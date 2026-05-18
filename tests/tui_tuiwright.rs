//! Owner: Interactive TUI subsystem — Tuiwright black-box integration tests
//! Proof: `TERM=xterm-256color cargo test --test tui_tuiwright -- --test-threads=1`
//! Invariants: Each test spawns a real PTY session; tests are serial to avoid port contention.

use std::time::Duration;
use tuiwright::{Page, SpawnConfig};

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

fn spawn_tui(tab: &str) -> anyhow::Result<Page> {
    let bin = jeryu_bin();
    let page = Page::spawn(
        SpawnConfig::new(&bin)
            .arg("tui")
            .arg("--screenshot")
            .arg("--tab")
            .arg(tab)
            .arg("--screenshot-hold-ms")
            .arg("10000")
            .size(120, 36)
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .timeout(Duration::from_secs(8)),
    )?;
    // Wait for the TUI to finish its first render.
    std::thread::sleep(Duration::from_millis(800));
    Ok(page)
}

// ── Test: Workflow tab renders on startup ────────────────────────────────

#[test]
fn workflow_tab_renders_header_and_content() -> anyhow::Result<()> {
    let page = spawn_tui("workflow")?;

    // The header bar must show the Delivery (Workflow) tab label.
    page.wait_for_text("Delivery", Duration::from_secs(5))?;

    // The 5-PR demo renders the canonical pre-merge phase header.
    page.wait_for_text("Pre-merge CI", Duration::from_secs(3))?;

    std::fs::create_dir_all("target/tuiwright")?;
    page.screenshot("target/tuiwright/workflow-default.png")?;

    Ok(())
}

// ── Test: Workflow tab shows demo PR + status glyphs ────────────────────

#[test]
fn workflow_demo_shows_node_labels() -> anyhow::Result<()> {
    let page = spawn_tui("workflow")?;

    // The 5-PR demo includes a PR #1842 chip in the PR rail.
    page.wait_for_text("#1842", Duration::from_secs(5))?;

    // The mission strip + cards should contain status glyphs.
    let screen = page.screen();
    let text = screen.plain_text();

    assert!(
        text.contains('✓') || text.contains('●') || text.contains("RAN") || text.contains("OPEN"),
        "expected a status glyph or label in delivery view; got:\n{}",
        text
    );

    page.screenshot("target/tuiwright/workflow-nodes.png")?;
    Ok(())
}

// ── Test: Mission tab renders ───────────────────────────────────────────

#[test]
fn mission_tab_renders() -> anyhow::Result<()> {
    let page = spawn_tui("mission")?;

    page.wait_for_text("Mission", Duration::from_secs(5))?;

    page.screenshot("target/tuiwright/mission.png")?;
    Ok(())
}

// ── Test: Jobs tab renders ──────────────────────────────────────────────

#[test]
fn jobs_tab_renders() -> anyhow::Result<()> {
    let page = spawn_tui("jobs")?;
    // The Jobs tab shows a Pipeline Progress panel in its content area.
    page.wait_for_text("Pipeline", Duration::from_secs(5))?;

    page.screenshot("target/tuiwright/jobs.png")?;
    Ok(())
}

// ── Test: Screenshot is deterministic PNG ───────────────────────────────

#[test]
fn screenshot_produces_valid_png() -> anyhow::Result<()> {
    let page = spawn_tui("workflow")?;
    page.wait_for_text("Workflow", Duration::from_secs(5))?;

    let path = "target/tuiwright/workflow-deterministic.png";
    std::fs::create_dir_all("target/tuiwright")?;
    page.screenshot(path)?;

    // Verify the file exists and has valid PNG header.
    let data = std::fs::read(path)?;
    assert!(data.len() > 100, "PNG file too small");
    assert_eq!(&data[1..4], b"PNG", "not a valid PNG file");

    Ok(())
}
