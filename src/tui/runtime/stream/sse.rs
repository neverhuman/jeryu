//! Owner: TUI runtime - SseStream (live `/api/v1/events/stream` consumer).
//! Proof: `cargo nextest run -p jeryu --lib tui::runtime::stream::sse`
//! Invariants:
//!   - We hand-parse `event: ... / data: ...` line pairs rather than
//!     pulling in `eventsource-stream` or a similar dep (zero new
//!     dependencies per plan §17 hard rule).
//!   - The parser is line-based; partial chunks are buffered across
//!     reads. Each event terminates on a blank line (per SSE spec).
//!   - `Disconnected` is the last event the channel emits before close;
//!     callers MUST observe it to switch to PollStream and render the
//!     `[poll]` badge.

use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// What the SSE channel emits. UI only reads from this enum; it never
/// touches the raw bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Server liveness ping. UI uses this to confirm the connection.
    Heartbeat,
    /// A control-plane event (raw JSON; decoded downstream into
    /// `TuiEvent` once the schema stabilises).
    Event(serde_json::Value),
    /// The connection closed. Always the last event before channel drop.
    Disconnected,
}

/// Background SSE reader. Drop the handle to cancel the connection.
#[derive(Debug)]
pub struct SseStream {
    rx: mpsc::Receiver<StreamEvent>,
    handle: JoinHandle<Result<()>>,
}

impl SseStream {
    /// Connect to `{base_url}/api/v1/events/stream`. The returned stream
    /// emits `StreamEvent::Heartbeat` and `Event(json)` until the
    /// server closes; then a final `Disconnected` is emitted.
    pub async fn connect(base_url: impl Into<String>) -> Result<Self> {
        let url = format!(
            "{}/api/v1/events/stream",
            base_url.into().trim_end_matches('/')
        );
        let resp = reqwest::Client::new()
            .get(&url)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .with_context(|| format!("sse: GET {url}"))?
            .error_for_status()
            .with_context(|| format!("sse: GET {url} non-2xx"))?;

        let (tx, rx) = mpsc::channel(64);
        let handle = tokio::spawn(reader_loop(resp, tx));
        Ok(Self { rx, handle })
    }

    /// Receive the next stream event (None when the channel closes).
    pub async fn recv(&mut self) -> Option<StreamEvent> {
        self.rx.recv().await
    }

    /// Abort the background reader.
    pub fn abort(&self) {
        self.handle.abort();
    }
}

async fn reader_loop(mut resp: reqwest::Response, tx: mpsc::Sender<StreamEvent>) -> Result<()> {
    // Stream the body and parse `event: ...\ndata: ...\n\n` blocks.
    // We use `Response::chunk()` (no extra features needed) instead of
    // `bytes_stream()` to keep reqwest at its current feature set.
    let mut buf = String::new();
    let mut current_event: Option<String> = None;
    let mut current_data: Vec<String> = Vec::new();
    loop {
        let bytes = match resp.chunk().await {
            Ok(Some(b)) => b,
            Ok(None) => break,
            Err(_) => break,
        };
        buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(nl_pos) = buf.find('\n') {
            let line = buf[..nl_pos].trim_end_matches('\r').to_string();
            buf.drain(..=nl_pos);
            if line.is_empty() {
                // End of an event block; emit what we have.
                if let Some(name) = current_event.take() {
                    let data = current_data.join("\n");
                    let event = build_event(&name, &data);
                    if tx.send(event).await.is_err() {
                        return Ok(());
                    }
                }
                current_data.clear();
            } else if let Some(rest) = line.strip_prefix("event:") {
                current_event = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("data:") {
                current_data.push(rest.trim().to_string());
            }
            // Other SSE fields (id:, retry:) are ignored for now.
        }
    }
    let _ = tx.send(StreamEvent::Disconnected).await;
    Ok(())
}

fn build_event(name: &str, data: &str) -> StreamEvent {
    if name == "heartbeat" {
        return StreamEvent::Heartbeat;
    }
    match serde_json::from_str::<serde_json::Value>(data) {
        Ok(v) => StreamEvent::Event(v),
        Err(_) => StreamEvent::Event(serde_json::json!({ "raw": data, "event": name })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspection::serve::serve_inspection;
    use crate::inspection::state::InspectionState;
    use tokio::net::TcpListener;

    async fn spawn() -> (String, tokio::task::JoinHandle<anyhow::Result<()>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        let state = InspectionState::default();
        let handle = tokio::spawn(async move { serve_inspection(listener, state).await });
        (base, handle)
    }

    #[tokio::test]
    async fn sse_parses_heartbeat() {
        let (base, handle) = spawn().await;
        let mut stream = SseStream::connect(&base).await.expect("connect");
        // The server currently sends a single heartbeat then closes the
        // body. We assert that we observe heartbeat then disconnect.
        let first = tokio::time::timeout(Duration::from_secs(5), stream.recv())
            .await
            .expect("recv timed out")
            .expect("channel closed before event");
        assert_eq!(first, StreamEvent::Heartbeat);
        // Drain the channel until disconnect.
        let mut saw_disconnect = false;
        for _ in 0..4 {
            match tokio::time::timeout(Duration::from_secs(2), stream.recv()).await {
                Ok(Some(StreamEvent::Disconnected)) => {
                    saw_disconnect = true;
                    break;
                }
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(_) => break,
            }
        }
        assert!(saw_disconnect, "expected Disconnected sentinel");
        handle.abort();
    }

    #[test]
    fn build_event_decodes_json_data() {
        let e = build_event("custom", "{\"a\":1}");
        match e {
            StreamEvent::Event(v) => assert_eq!(v["a"], 1),
            _ => panic!("expected Event variant"),
        }
    }

    #[test]
    fn build_event_falls_back_to_raw_on_invalid_json() {
        let e = build_event("custom", "not-json");
        match e {
            StreamEvent::Event(v) => {
                assert_eq!(v["raw"], "not-json");
                assert_eq!(v["event"], "custom");
            }
            _ => panic!("expected Event variant"),
        }
    }
}
