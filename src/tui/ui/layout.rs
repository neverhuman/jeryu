//! Owner: Interactive TUI subsystem — rendering layout helpers
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Layout helpers are pure geometry — no app-state mutation, no IO.
//! U14 (first-cut): extracted from src/tui/ui.rs (no behaviour changes).

use std::rc::Rc;

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Vertical chunks for the fullscreen-activity layout: header / log body / footer.
///
/// Returned slice has 3 entries: `[header (3 rows), body (>=10 rows), footer (1 row)]`.
pub(super) fn fullscreen_layout(area: Rect) -> Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header + tabs
            Constraint::Min(10),   // Full log view
            Constraint::Length(1), // Footer
        ])
        .split(area)
}

/// Vertical chunks for the standard layout: header / body / activity / footer.
///
/// Returned slice has 4 entries: `[header (3 rows), body (>=10 rows),
/// activity (7 rows), footer (1 row)]`.
pub(super) fn standard_layout(area: Rect) -> Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header + tabs
            Constraint::Min(10),   // Content
            Constraint::Length(7), // Activity / Logs
            Constraint::Length(1), // Footer
        ])
        .split(area)
}
