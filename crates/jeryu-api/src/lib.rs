//! Typed API facade for Phase 10 endpoints plus the GitHub-compatible REST edge.

pub mod github;
pub mod routes;
#[cfg(feature = "web")]
pub mod web;

pub use github::{GithubRouter, JERYU_API_VERSION, Method};
pub use routes::{ApiState, Response, Router};
