//! Owner: Web Forge BFF — merge / review / merge-passport domain (W-B-11..13).
//! Proof: `cargo nextest run -p jeryu --lib merge::`
//! Invariants:
//!   - Approve / merge calls all flow through [`guards::verify_head_sha`] so
//!     no exact-SHA window slips between the user's preview and the host
//!     mutation (Tip1 Law 4).
//!   - Every mutation writes an audit row via [`crate::web::audit`] and
//!     publishes a WebSocket event via [`jeryu::web_events::WebEventBus`].
//!   - Merge Passport gates live in [`merge_gate::MergePassportService`];
//!     each blocker carries a stable `code` from §35.6 so the UI can map
//!     gate-by-gate explanations.
//!
//! See `WEB_WORK_CLAUDE.md` §7.2 W-B-11..13 and §35.1.14 (action algorithm).

pub mod guards;
pub mod merge_gate;
pub mod review;
pub mod service;

pub use merge_gate::MergePassportService;
pub use review::ReviewService;
pub use service::MergeService;
