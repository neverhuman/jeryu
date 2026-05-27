//! REST endpoints for the JeRyu Web Forge BFF.
//!
//! Phase 1 ships the bootstrap surface only; W-B-* fill in repo/code/MR
//! /issue/settings/markdown/audit/agent endpoints atop the same
//! [`WebState`](super::state::WebState).
//!
//! Each submodule houses its route handlers; the router lives in
//! [`super::router`](super::router::build_web_router).

pub mod bootstrap;
