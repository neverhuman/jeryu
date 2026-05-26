//! Owner: Interactive TUI subsystem - Source Doctor lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::source_doctor::`
//! Invariants: Surfaces source health, schema drift, action drift, MCP
//!             drift, docs drift, and DB profile mismatch. Drives the
//!             header's worst-source freshness badge.
//!
//! Scaffold only — view/data/nav/tests + subcomponents land as part of
//! the U29 settings/source-doctor unit.

pub const LENS_ID: super::LensId = super::LensId::SourceDoctor;
