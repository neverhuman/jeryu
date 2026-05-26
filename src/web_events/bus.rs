//! `WebEventBus` — bounded broadcast bus with seq-based ordering.
//!
//! Per WEB_WORK_CLAUDE.md §35.1.13, the full channel drops Low priority first.
//! The drop policy is implemented at the publisher boundary using two
//! `tokio::sync::broadcast` channels: a larger one for High and Medium events
//! (default capacity 4096) and a smaller one for Low events (default 1024).
//! Saturation on the Low channel surfaces as a `SendError` we silently absorb,
//! while back-pressure on the High/Medium channel surfaces to consumers as
//! `RecvError::Lagged` per `tokio::sync::broadcast`'s contract.
//!
//! Every published event gets a freshly-allocated monotonic `seq` from an
//! atomic counter. The current top-of-stream seq is observable through
//! [`WebEventBus::current_seq`], which the WS handler uses when emitting the
//! initial `ServerWsMessage::Hello` frame (§35.1.12) and when deciding whether
//! a resuming client is too far behind to catch up (§35.1.13).

use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;

use super::protocol::{EventPriority, WebEvent};

/// Default capacity for the High/Medium broadcast channel.
///
/// Sized to absorb several seconds of bursty activity at the steady-state
/// rates seen in `WEB_WORK_CLAUDE.md` §35.1.13. Tunable per deployment via
/// [`WebEventBus::new`].
pub const DEFAULT_CAPACITY_HIGH: usize = 4096;

/// Default capacity for the Low broadcast channel.
///
/// Smaller because Low-priority traffic (log chunks, periodic health pings)
/// is intentionally droppable when the bus is under pressure.
pub const DEFAULT_CAPACITY_LOW: usize = 1024;

/// Bounded fan-out bus for `WebEvent` traffic.
///
/// Each WebSocket connection (W-B-04) subscribes to one or both internal
/// channels. The bus assigns a monotonic `seq` to every published event;
/// callers must not pre-fill `event.seq` (any value provided is overwritten).
#[derive(Debug)]
pub struct WebEventBus {
    seq: AtomicU64,
    high_tx: broadcast::Sender<WebEvent>,
    low_tx: broadcast::Sender<WebEvent>,
}

impl WebEventBus {
    /// Construct a bus with explicit per-channel capacities.
    ///
    /// Both capacities must be positive (tokio's broadcast channel panics on
    /// zero); we don't enforce a minimum here because the constants in this
    /// module are the only call sites in production.
    pub fn new(capacity_high: usize, capacity_low: usize) -> Self {
        let (high_tx, _) = broadcast::channel(capacity_high);
        let (low_tx, _) = broadcast::channel(capacity_low);
        Self {
            seq: AtomicU64::new(0),
            high_tx,
            low_tx,
        }
    }

    /// Construct a bus with the default capacities defined in this module.
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_CAPACITY_HIGH, DEFAULT_CAPACITY_LOW)
    }

    /// Current top-of-stream sequence number. Equal to the last seq returned
    /// by [`Self::publish`], or `0` if nothing has been published yet.
    pub fn current_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }

    /// Subscribe to High/Medium-priority events. Consumers are responsible
    /// for draining promptly; `RecvError::Lagged` indicates back-pressure.
    pub fn subscribe_high(&self) -> broadcast::Receiver<WebEvent> {
        self.high_tx.subscribe()
    }

    /// Subscribe to Low-priority events. Saturation results in silent drop
    /// at publish time, so this channel never reports `Lagged` for that
    /// reason in practice (it still can if the subscriber stalls).
    pub fn subscribe_low(&self) -> broadcast::Receiver<WebEvent> {
        self.low_tx.subscribe()
    }

    /// Publish a `WebEvent` with a freshly-allocated `seq`. Returns the seq
    /// assigned. Low-priority events may be dropped if the low channel is
    /// saturated; High/Medium events are sent on the high channel and any
    /// back-pressure surfaces as `Lagged` to the consumer.
    pub fn publish(&self, mut event: WebEvent) -> u64 {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        event.seq = seq;
        match EventPriority::from_kind(&event.kind) {
            EventPriority::Low => {
                let _ = self.low_tx.send(event);
            }
            _ => {
                let _ = self.high_tx.send(event);
            }
        }
        seq
    }
}

impl Default for WebEventBus {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_event(kind: &str) -> WebEvent {
        WebEvent {
            seq: 0,
            timestamp: Utc::now(),
            scope: "global.activity".into(),
            kind: kind.into(),
            entity: "test".into(),
            summary: "t".into(),
            payload: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn seq_monotonically_increases() {
        let bus = WebEventBus::with_defaults();
        let a = bus.publish(make_event("repo.updated"));
        let b = bus.publish(make_event("repo.updated"));
        assert!(b > a);
        assert_eq!(bus.current_seq(), b);
    }

    #[tokio::test]
    async fn high_priority_routes_through_high_tx() {
        let bus = WebEventBus::with_defaults();
        let mut rx = bus.subscribe_high();
        let _ = bus.publish(make_event("mr.approved"));
        let evt = rx.try_recv().expect("should receive on high");
        assert_eq!(evt.kind, "mr.approved");
    }

    #[tokio::test]
    async fn low_priority_routes_through_low_tx() {
        let bus = WebEventBus::with_defaults();
        let mut rx = bus.subscribe_low();
        let _ = bus.publish(make_event("job.log.chunk"));
        let evt = rx.try_recv().expect("should receive on low");
        assert_eq!(evt.kind, "job.log.chunk");
    }

    #[tokio::test]
    async fn medium_priority_routes_through_high_tx() {
        let bus = WebEventBus::with_defaults();
        let mut rx_high = bus.subscribe_high();
        let mut rx_low = bus.subscribe_low();
        let _ = bus.publish(make_event("pipeline.created"));
        let evt = rx_high.try_recv().expect("medium should arrive on high");
        assert_eq!(evt.kind, "pipeline.created");
        assert!(
            rx_low.try_recv().is_err(),
            "medium must not arrive on low channel"
        );
    }
}
