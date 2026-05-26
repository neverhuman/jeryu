//! Owner: TUI Control-Plane API — unified read-model, event stream, and action dispatch
//! Proof: `cargo nextest run -p jeryu -- api`
//! Invariants: All TUI rendering consumes typed API projections, never raw DB/Docker/GitLab state.
//! The API module is the single source of truth for entity types, event contracts, and action dispatch.

pub mod actions;
pub mod agent_session;
pub mod entity;
pub mod events;
pub mod freshness;
pub mod inspection;
pub mod read_model;
pub mod runtime_profile;
pub mod snapshot;
