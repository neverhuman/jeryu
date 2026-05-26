//! Owner: Interactive TUI subsystem — rendering header strip
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Header rendering is read-only over App; no control-plane side effects.
//! U14 (first-cut): extracted from src/tui/ui.rs (no behaviour changes).
//!
//! Thin pass-through over `ui_chrome::draw_header_tabs`. The real header
//! widget (tabs + freshness + breadcrumb) still lives in `src/tui/ui_chrome.rs`
//! and `src/tui/ui_chrome_*.rs` and is explicitly out of scope for U14
//! first-cut.

use ratatui::{Frame, layout::Rect};

use crate::tui::app::App;

use super::ui_chrome;

pub(super) fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    ui_chrome::draw_header_tabs(f, app, area);
}
