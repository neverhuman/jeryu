//! Owner: TUI Control-Plane API — unified read-model, event stream, and action dispatch
//! Proof: `cargo nextest run -p jeryu -- api`
//! Invariants: All TUI rendering consumes typed API projections, never raw DB/Docker/GitLab state.
//! The API module is the single source of truth for entity types, event contracts, and action dispatch.
//!
//! Web Forge BFF DTOs (W-F-03): `repository`, `repo_browser`, `merge_request`,
//! `review`, `settings`, `web_read_model`, `issues`, `websocket`. These derive
//! `ts_rs::TS`, `utoipa::ToSchema`, and `schemars::JsonSchema` under
//! `#[cfg(feature = "web")]` and export TypeScript bindings to
//! `contracts/generated/` per `agent/boundaries.toml`'s `generated_paths`.

pub mod actions;
pub mod agent_session;
pub mod capacity;
pub mod dashboards;
pub mod entity;
pub mod events;
pub mod freshness;
pub mod inspection;
pub mod issues;
pub mod merge_request;
pub mod proof;
pub mod read_model;
pub mod repo_browser;
pub mod repository;
pub mod review;
pub mod runtime_profile;
pub mod settings;
pub mod snapshot;
pub mod web_read_model;
pub mod websocket;
