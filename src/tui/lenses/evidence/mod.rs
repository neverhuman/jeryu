//! Owner: Interactive TUI subsystem - Evidence flight recorder lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::evidence::`
//! Invariants: Searchable proof timeline + entity proof graph. `e` on any
//!             important entity opens its relevant proof. Redacted bundle
//!             export round-trip is part of the unit's acceptance.
//!
//! Scaffold only — view/data/nav/tests + subcomponents (timeline,
//! capsule_ledger, query, proof_viewer, bundle_export) land in U21.

pub const LENS_ID: super::LensId = super::LensId::Evidence;
