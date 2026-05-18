//! Owner: VTI Test Intelligence subsystem (module root)
//! Proof: `cargo nextest run -p jeryu -- test_intel`
//! Invariants: VTI routing remains explainable, deterministic, and conservative when evidence is incomplete.
//! Test Intelligence Plane (VTI).
//!
//! Subsystem-aware test selection, CI pipeline generation, and self-auditing
//! for the jeryu orchestrator. This module replaces the coarse lane-based
//! impact analysis with a fine-grained subsystem graph that maps changed
//! files to the minimal set of tests required to validate the change.

pub mod cache;
pub mod ci_gen;
pub mod explain;
pub mod nightly;
pub mod nightly_report;
pub mod planner;
pub mod subsystem;
pub mod testmap;
