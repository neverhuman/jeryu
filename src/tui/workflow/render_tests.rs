//! Owner: Interactive TUI subsystem — Delivery view end-to-end render tests
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::render_tests`
//! Invariants: Pure rendering tests; no async, no I/O. Catches layout panics.

use ratatui::{Terminal, backend::TestBackend};

use super::delivery::build_demo_delivery;
use super::hit_map::DeliveryHitMap;
use super::inspector::{InspectorTab, draw_inspector_pane};
use super::nav::WorkflowNav;
use super::widget::draw_delivery_tab;
use crate::tui::app::LiveLogState;
use crate::tui::theme::Theme;

#[test]
fn delivery_view_renders_at_200x60_without_panic() {
    let snap = build_demo_delivery();
    let nav = WorkflowNav::default();
    let theme = Theme::dark();
    let mut hits = DeliveryHitMap::default();

    let backend = TestBackend::new(200, 60);
    let mut term = Terminal::new(backend).unwrap();

    term.draw(|f| {
        draw_delivery_tab(f, f.area(), &snap, &nav, &theme, 0, &mut hits);
    })
    .unwrap();

    // Mission strip + PR rail + phase rail + canvas + minimap should all
    // be visible at this terminal size.
    assert!(hits.mission.is_some(), "mission strip should be visible");
    assert!(hits.pr_rail.is_some(), "PR rail should be visible");
    assert!(
        hits.phase_rail.is_some(),
        "phase rail should be visible at 200 cols"
    );
    assert!(hits.canvas.is_some(), "canvas should be visible");
    assert!(
        hits.minimap.is_some(),
        "minimap should be visible at 200 cols"
    );

    // First-PR card hit boxes populated.
    assert!(
        !hits.cards.is_empty(),
        "at least one node card should have been laid out"
    );
}

#[test]
fn delivery_view_collapses_chrome_at_narrow_terminal() {
    let snap = build_demo_delivery();
    let nav = WorkflowNav::default();
    let theme = Theme::dark();
    let mut hits = DeliveryHitMap::default();

    let backend = TestBackend::new(90, 30);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        draw_delivery_tab(f, f.area(), &snap, &nav, &theme, 0, &mut hits);
    })
    .unwrap();

    assert!(hits.canvas.is_some());
    assert!(
        hits.phase_rail.is_none(),
        "phase rail collapses below 120 cols"
    );
    assert!(hits.minimap.is_none(), "minimap collapses below 160 cols");
}

#[test]
fn inspector_pane_renders_for_each_subtab() {
    let snap = build_demo_delivery();
    let theme = Theme::dark();
    let log = LiveLogState {
        target: None,
        text: "running cargo test\nwarning: slow test\nerror: bang".into(),
        ..Default::default()
    };

    let backend = TestBackend::new(60, 30);
    let mut term = Terminal::new(backend).unwrap();
    let selected_id = snap.pull_requests[0].snapshot.nodes[0].id.clone();

    for tab in InspectorTab::ALL {
        term.draw(|f| {
            draw_inspector_pane(
                f,
                f.area(),
                &snap,
                Some(&selected_id),
                tab,
                &log,
                Some("Test action message — last rollback scheduled"),
                &theme,
            );
        })
        .unwrap();
    }
}

#[test]
fn ticking_progress_doesnt_panic_with_pulsing_borders() {
    // Pulses are tick-driven (tick / 2) % 2; render across a tick window.
    let snap = build_demo_delivery();
    let nav = WorkflowNav::default();
    let theme = Theme::dark();
    let mut hits = DeliveryHitMap::default();

    let backend = TestBackend::new(180, 50);
    let mut term = Terminal::new(backend).unwrap();
    for tick in 0..8 {
        term.draw(|f| {
            draw_delivery_tab(f, f.area(), &snap, &nav, &theme, tick, &mut hits);
        })
        .unwrap();
    }
}
