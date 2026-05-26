//! Owner: TUI runtime - event stream pipeline (SSE -> poll -> degraded).
//! Proof: `cargo nextest run -p jeryu --lib tui::runtime::stream::`
//! Invariants: SSE is the preferred path; polling is the proven fallback;
//!             WS is reserved for future tooling. `DegradedTimeline` is
//!             the single typed signal the UI consumes to render `[poll]`,
//!             `LAST KNOWN`, and `SOURCE DOWN` badges (no string sniffing).

pub mod degraded;
pub mod poll;
pub mod sse;
pub mod ws;

pub use degraded::{DegradedReason, DegradedTimeline};
pub use poll::PollStream;
pub use sse::{SseStream, StreamEvent};

use serde::{Deserialize, Serialize};

/// Which streaming strategy is currently feeding the runtime. The header
/// badge derives from this and from the live `DegradedTimeline`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamMode {
    /// Live SSE connection to `/api/v1/events/stream`.
    Sse,
    /// HTTP polling against `/api/v1/events?cursor=N`.
    Poll,
    /// Last known state; no live source.
    Degraded,
    /// Future: WebSocket transport (lands when WS dep is wired).
    Ws,
}

impl StreamMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sse => "SSE",
            Self::Poll => "[poll]",
            Self::Degraded => "LAST KNOWN",
            Self::Ws => "WS",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_mode_labels_are_stable() {
        assert_eq!(StreamMode::Sse.label(), "SSE");
        assert_eq!(StreamMode::Poll.label(), "[poll]");
        assert_eq!(StreamMode::Degraded.label(), "LAST KNOWN");
        assert_eq!(StreamMode::Ws.label(), "WS");
    }

    #[test]
    fn stream_mode_serde_roundtrip() {
        let json = serde_json::to_string(&StreamMode::Poll).unwrap();
        assert_eq!(json, "\"poll\"");
        let back: StreamMode = serde_json::from_str("\"sse\"").unwrap();
        assert_eq!(back, StreamMode::Sse);
    }
}
