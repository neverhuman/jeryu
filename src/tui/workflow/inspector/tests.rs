//! Owner: Interactive TUI subsystem — Delivery inspector unit tests
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::inspector`
//! Invariants: Tests are deterministic and isolated; no I/O.

use super::InspectorTab;

#[test]
fn tab_cycles_next_and_prev() {
    let mut t = InspectorTab::Overview;
    for _ in 0..InspectorTab::ALL.len() {
        t = t.next();
    }
    assert_eq!(t, InspectorTab::Overview);

    let mut t = InspectorTab::Logs;
    t = t.prev();
    assert_eq!(t, InspectorTab::Overview);
}
