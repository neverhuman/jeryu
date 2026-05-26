//! Owner: Interactive TUI subsystem - runtime routing
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: TUI runtime stays split into focused input, maintenance, and render helpers.

pub mod data;
pub mod input;
pub mod maintenance;
pub mod render;
pub mod stream;
