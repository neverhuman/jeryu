//! Typed API facade for Phase 10 endpoints plus the GitHub-compatible REST edge.

#[cfg(feature = "web")]
mod autonomy_bridge;
#[cfg(feature = "web")]
mod ci_bridge;
#[cfg(feature = "web")]
mod git_materializer;
#[cfg(feature = "web")]
mod git_transport;
pub mod github;
#[cfg(feature = "web")]
mod read_model;
pub mod routes;
#[cfg(feature = "web")]
pub mod web;

pub use github::{GithubRouter, JERYU_API_VERSION, Method};
pub use routes::{ApiState, Response, Router};
