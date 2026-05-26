//! Owner: TUI runtime - data client transport selector and submodules.
//! Proof: `cargo nextest run -p jeryu --lib tui::runtime::data::`
//! Invariants: All TUI data fetches flow through `DataClient`; concrete
//!             transports (`HttpDataClient`, `FixtureDataClient`, etc.)
//!             never bypass the trait. The `DataTransport` enum exists so
//!             callers can switch transport without leaking concrete types
//!             into UI code. Stubs (`McpResourceDataClient`,
//!             `LocalDataClient`, `TraceDataClient`) return typed errors
//!             rather than panicking so the UI can surface a degraded
//!             badge instead of crashing.

pub mod client;
pub mod fixture;
pub mod http;
pub mod local;
pub mod mcp;
pub mod trace;

pub use client::{DataClient, EventPage};
pub use fixture::FixtureDataClient;
pub use http::HttpDataClient;
pub use local::LocalDataClient;
pub use mcp::McpResourceDataClient;
pub use trace::TraceDataClient;

use serde::{Deserialize, Serialize};

/// Which transport backs the active `DataClient`. The UI uses this to
/// render the source badge in the header and Source Doctor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataTransport {
    /// Primary HTTP transport (`/api/v1/*`).
    Http,
    /// Read-only MCP resources (post-stabilization).
    McpResource,
    /// Local DB-backed fallback (degraded).
    Local,
    /// Fixture-driven (demo, screenshots, tests).
    Fixture,
}

impl DataTransport {
    /// Short label for the header badge.
    pub fn label(self) -> &'static str {
        match self {
            Self::Http => "HTTP",
            Self::McpResource => "MCP",
            Self::Local => "LOCAL",
            Self::Fixture => "FIX",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_labels_are_stable() {
        assert_eq!(DataTransport::Http.label(), "HTTP");
        assert_eq!(DataTransport::McpResource.label(), "MCP");
        assert_eq!(DataTransport::Local.label(), "LOCAL");
        assert_eq!(DataTransport::Fixture.label(), "FIX");
    }

    #[test]
    fn transport_serde_roundtrip() {
        let json = serde_json::to_string(&DataTransport::Http).unwrap();
        assert_eq!(json, "\"http\"");
        let back: DataTransport = serde_json::from_str("\"fixture\"").unwrap();
        assert_eq!(back, DataTransport::Fixture);
    }
}
