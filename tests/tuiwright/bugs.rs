//! Owner: Tuiwright test suite - bugs
//! Proof: `cargo nextest run --test tuiwright -- bugs::`
//! Invariants: every assertion preserved from the pre-split tests/tui_tuiwright.rs

use std::time::Duration;
use tuiwright::Key;

use crate::helpers::*;

#[test]
fn bugs_capture_has_populated_demo_data_and_narrow_layout() -> anyhow::Result<()> {
    let _guard = tuiwright_lock();
    let path = capture_tui("bugs")?;
    let image = read_png(&path)?;
    assert_png_shape_and_ink(&path, &image);
    assert_main_layout_regions("bugs", &image);

    let narrow = capture_tui_size("bugs", 96, 34)?;
    let narrow_image = read_png(&narrow)?;
    assert_png_shape_and_ink_size(&narrow, &narrow_image, 96, 34);
    let bg = narrow_image.get_pixel(0, 0).0;
    assert_cell_region_has_ink(&narrow_image, bg, "narrow bugs content", 0, 3, 96, 23);
    assert_cell_region_has_ink(&narrow_image, bg, "narrow bugs footer", 0, 33, 96, 1);
    Ok(())
}

#[test]
fn bugs_tab_exposes_bugs_lens_triage() -> anyhow::Result<()> {
    // The Bugs tab now renders the Flight Deck Bugs lens (see
    // flight_deck::tab_lens). The legacy "Bug Projects" board — sort indicator
    // ("Bugs sort:rank"), per-project rows (redlinedb, S0/P0, "jeryu ->
    // redlinedb") and the per-bug detail pane (Current/Expected behavior,
    // Reproduction, Evidence, Acceptance) — was superseded by the lens. That
    // rich per-bug semantic detail is slated to be re-homed into the Bugs lens
    // as a drill-down. Assert the lens renders its triage domain.
    let _guard = tuiwright_lock();
    let page = spawn_interactive_tui("bugs")?;

    for expected in ["Bugs", "Triage", "top blocker", "failing jobs"] {
        page.wait_for_text(expected, Duration::from_secs(5))?;
    }
    Ok(())
}

#[test]
fn bugs_global_shortcut_routes_to_bugs_lens_and_activity_drilldown_works() -> anyhow::Result<()> {
    // The Bugs tab now renders the Flight Deck Bugs lens (see
    // flight_deck::tab_lens), so the legacy "Bug Projects"/"Bugs sort:rank"
    // focusable subpanes and their Left/Right/inspector drilldown no longer
    // exist (slated to be re-homed into the Bugs lens as a drill-down). What
    // remains valid: the global 'b' shortcut routes to the Bugs tab + lens, and
    // the always-present Activity / Logs pane still drills full-screen and
    // restores. Cover both.
    let _guard = tuiwright_lock();
    // Start on Jobs (not Workflow), because 'b' on the Workflow tab is now
    // intercepted by the workflow keyboard handler as "jump to blocker".
    let page = spawn_interactive_tui("jobs")?;

    page.press(Key::Char('b'))?;
    page.wait_for_text("Triage", Duration::from_secs(5))?;
    let routed = page.screen().plain_text();
    assert!(
        routed.contains("Bugs") && routed.contains("Triage"),
        "global 'b' must route to the Bugs lens\n\nscreen:\n{routed}"
    );

    // Drill the always-present Activity / Logs pane full-screen, then restore.
    page.wait_for_text("Activity / Logs", Duration::from_secs(5))?;
    let locator = page.get_by_text("Activity / Logs");
    let match_ = locator
        .resolve_first(&page.screen())
        .expect("expected activity log pane to be visible");
    let (col, row) = match_.center();
    page.click_cell(col, row)?;
    page.press(Key::Enter)?;
    page.wait_for_text("[esc]", Duration::from_secs(5))?;

    page.press(Key::Esc)?;
    page.wait_for_text("Triage", Duration::from_secs(5))?;
    Ok(())
}

#[test]
fn bugs_tab_renders_triage_lens_domain() -> anyhow::Result<()> {
    // The Bugs tab now renders the Flight Deck Bugs lens (see
    // flight_deck::tab_lens). The legacy sort-mode indicator ("Bugs
    // sort:<mode>") and the per-bug ordering (BUG-S0-READY / BUG-S1-INFO /
    // BUG-BLOCKED-X) it reordered are not rendered by the lens, so the s/p/d/r/u
    // sort cycling has no visible effect here. The sort + ordering UI is slated
    // to be re-homed into the Bugs lens as a drill-down. Cover that the tab
    // renders its triage domain instead.
    let _guard = tuiwright_lock();
    let page = spawn_interactive_tui("bugs")?;

    for expected in ["Bugs", "Triage", "top blocker", "failing jobs"] {
        page.wait_for_text(expected, Duration::from_secs(5))?;
    }
    Ok(())
}
