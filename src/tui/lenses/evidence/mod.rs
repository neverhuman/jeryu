//! Owner: Interactive TUI subsystem - Evidence flight recorder lens
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::evidence::`
//! Invariants: Searchable proof timeline + entity proof graph. `e` on any
//!             important entity opens its relevant proof. Redacted bundle
//!             export round-trip is part of the unit's acceptance.
//!
//! Wave 4 U21 takes this scaffold and fills in real timeline, capsule
//! ledger, query, proof viewer, and bundle export subcomponents. The
//! current first-cut implementation renders typed `EvidenceLensInput`
//! (from `TuiReadModel`) and supports the universal keyboard grammar
//! (Enter/Esc/`/`/e/?) as intents.

pub mod data;
pub mod nav;
pub mod view;

pub use data::EvidenceLensInput;
pub use nav::{EvidenceIntent, handle_key};
pub use view::draw;

pub const LENS_ID: super::LensId = super::LensId::Evidence;
