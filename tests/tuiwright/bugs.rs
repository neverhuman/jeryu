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
fn bugs_tab_exposes_semantic_bug_details() -> anyhow::Result<()> {
    let _guard = tuiwright_lock();
    let page = spawn_interactive_tui("bugs")?;

    for expected in [
        "Bug Projects",
        "Bugs sort:rank",
        "redlinedb",
        "S0/P0",
        "ready",
        "1/0",
        "jeryu -> redlinedb",
        "Current behavior",
        "Expected behavior",
        "Reproduction",
        "Evidence",
        "Acceptance",
    ] {
        page.wait_for_text(expected, Duration::from_secs(5))?;
    }
    Ok(())
}

#[test]
fn bugs_global_shortcut_focus_navigation_and_inspector_drilldown_work() -> anyhow::Result<()> {
    let _guard = tuiwright_lock();
    // Start on Jobs (not Workflow), because 'b' on the Workflow tab is now
    // intercepted by the workflow keyboard handler as "jump to blocker".
    let page = spawn_interactive_tui("jobs")?;

    page.press(Key::Char('b'))?;
    page.wait_for_text("Bugs sort:rank", Duration::from_secs(5))?;
    wait_for_focused_title(&page, "Bugs sort:rank")?;

    page.press(Key::Left)?;
    wait_for_focused_title(&page, "Bug Projects")?;
    page.press(Key::Right)?;
    wait_for_focused_title(&page, "Bugs sort:rank")?;
    page.press(Key::Down)?;
    wait_for_focused_title(&page, "Activity / Logs")?;
    page.press(Key::Up)?;
    wait_for_focused_title(&page, "Bugs sort:rank")?;
    page.press(Key::Enter)?;
    page.wait_for_text("[esc]", Duration::from_secs(5))?;
    let drilled = page.screen().plain_text();
    assert!(drilled.contains("Bugs sort:rank"));

    page.press(Key::Esc)?;
    page.wait_for_text("Bug Projects", Duration::from_secs(5))?;
    Ok(())
}

#[test]
fn bugs_sort_keys_change_indicator_and_visible_order() -> anyhow::Result<()> {
    let _guard = tuiwright_lock();
    let page = spawn_interactive_tui("bugs")?;
    page.wait_for_text("Bugs sort:rank", Duration::from_secs(5))?;

    page.press(Key::Char('s'))?;
    page.wait_for_text("Bugs sort:severity", Duration::from_secs(5))?;
    assert_text_order(&screen_text(&page), "BUG-S0-READY", "BUG-S1-INFO");

    page.press(Key::Char('p'))?;
    page.wait_for_text("Bugs sort:priority", Duration::from_secs(5))?;
    assert_text_order(&screen_text(&page), "BUG-S0-READY", "BUG-S1-INFO");

    page.press(Key::Char('d'))?;
    page.wait_for_text("Bugs sort:difficulty", Duration::from_secs(5))?;
    assert_text_order(&screen_text(&page), "BUG-S0-READY", "BUG-S1-INFO");

    page.press(Key::Char('r'))?;
    page.wait_for_text("Bugs sort:ready", Duration::from_secs(5))?;
    assert_text_order(&screen_text(&page), "BUG-S0-READY", "BUG-BLOCKED-X");

    page.press(Key::Char('u'))?;
    page.wait_for_text("Bugs sort:updated", Duration::from_secs(5))?;
    assert_text_order(&screen_text(&page), "BUG-BLOCKED-X", "BUG-S0-READY");
    Ok(())
}
