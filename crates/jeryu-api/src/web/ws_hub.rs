use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// Live-stream fan-out hub for the WebSocket event spine.
///
/// The tokio `sync` feature is intentionally NOT enabled in this crate, so this
/// is a deliberately minimal `Arc<Mutex<_>>` registry rather than a
/// `tokio::sync::broadcast`. It hands out the server-wide monotonic event
/// sequence and tracks which scopes each live connection is subscribed to, so a
/// future producer can fan deltas out to exactly the interested connections.
/// The snapshot-on-subscribe path works entirely through this hub today.
#[derive(Clone, Default)]
pub(super) struct WsHub {
    inner: Arc<Mutex<WsHubInner>>,
}

#[derive(Default)]
struct WsHubInner {
    /// Server-wide monotonic event sequence; never reused, never decreases.
    next_seq: u64,
    /// Live connections, in registration order. Each tracks its own scopes.
    connections: Vec<WsConnection>,
}

/// A single live WebSocket connection's subscription state inside the hub.
struct WsConnection {
    id: u64,
    scopes: BTreeSet<String>,
}

impl WsHub {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Allocate the next monotonic event sequence number.
    pub(super) fn next_seq(&self) -> u64 {
        let mut inner = self.inner.lock().expect("ws hub mutex poisoned");
        inner.next_seq = inner.next_seq.saturating_add(1);
        inner.next_seq
    }

    /// The highest sequence handed out so far (0 before any event).
    pub(super) fn current_seq(&self) -> u64 {
        self.inner.lock().expect("ws hub mutex poisoned").next_seq
    }

    /// Register a fresh connection and return its hub-unique id.
    pub(super) fn register(&self) -> u64 {
        let mut inner = self.inner.lock().expect("ws hub mutex poisoned");
        let id = inner
            .next_seq
            .wrapping_add(inner.connections.len() as u64 + 1);
        inner.connections.push(WsConnection {
            id,
            scopes: BTreeSet::new(),
        });
        id
    }

    /// Replace the scope set a connection is subscribed to.
    pub(super) fn set_scopes(&self, id: u64, scopes: &BTreeSet<String>) {
        let mut inner = self.inner.lock().expect("ws hub mutex poisoned");
        if let Some(conn) = inner.connections.iter_mut().find(|c| c.id == id) {
            conn.scopes = scopes.clone();
        }
    }

    /// Drop scopes from a connection's subscription set.
    pub(super) fn remove_scopes(&self, id: u64, scopes: &[String]) {
        let mut inner = self.inner.lock().expect("ws hub mutex poisoned");
        if let Some(conn) = inner.connections.iter_mut().find(|c| c.id == id) {
            for scope in scopes {
                conn.scopes.remove(scope);
            }
        }
    }

    /// Forget a connection entirely (on socket close).
    pub(super) fn unregister(&self, id: u64) {
        let mut inner = self.inner.lock().expect("ws hub mutex poisoned");
        inner.connections.retain(|c| c.id != id);
    }
}
