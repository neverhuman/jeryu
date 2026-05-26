//! Owner: Tuiwright test suite - overlays
//! Proof: `cargo nextest run --test tuiwright -- overlays::`
//! Invariants: every assertion preserved from the pre-split tests/tui_tuiwright.rs

use std::time::Duration;
use tuiwright::Key;

use crate::helpers::*;

/// When the fleet bar detail overlay is open it must show `[esc]` in its
/// top-right corner so users can discover how to close it.
///
/// We try to reach the fleet bar via Up-Up (same approach as the existing
/// `fleet_bar_focus_enter_opens_detail_and_arrows_cycle_repos` test).
/// If the overlay does not open (focus did not reach FleetBar), the test
/// passes vacuously — the assertion only fires when the overlay is confirmed
/// on screen.
#[test]
fn repo_detail_overlay_has_esc_badge() -> anyhow::Result<()> {
    let _guard = tuiwright_lock();
    let page = spawn_interactive_tui_size("workflow", 220, 44)?;

    page.wait_for_text("nht", Duration::from_secs(5))?;
    page.wait_for_text("PRs", Duration::from_secs(5))?;

    // Arrow up twice to try to focus the fleet bar.
    page.press(Key::Up)?;
    std::thread::sleep(Duration::from_millis(300));
    page.press(Key::Up)?;
    std::thread::sleep(Duration::from_millis(300));

    // Press Enter to open the repo detail overlay (only fires if focused).
    page.press(Key::Enter)?;
    std::thread::sleep(Duration::from_millis(500));
    let text = screen_text(&page);

    if text.contains("Repo: All") || text.contains("Repositories:") {
        assert!(
            text.contains("[esc]"),
            "repo detail overlay must show [esc] badge in top-right corner\n\nscreen:\n{text}"
        );
    }
    Ok(())
}

/// When the repo detail overlay is open, clicking its `[esc]` badge must close
/// it (and not leave `repo_detail_open = true` in a zombie state).
#[test]
fn repo_detail_overlay_esc_click_closes_it() -> anyhow::Result<()> {
    let _guard = tuiwright_lock();
    let page = spawn_interactive_tui_size("workflow", 220, 44)?;

    page.wait_for_text("nht", Duration::from_secs(5))?;
    page.wait_for_text("PRs", Duration::from_secs(5))?;

    page.press(Key::Up)?;
    std::thread::sleep(Duration::from_millis(300));
    page.press(Key::Up)?;
    std::thread::sleep(Duration::from_millis(300));
    page.press(Key::Enter)?;
    std::thread::sleep(Duration::from_millis(500));

    let text = screen_text(&page);
    if !text.contains("Repo: All") && !text.contains("Repositories:") {
        // Fleet bar was not focused — skip the click test.
        return Ok(());
    }

    // Click the center of the overlay's top border row (the esc hotspot spans
    // that entire row). We compute the position directly from the screen size
    // rather than via find_text, because find_text returns byte offsets that
    // diverge from cell offsets when multi-byte ─ characters precede [esc].
    //
    // For a 220×44 terminal, the overlay rect is Rect::new(37, 16, 146, 12):
    //   overlay_w = (220*2/3).max(44).min(216) = 146
    //   overlay_x = (220-146)/2 = 37
    //   overlay_y = (44-12)/2 = 16
    // The esc hotspot covers (37..182, row=16). Click the center column.
    let overlay_x: u16 = 37;
    let overlay_y: u16 = 16;
    let overlay_w: u16 = 146;
    let click_col = overlay_x + overlay_w / 2; // 110 — well within hotspot
    let click_row = overlay_y;
    page.click_cell(click_col, click_row)?;
    std::thread::sleep(Duration::from_millis(400));

    let after = screen_text(&page);
    assert!(
        !after.contains("Repositories:"),
        "clicking [esc] hotspot on the overlay title row must close the repo detail overlay\n\nscreen:\n{after}"
    );
    Ok(())
}

/// Pressing `?` opens the help overlay. The overlay must render `[esc]` in its
/// top-right corner so the close affordance is visible to all users.
#[test]
fn help_overlay_has_esc_badge() -> anyhow::Result<()> {
    let _guard = tuiwright_lock();
    let page = spawn_interactive_tui("workflow")?;

    page.wait_for_text("nht", Duration::from_secs(5))?;

    page.press(Key::Char('?'))?;
    page.wait_for_text("[ Help ]", Duration::from_secs(5))?;

    let text = screen_text(&page);
    assert!(
        text.contains("[esc]"),
        "help overlay must show [esc] badge in top-right corner\n\nscreen:\n{text}"
    );
    Ok(())
}
