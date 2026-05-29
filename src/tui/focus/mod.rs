//! Owner: Interactive TUI subsystem — focus module root (U11 first-cut).
//! Proof: `cargo nextest run -p jeryu --lib tui::focus::`.
//! Invariants: the focus module groups pane identity (`pane`), state stack
//! (`state`), hit-test map (`map`), and chrome/border helpers (`chrome`).
//! Every previously-public symbol from the legacy `src/tui/focus.rs` is
//! re-exported here so `crate::tui::focus::*` callers keep compiling
//! unchanged. Macro/micro focus-graph redesign is deferred to U11-followup.

pub mod chrome;
pub mod map;
pub mod pane;
pub mod state;

// Pane identifier surface — re-export the enum + nav direction so every
// `crate::tui::focus::PaneId` / `crate::tui::focus::NavDirection` call site
// resolves without change.
pub use pane::{NavDirection, PaneId};

// State stack surface — `FocusState` is held by `App` and mutated by input
// handlers; preserve the existing path.
pub use state::FocusState;

// Hit-test map surface — `FocusMap` is held by `App` and queried by mouse
// routing; `FocusPane` is its row type and is referenced by tests.
pub use map::{FocusMap, FocusPane};

// Pane chrome surface — `PaneChrome` is consumed by every widget that draws
// a bordered pane; the free functions (`border_color`, `border_style`,
// `is_active`, `should_show_esc`, `should_show_drill_esc`, `pane_chrome`,
// `register_pane`, `register_esc_hotspot`, `register_drill_esc_hotspot`,
// `esc_label`, `title_with_esc`) are called via `focus::*` paths from
// `ui_panels*.rs` and `repo_fleet_bar.rs`.
pub use chrome::{
    PaneChrome, border_color, border_style, esc_label, is_active, pane_block, pane_chrome,
    register_drill_esc_hotspot, register_esc_hotspot, register_pane, should_show_drill_esc,
    should_show_esc, title_with_esc,
};
