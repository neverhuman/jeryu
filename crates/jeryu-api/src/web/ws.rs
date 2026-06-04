use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::StreamExt;
use jeryu_readmodel::Bottleneck;
use jeryu_readmodel::contracts::{ServerWsMessage, WebEvent};
use serde_json::{Value, json};

use super::surface::serialize_payload;
use super::{WebState, server_time, workcells};

pub(super) async fn ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WebState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<WebState>) {
    let conn_id = state.ws.register();
    let _ = send_server_message(&mut socket, hello_message(&state)).await;
    // Per-connection scope subscription set, mirrored into the hub registry.
    let mut scopes: BTreeSet<String> = BTreeSet::new();
    while let Some(message) = socket.next().await {
        match message {
            Ok(Message::Text(text)) => {
                if let Ok(value) = serde_json::from_str::<Value>(&text) {
                    match value.get("type").and_then(Value::as_str) {
                        Some("ping") => {
                            let nonce = value
                                .get("nonce")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let _ = send_server_message(
                                &mut socket,
                                ServerWsMessage::Pong {
                                    nonce,
                                    server_time: server_time(),
                                },
                            )
                            .await;
                        }
                        Some("hello") => {
                            // A `hello` may carry an initial subscription set.
                            for scope in requested_scopes(&value) {
                                scopes.insert(scope);
                            }
                            state.ws.set_scopes(conn_id, &scopes);
                            let _ = send_server_message(&mut socket, hello_message(&state)).await;
                            send_scope_snapshots(&mut socket, &state, &scopes).await;
                        }
                        Some("subscribe") => {
                            // Track the newly requested scopes and immediately push
                            // a snapshot Event frame for each, so the client paints
                            // from live read-model data without waiting for a delta.
                            let added: Vec<String> = requested_scopes(&value);
                            for scope in &added {
                                scopes.insert(scope.clone());
                            }
                            state.ws.set_scopes(conn_id, &scopes);
                            let snapshot_scopes: BTreeSet<String> = added.into_iter().collect();
                            send_scope_snapshots(&mut socket, &state, &snapshot_scopes).await;
                        }
                        Some("unsubscribe") => {
                            let dropped = unsubscribe_scopes(&value);
                            for scope in &dropped {
                                scopes.remove(scope);
                            }
                            state.ws.remove_scopes(conn_id, &dropped);
                        }
                        Some("ack") => {}
                        _ => {
                            let _ = send_server_message(
                                &mut socket,
                                ServerWsMessage::Error {
                                    code: "unknown_message".to_string(),
                                    message: "unsupported websocket message type".to_string(),
                                },
                            )
                            .await;
                        }
                    }
                }
            }
            Ok(Message::Ping(payload)) => {
                let _ = socket.send(Message::Pong(payload)).await;
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
    state.ws.unregister(conn_id);
}

/// Extract subscription scopes from a `hello`/`subscribe` frame. Both carry
/// `subscriptions: [{ scope, filters }]` per the [`ClientWsMessage`] contract.
pub(super) fn requested_scopes(value: &Value) -> Vec<String> {
    value
        .get("subscriptions")
        .and_then(Value::as_array)
        .map(|specs| {
            specs
                .iter()
                .filter_map(|spec| spec.get("scope").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Extract the scope list from an `unsubscribe` frame (`scopes: [..]`).
pub(super) fn unsubscribe_scopes(value: &Value) -> Vec<String> {
    value
        .get("scopes")
        .and_then(Value::as_array)
        .map(|scopes| {
            scopes
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Push one snapshot [`ServerWsMessage::Event`] frame per subscribed scope, each
/// stamped with a fresh monotonic sequence from the hub.
async fn send_scope_snapshots(socket: &mut WebSocket, state: &WebState, scopes: &BTreeSet<String>) {
    for scope in scopes {
        if let Some(event) = snapshot_event(state, scope) {
            let _ = send_server_message(socket, ServerWsMessage::Event { event }).await;
        }
    }
}

/// Build a snapshot [`WebEvent`] for a subscribed scope from the read model.
///
/// Supported scopes: `global.activity` (server-wide pool totals + bottlenecks),
/// `pool.{name}` (one pool's rollup), and `system.health` (component health).
/// Unknown scopes yield `None` and are silently ignored (no spurious frame).
pub(super) fn snapshot_event(state: &WebState, scope: &str) -> Option<WebEvent> {
    let activity = &state.tui.pool_activity;
    let seq = state.ws.next_seq();
    let timestamp = server_time();
    if scope == "global.activity" {
        let totals = activity.totals();
        let bottlenecks: Vec<String> = activity
            .bottlenecks()
            .iter()
            .map(Bottleneck::describe)
            .collect();
        return Some(WebEvent {
            seq,
            timestamp,
            scope: scope.to_string(),
            kind: "activity.snapshot".to_string(),
            entity: "global".to_string(),
            summary: format!(
                "{} queued / {} running / {} failed across {} pool(s)",
                totals.queued_jobs, totals.running_jobs, totals.failed_jobs, totals.pools
            ),
            payload: json!({
                "health": activity.health(),
                "totals": totals,
                "bottlenecks": bottlenecks,
            }),
        });
    }
    if let Some(pool_name) = scope.strip_prefix("pool.") {
        let pool = activity.pools.iter().find(|p| p.pool == pool_name)?;
        let payload = serialize_payload(pool).ok()?;
        return Some(WebEvent {
            seq,
            timestamp,
            scope: scope.to_string(),
            kind: "pool.snapshot".to_string(),
            entity: pool.pool.clone(),
            summary: format!(
                "pool '{}': {} queued / {} running, {:.0}% utilized",
                pool.pool,
                pool.queued_jobs,
                pool.running_jobs,
                pool.utilization() * 100.0
            ),
            payload,
        });
    }
    if scope == "system.health" {
        let system = &state.tui.system;
        let payload = serialize_payload(system).ok()?;
        return Some(WebEvent {
            seq,
            timestamp,
            scope: scope.to_string(),
            kind: "system.snapshot".to_string(),
            entity: "system".to_string(),
            summary: "system component health snapshot".to_string(),
            payload,
        });
    }
    if let Some(workcell_id) = scope.strip_prefix("workcell.") {
        return workcells::snapshot_event(state, workcell_id);
    }
    if let Some(agent_run_id) = scope.strip_prefix("agent_run.") {
        return super::agent_runs::snapshot_event(state, agent_run_id);
    }
    None
}

async fn send_server_message(socket: &mut WebSocket, message: ServerWsMessage) -> Result<(), ()> {
    let encoded = serde_json::to_string(&message).map_err(|_| ())?;
    socket.send(Message::Text(encoded)).await.map_err(|_| ())
}

pub(super) fn hello_message(state: &WebState) -> ServerWsMessage {
    ServerWsMessage::Hello {
        server_time: server_time(),
        current_seq: state.ws.current_seq(),
        protocol: super::WS_PROTOCOL.to_string(),
    }
}
