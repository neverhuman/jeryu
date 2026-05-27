//! Owner: Web Forge BFF — repository domain (W-B-06 + W-B-07).
//! Proof: `cargo nextest run -p jeryu --lib repos`
//! Invariants:
//!   - Every `repo_id` accepted from the API surface is treated as opaque
//!     (§35.1.2). Decoding is centralized in [`RepoId::parse`] so handlers
//!     can never accidentally trust an unvalidated id.
//!   - The Phase-2 `repo_id` is `format!("{host}/{owner}/{name}")` so it is
//!     opaque to clients but human-decodable for ops. The §35.1.2 contract
//!     is "opaque from the client's perspective" — the encoding is internal.
//!   - All host calls go through `GitHost`; `HostError::NotImplemented`
//!     surfaces as `ApiError::Upstream`.
//!
//! See `WEB_WORK_CLAUDE.md` §7.2 W-B-06/W-B-07 and §35.1 for the
//! surface contracts.

pub mod create;
pub mod host_sync;
pub mod models;
pub mod permissions;
pub mod service;
pub mod settings;

pub use service::RepoService;
pub use settings::SettingsService;
