//! Owner: Interactive TUI subsystem — rendering footer strip
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Footer rendering is read-only over App; no control-plane side effects.
//! U14 (first-cut): extracted from src/tui/ui.rs (no behaviour changes).
//!
//! Thin pass-through over `ui_chrome::draw_footer`. The real footer widget
//! (key hints + frame metrics) still lives in `src/tui/ui_chrome.rs` and
//! `src/tui/ui_chrome_footer.rs` and is explicitly out of scope for U14
//! first-cut.

use ratatui::{Frame, layout::Rect};

use crate::tui::app::App;

use super::ui_chrome;

pub(super) fn draw(f: &mut Frame, app: &App, area: Rect) {
    ui_chrome::draw_footer(f, app, area);
}
