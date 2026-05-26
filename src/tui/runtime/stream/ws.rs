//! Owner: TUI runtime - WebSocket stream stub.
//! Proof: `cargo nextest run -p jeryu --lib tui::runtime::stream::ws`
//! Invariants: WS is reserved for future tooling. The repo currently has
//!             no WebSocket dependency (per plan §17 hard rule: no new
//!             third-party deps); this file exists only so callers can
//!             refer to `StreamMode::Ws` symbolically. The real reader
//!             lands when WS tooling (e.g. `tokio-tungstenite`) is
//!             approved into the workspace.

use anyhow::{Result, anyhow};

/// Placeholder constructor. Always returns an error until WS support
/// arrives. UI code that picks the WS path should treat this error as
/// a signal to fall back to SSE or polling.
pub fn connect(_base_url: &str) -> Result<()> {
    Err(anyhow!(
        "websocket stream not yet wired (U10 follow-up; needs WS dep)"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_stub_returns_error() {
        let r = connect("http://127.0.0.1");
        assert!(r.is_err());
        assert!(
            r.unwrap_err().to_string().contains("websocket stream"),
            "stub error should mention websocket"
        );
    }
}
