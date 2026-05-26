//! Owner: TUI Control-Plane API - per-lens dashboard contracts
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards`
//! Invariants: Each dashboard is a typed view consumed by exactly one lens.
//!             Real projections fill them; lenses render whatever the
//!             projection produced (including empty/degraded states).

pub mod aer;
pub mod agents;
pub mod artifacts;
pub mod autonomy;
pub mod bottlenecks;
pub mod bugs;
pub mod cache;
pub mod evidence;
pub mod fleet;
pub mod git_sync;
pub mod jankurai;
pub mod llms;
pub mod queue;
pub mod release;
pub mod runners;
pub mod security;
pub mod source_doctor;
pub mod vti;
pub mod workflow;
