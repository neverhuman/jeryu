//! Owner: Tuiwright test suite - palette
//! Proof: `cargo nextest run --test tuiwright -- palette::`
//! Invariants: every assertion preserved from the pre-split tests/tui_tuiwright.rs

use std::time::Duration;
use tuiwright::Key;

use crate::helpers::*;

/// Pressing Ctrl-K opens the command palette. The palette must render `[esc]`
/// in its top-right corner.
#[test]
fn command_palette_has_esc_badge() -> anyhow::Result<()> {
    let _guard = tuiwright_lock();
    let page = spawn_interactive_tui("workflow")?;

    page.wait_for_text("nht", Duration::from_secs(5))?;

    page.press(Key::Ctrl('k'))?;
    page.wait_for_text("Command Palette", Duration::from_secs(5))?;

    let text = screen_text(&page);
    assert!(
        text.contains("[esc]"),
        "command palette must show [esc] badge in top-right corner\n\nscreen:\n{text}"
    );
    Ok(())
}
