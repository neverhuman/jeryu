//! Owner: Inspection HTTP plane - /api/v1/* read routes for the TUI Flight Deck.
//! Proof: `cargo nextest run -p jeryu --lib inspection::`
//! Invariants: Routes return typed `crate::api` contracts only; no raw SQL or
//!             ad-hoc state mutation. Mutating routes live in `actions.rs`
//!             (U08); this module is read-only.

pub mod entity;
pub mod events;
pub mod health;
pub mod proof;
pub mod read_model;
pub mod router;
pub mod serve;
pub mod state;

pub use router::build_router;
pub use serve::serve_inspection;
pub use state::InspectionState;
