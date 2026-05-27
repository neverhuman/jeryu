//! Axum WebSocket handler for `GET /api/v1/ws` (W-B-04).
//!
//! Wire protocol: `jeryu.ws.v1` (defined in `crate::api::websocket`).
//!
//! Lifecycle (§35.1.12):
//! 1. Upgrade -> server sends `Hello { current_seq }`.
//! 2. Server pings every 15 s; client must reply within 30 s.
//! 3. Client `Hello`/`Subscribe`/`Unsubscribe` frames mutate the local
//!    `SubscriptionRegistry`. Each scope is re-checked against the
//!    viewer's permissions per §35.1.6; unauthorized scopes are silently
//!    dropped from the subscription set and the client receives an
//!    `Error { code: "subscribe_forbidden", scopes }` frame.
//! 4. Server forwards every published `WebEvent` whose scope is in the
//!    registry, prefixed with the priority-class routing (`High`/`Medium`
//!    on the `high` channel, `Low` on the `low` channel).
//! 5. `RecvError::Lagged` on either channel terminates with
//!    `SnapshotRequired { reason: "lag" }` (§35.1.13).

use std::collections::HashSet;
use std::time::Duration;

use axum::{
    Extension,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast::error::RecvError;

use jeryu::api::websocket::{ClientWsMessage, ServerWsMessage};

use crate::web::auth::Viewer;
use crate::web::permissions;
use crate::web::state::WebState;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Axum handler — upgrades the request and spawns `handle_socket`.
pub async fn ws_handler(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, viewer, socket))
}

async fn handle_socket(state: WebState, viewer: Viewer, socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to both priority classes; the bus owns drop policy
    // (§35.1.13). The WS handler just demuxes by scope.
    let mut rx_high = state.event_bus.subscribe_high();
    let mut rx_low = state.event_bus.subscribe_low();
    let mut subs: HashSet<String> = HashSet::new();

    // §35.1.12 step 1 — initial Hello with the current seq.
    let hello = ServerWsMessage::hello(state.event_bus.current_seq());
    if sender
        .send(Message::Text(hello.unwrap_json().into()))
        .await
        .is_err()
    {
        return;
    }

    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Fire the first tick immediately on entry; the loop drains it.
    let _ = heartbeat.tick().await;

    loop {
        tokio::select! {
            // ── Client frame ──
            msg = tokio::time::timeout(READ_TIMEOUT, receiver.next()) => match msg {
                Ok(Some(Ok(Message::Text(t)))) => {
                    if !handle_client_frame(&t, &viewer, &mut subs, &mut sender).await {
                        break;
                    }
                }
                Ok(Some(Ok(Message::Binary(_)))) => {
                    // Phase 1 ignores binary frames.
                }
                Ok(Some(Ok(Message::Ping(p)))) => {
                    if sender.send(Message::Pong(p)).await.is_err() {
                        break;
                    }
                }
                Ok(Some(Ok(Message::Pong(_)))) => {}
                Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) => break,
                Err(_elapsed) => {
                    // §35.1.12 — 30 s read timeout means the peer is gone.
                    break;
                }
            },

            // ── Event broadcast (high/medium priority) ──
            evt = rx_high.recv() => match evt {
                Ok(event) => {
                    if subs.contains("global") || subs.contains(&event.scope) {
                        let frame = ServerWsMessage::event(event).unwrap_json();
                        if sender.send(Message::Text(frame.into())).await.is_err() {
                            break;
                        }
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    let snap = ServerWsMessage::SnapshotRequired {
                        reason: "lag".into(),
                        current_seq: state.event_bus.current_seq(),
                    };
                    let _ = sender.send(Message::Text(snap.unwrap_json().into())).await;
                    break;
                }
                Err(RecvError::Closed) => break,
            },

            // ── Event broadcast (low priority) ──
            evt = rx_low.recv() => match evt {
                Ok(event) => {
                    if subs.contains("global") || subs.contains(&event.scope) {
                        let frame = ServerWsMessage::event(event).unwrap_json();
                        if sender.send(Message::Text(frame.into())).await.is_err() {
                            break;
                        }
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    // Low-priority lag is expected under load; the bus
                    // already discarded silently on publish. We do not
                    // forcibly disconnect the client.
                    continue;
                }
                Err(RecvError::Closed) => break,
            },

            // ── Server heartbeat ──
            _ = heartbeat.tick() => {
                if sender.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// Apply a single client frame. Returns `false` to indicate the socket
/// should be terminated.
async fn handle_client_frame(
    text: &str,
    viewer: &Viewer,
    subs: &mut HashSet<String>,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> bool {
    let Ok(frame) = serde_json::from_str::<ClientWsMessage>(text) else {
        // Phase 1: silently drop malformed frames. A future hardening
        // pass can emit Error frames for invalid JSON.
        return true;
    };

    match frame {
        ClientWsMessage::Hello { subscriptions, .. }
        | ClientWsMessage::Subscribe { subscriptions } => {
            let mut forbidden = Vec::new();
            for spec in subscriptions {
                let perm = permissions::perm_for_scope(&spec.scope);
                let allowed = perm.map(|p| viewer.perms.contains(p)).unwrap_or(true);
                if allowed {
                    subs.insert(spec.scope);
                } else {
                    forbidden.push(spec.scope);
                }
            }
            if !forbidden.is_empty() {
                let err = ServerWsMessage::Error {
                    code: "subscribe_forbidden".into(),
                    message: format!("missing perms for scopes: {forbidden:?}"),
                };
                if sender
                    .send(Message::Text(err.unwrap_json().into()))
                    .await
                    .is_err()
                {
                    return false;
                }
            }
        }
        ClientWsMessage::Unsubscribe { scopes } => {
            for s in scopes {
                subs.remove(&s);
            }
        }
        ClientWsMessage::Ping { nonce } => {
            let pong = ServerWsMessage::Pong {
                nonce,
                server_time: chrono::Utc::now(),
            };
            if sender
                .send(Message::Text(pong.unwrap_json().into()))
                .await
                .is_err()
            {
                return false;
            }
        }
        ClientWsMessage::Ack { .. } => {
            // Ack semantics land with the snapshot/resume work (§35.1.13
            // resume-from path). Phase 1 acknowledges by no-op.
        }
    }
    true
}
