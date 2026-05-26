//! Owner: Interactive TUI subsystem - Mission Control lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::mission::`
//! Invariants: Renders global posture, top blocker, source freshness, next
//!             action. Never performs backend I/O during draw.
//!
//! Scaffold only — view/data/nav/tests submodules land in U16.

pub const LENS_ID: super::LensId = super::LensId::Mission;
