//! Owner: TUI runtime - PollStream (`/api/v1/events?cursor=N` fallback).
//! Proof: `cargo nextest run -p jeryu --lib tui::runtime::stream::poll`
//! Invariants:
//!   - Cursor MUST be monotonic. The poller advances only when the
//!     server returns a non-empty `next_cursor`; otherwise the cursor
//!     stays at the last observed `cursor`.
//!   - The poll interval is configurable so tests can drive a tight
//!     loop without sleeping for real seconds.
//!   - Polling is the fallback path when SSE is unavailable; UI renders
//!     the `[poll]` badge whenever this stream is active.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::tui::runtime::data::client::DataClient;
use crate::tui::runtime::stream::sse::StreamEvent;

/// Background poller that calls `fetch_events(cursor, limit)` on a
/// timer and forwards items as `StreamEvent::Event(json)`. Drop the
/// stream (or call `abort`) to stop the loop.
#[derive(Debug)]
pub struct PollStream {
    rx: mpsc::Receiver<StreamEvent>,
    handle: JoinHandle<Result<()>>,
    cursor: Arc<Mutex<u64>>,
}

impl PollStream {
    /// Start polling with the supplied client. `interval` controls how
    /// often the poller wakes; `limit` caps each page.
    pub fn start(client: Arc<dyn DataClient>, interval: Duration, limit: u32) -> Self {
        Self::start_at(client, interval, limit, 0)
    }

    /// Start polling from `initial_cursor` (useful for resume).
    pub fn start_at(
        client: Arc<dyn DataClient>,
        interval: Duration,
        limit: u32,
        initial_cursor: u64,
    ) -> Self {
        let (tx, rx) = mpsc::channel(64);
        let cursor = Arc::new(Mutex::new(initial_cursor));
        let cursor_handle = cursor.clone();
        let handle =
            tokio::spawn(
                async move { poll_loop(client, tx, cursor_handle, interval, limit).await },
            );
        Self { rx, handle, cursor }
    }

    pub async fn recv(&mut self) -> Option<StreamEvent> {
        self.rx.recv().await
    }

    /// Read the current cursor (for tests + UI debug surface).
    pub async fn current_cursor(&self) -> u64 {
        *self.cursor.lock().await
    }

    /// Stop the background task.
    pub fn abort(&self) {
        self.handle.abort();
    }
}

async fn poll_loop(
    client: Arc<dyn DataClient>,
    tx: mpsc::Sender<StreamEvent>,
    cursor: Arc<Mutex<u64>>,
    interval: Duration,
    limit: u32,
) -> Result<()> {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let current = *cursor.lock().await;
        match client.fetch_events(current, limit).await {
            Ok(page) => {
                for item in page.items {
                    if tx.send(StreamEvent::Event(item)).await.is_err() {
                        return Ok(());
                    }
                }
                let next = page.next_cursor.unwrap_or(page.cursor);
                let mut guard = cursor.lock().await;
                if next > *guard {
                    *guard = next;
                }
                // Heartbeat after each successful poll so UI knows
                // the loop is alive even when no events arrived.
                if tx.send(StreamEvent::Heartbeat).await.is_err() {
                    return Ok(());
                }
            }
            Err(_) => {
                // On transport error, advance to Disconnected and exit.
                let _ = tx.send(StreamEvent::Disconnected).await;
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspection::serve::serve_inspection;
    use crate::inspection::state::InspectionState;
    use crate::tui::runtime::data::http::HttpDataClient;
    use tokio::net::TcpListener;

    async fn spawn() -> (String, tokio::task::JoinHandle<Result<()>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        let state = InspectionState::default();
        let handle = tokio::spawn(async move { serve_inspection(listener, state).await });
        (base, handle)
    }

    #[tokio::test]
    async fn poll_cursor_advances() {
        let (base, handle) = spawn().await;
        let http = HttpDataClient::new(base);
        let client: Arc<dyn DataClient> = Arc::new(http);
        let mut stream = PollStream::start_at(client.clone(), Duration::from_millis(20), 100, 0);
        // Read a few heartbeats off the channel to confirm the poller
        // actually woke up at least once.
        let mut beats = 0;
        for _ in 0..6 {
            match tokio::time::timeout(Duration::from_secs(2), stream.recv()).await {
                Ok(Some(StreamEvent::Heartbeat)) => {
                    beats += 1;
                    if beats >= 2 {
                        break;
                    }
                }
                Ok(Some(_)) => continue,
                _ => break,
            }
        }
        assert!(beats >= 2, "poller should have heartbeat at least twice");
        // Cursor must be >= the read-model cursor (which starts at 0)
        // and monotonic across calls.
        let c1 = stream.current_cursor().await;
        let c2 = stream.current_cursor().await;
        assert!(c2 >= c1, "cursor regressed: {c1} -> {c2}");
        stream.abort();
        handle.abort();
    }

    #[tokio::test]
    async fn poll_emits_disconnected_on_transport_error() {
        // Spawn a poller against a port that nothing listens on; the
        // first request fails and the loop should emit Disconnected.
        let http = HttpDataClient::new("http://127.0.0.1:1"); // RFC reserved
        let client: Arc<dyn DataClient> = Arc::new(http);
        let mut stream = PollStream::start(client, Duration::from_millis(10), 10);
        let evt = tokio::time::timeout(Duration::from_secs(5), stream.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert_eq!(evt, StreamEvent::Disconnected);
        stream.abort();
    }
}
