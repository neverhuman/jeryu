//! JeRyu Web Forge BFF (Backend-For-Frontend).
//!
//! See `WEB_WORK_CLAUDE.md` §2, §28, §35 for architecture. The Phase-1
//! shell (W-B-01..05 + W-CC-05..09) lives here:
//!
//! * [`state`] — `WebState` service bag passed to every handler.
//! * [`error`] — Structured error envelope (§35.1.11).
//! * [`auth`] — Cookie-based session middleware (W-CC-07 prerequisite).
//! * [`csrf`] — Double-submit CSRF middleware (W-CC-09).
//! * [`permissions`] — Canonical 28-key permission set + scope→perm map
//!   used by both the REST handlers and the WS subscribe path (§35.1.18,
//!   §35.1.6).
//! * [`telemetry`] — `tower-http` request-id + trace + timeout +
//!   compression glue (W-CC-08).
//! * [`audit`] — Phase-1 tracing-only audit hook (W-CC-05). Real
//!   `audit_events` / `web_action_receipts` rows land when the service
//!   layer materializes.
//! * [`idempotency`] — In-memory replay cache (W-CC-06). DB-backed
//!   variant lands with the §35.1.16 migration.
//! * [`ws`] — Axum WebSocket handler (W-B-04).
//! * [`static_assets`] — SPA `ServeDir` with `index.html` fallback
//!   (W-B-03).
//! * [`rest`] — REST endpoints; Phase 1 ships `/api/v1/bootstrap`
//!   (W-B-05).
//! * [`router`] — Composition (W-B-02): merges legacy engine routes,
//!   API, WS, and SPA fallback under the cross-cutting middleware
//!   stack.
//! * [`command`] — `jeryu web serve` CLI entry point (W-B-01).

pub mod audit;
pub mod auth;
pub mod command;
pub mod csrf;
pub mod error;
pub mod idempotency;
pub mod openapi;
pub mod permissions;
pub mod rest;
pub mod router;
pub mod sessions;
pub mod state;
pub mod static_assets;
pub mod telemetry;
pub mod ws;
