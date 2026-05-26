//! Owner: Interactive TUI subsystem - Source Doctor lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::source_doctor::`
//! Invariants: Surfaces source health, schema drift, action drift, MCP
//!             drift, docs drift, and DB profile mismatch. Drives the
//!             header's worst-source freshness badge.
//!
//! First-cut scaffold — per-source freshness, drift detectors, and the
//! R1 `reconnect` action wiring land as part of the U29 settings/
//! source-doctor unit. Current first-cut implementation renders typed
//! `SourceDoctorLensInput` (from `TuiReadModel`) and supports the
//! universal keyboard grammar (Enter/Esc/e/r/?) as intents.

pub mod data;
pub mod nav;
pub mod view;

pub use data::SourceDoctorLensInput;
pub use nav::{SourceDoctorIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::SourceDoctor;
