//! Owner: Interactive TUI subsystem - Bugs lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::bugs::`
//! Invariants: User can trace bug -> attempt -> branch/MR -> pipeline ->
//!             evidence. Ready/blocked/racing/fixed bugs each get a lane.
//!
//! Wave 5 U26 wires this scaffold to `BugsDashboard` from
//! `src/api/dashboards/bugs.rs` and adds board/lanes/attempts/ready
//! subcomponents.

pub mod data;
pub mod nav;
pub mod view;

pub use data::BugsLensInput;
pub use nav::{BugsIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::Bugs;
