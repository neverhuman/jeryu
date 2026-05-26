//! Mission lens selectors, rendering, and local navigation.
//!
//! Rendering stays pure over `MissionLensInput`; no backend I/O is allowed here.

pub mod data;
pub mod nav;
pub mod view;

pub use data::{MissionLensInput, select_mission_lens_input};
pub use nav::{MissionNavOutcome, MissionPane, activate_pane, move_focus};
pub use view::MissionLens;

#[cfg(test)]
mod tests;
