//! Owner: Interactive TUI subsystem - app state, reducers, and selectors
//! Proof: `cargo test -p jeryu --lib tui::app`
//! Invariants: deterministic app transitions stay separate from rendering and I/O.

mod compat;
pub mod reducer;
pub mod selectors;
pub mod state;

pub use compat::*;

#[cfg(test)]
pub(crate) use compat::test_app;
