//! Owner: Tuiwright test suite root manifest
//! Proof: `cargo nextest run --test tuiwright`
//! Invariants: every assertion preserved from the pre-split tests/tui_tuiwright.rs.
//!
//! This file is the Cargo test harness root; it declares the per-category
//! submodules and the shared helpers module. Each submodule under
//! `tests/tuiwright/` contains a slice of the original 23 tests, grouped by
//! UX surface (capture, tabs, bugs, workflow, fleet_bar, jankurai, overlays,
//! palette, discovery).

#[path = "tuiwright/helpers.rs"]
mod helpers;

#[path = "tuiwright/bugs.rs"]
mod bugs;
#[path = "tuiwright/capture.rs"]
mod capture;
#[path = "tuiwright/discovery.rs"]
mod discovery;
#[path = "tuiwright/fleet_bar.rs"]
mod fleet_bar;
#[path = "tuiwright/jankurai.rs"]
mod jankurai;
#[path = "tuiwright/overlays.rs"]
mod overlays;
#[path = "tuiwright/palette.rs"]
mod palette;
#[path = "tuiwright/tabs.rs"]
mod tabs;
#[path = "tuiwright/workflow.rs"]
mod workflow;
