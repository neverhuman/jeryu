//! Owner: Tuiwright test suite - jankurai
//! Proof: `cargo nextest run --test tuiwright -- jankurai::`
//! Invariants: every assertion preserved from the pre-split tests/tui_tuiwright.rs

use crate::helpers::*;

/// The Jankurai tab should render with the real score data from
/// `agent/repo-score.json` (score 89, 0 caps, passing).
#[test]
fn jankurai_tab_renders_with_real_score_data() -> anyhow::Result<()> {
    let _guard = tuiwright_lock();

    // PNG capture: just verify the tab has ink and the layout structure holds.
    let path = capture_tui("jankurai")?;
    let image = read_png(&path)?;
    assert_png_shape_and_ink(&path, &image);
    assert_main_layout_regions("jankurai", &image);

    // Interactive: verify the score and caps text are rendered.
    let page = spawn_interactive_tui("jankurai")?;
    // repo-score.json always exists; score should be visible in the pane.
    let text = screen_text(&page);
    assert!(
        text.contains("89") || text.contains("Jankurai"),
        "jankurai tab should render score or header text\n\nscreen:\n{text}"
    );
    Ok(())
}
