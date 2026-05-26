//! Web Forge BFF (Backend-For-Frontend).
//!
//! See `WEB_WORK_CLAUDE.md` §2 and §35 for architecture. The heavy server
//! dependencies (axum, tower, tower-http) are gated behind the `web` Cargo
//! feature flag and land incrementally in W-B-01..04.
//!
//! The Phase-0 CLI stub added in W-F-10 lives in [`command`] and uses only
//! `anyhow`, so this module is always compiled; the gating happens inside
//! the future BFF submodules.

pub mod command;
