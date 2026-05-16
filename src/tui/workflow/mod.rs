//! Owner: Interactive TUI subsystem — workflow DAG module
//! Proof: `cargo nextest run -p jeryu -- tui::workflow`
//! Invariants: Workflow subsystem is a self-contained plan-driven test execution DAG.

pub mod builder;
pub mod collector;
pub mod delivery;
pub mod intelligence;
pub mod minimap;
pub mod mission_strip;
pub mod model;
pub mod nav;
pub mod phase_rail;
pub mod pr_rail;
pub mod regions;
pub mod widget;
