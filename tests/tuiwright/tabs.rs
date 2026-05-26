//! Owner: Tuiwright test suite - tabs
//! Proof: `cargo nextest run --test tuiwright -- tabs::`
//! Invariants: every assertion preserved from the pre-split tests/tui_tuiwright.rs

use std::time::Duration;
use tuiwright::Key;

use crate::helpers::*;

#[test]
fn tab_always_cycles_main_tabs_from_workflow() -> anyhow::Result<()> {
    let _guard = tuiwright_lock();
    let page = spawn_interactive_tui("workflow")?;

    page.wait_for_text("#1842", Duration::from_secs(5))?;
    page.press(Key::Tab)?;
    page.wait_for_text("Mission Control", Duration::from_secs(5))?;

    // Cycle through all remaining 15 tabs (Mission→Release→…→Git→Jankurai→Workflow)
    for _ in 0..15 {
        page.press(Key::Tab)?;
    }

    page.wait_for_text("Pre-merge CI", Duration::from_secs(5))?;
    let text = screen_text(&page);
    assert!(
        text.contains("#1842"),
        "should be back on Workflow tab showing PR #1842"
    );
    Ok(())
}
