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
#[path = "tuiwright/drilldown.rs"]
mod drilldown;
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

// U29 — exhaustive lens action coverage. One module per lens covering
// every KeyCode in <lens>::nav::handle_key plus 80x24 / 120x36 render checks.
#[path = "tuiwright/lenses_agents.rs"]
mod lenses_agents;
#[path = "tuiwright/lenses_autonomy.rs"]
mod lenses_autonomy;
#[path = "tuiwright/lenses_bugs.rs"]
mod lenses_bugs;
#[path = "tuiwright/lenses_cache.rs"]
mod lenses_cache;
#[path = "tuiwright/lenses_evidence.rs"]
mod lenses_evidence;
#[path = "tuiwright/lenses_llms.rs"]
mod lenses_llms;
#[path = "tuiwright/lenses_mission.rs"]
mod lenses_mission;
#[path = "tuiwright/lenses_queue.rs"]
mod lenses_queue;
#[path = "tuiwright/lenses_release.rs"]
mod lenses_release;
#[path = "tuiwright/lenses_repos.rs"]
mod lenses_repos;
#[path = "tuiwright/lenses_runners.rs"]
mod lenses_runners;
#[path = "tuiwright/lenses_source_doctor.rs"]
mod lenses_source_doctor;
#[path = "tuiwright/lenses_vti.rs"]
mod lenses_vti;
#[path = "tuiwright/lenses_workflow.rs"]
mod lenses_workflow;
