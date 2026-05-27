//! Owner: TUI Control-Plane API + Web Forge BFF — WebSocket wire types
//! Proof: `cargo nextest run -p jeryu --lib api`
//! Invariants: API contract types only; no I/O, no domain side effects.
//!
//! Source: FINAL spec §6.4. W-F-06 owns `src/web_events/protocol.rs`;
//! we house the wire types here in `src/api/websocket.rs` for two reasons:
//!
//! 1. Per WEB_WORK_CLAUDE.md §35 the API surface lives in `src/api/`.
//! 2. W-F-06 has not yet shipped, so W-F-03 makes these types available now
//!    and W-F-06 will re-export from `crate::web_events::protocol`.
//!
//! Callers should `use crate::api::websocket::{ClientWsMessage, ...}`. When
//! W-F-06 lands, both module paths resolve to the same types via re-export.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::events::TuiEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub enum ClientWsMessage {
    Hello {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resume_from: Option<u64>,
        subscriptions: Vec<SubscriptionSpec>,
    },
    Subscribe {
        subscriptions: Vec<SubscriptionSpec>,
    },
    Unsubscribe {
        scopes: Vec<String>,
    },
    Ack {
        seq: u64,
    },
    Ping {
        nonce: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct SubscriptionSpec {
    /// Granular topic per §35.1.15, e.g. `global.activity`, `repo.{id}`,
    /// `mr.{id}`, `agent.{id}`, `cache.{id}`. Backend re-checks each scope
    /// against the viewer's perms on every `Subscribe` frame (§35.1.6).
    pub scope: String,
    /// Free-form filter object (e.g. `{ kind: ["mr.approved"] }`).
    #[ts(type = "Record<string, unknown>")]
    #[schemars(with = "serde_json::Value")]
    #[schema(value_type = serde_json::Value)]
    pub filters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub enum ServerWsMessage {
    Hello {
        server_time: DateTime<Utc>,
        current_seq: u64,
        protocol: String,
    },
    SnapshotRequired {
        reason: String,
        current_seq: u64,
    },
    Event {
        event: WebEvent,
    },
    Pong {
        nonce: String,
        server_time: DateTime<Utc>,
    },
    Error {
        code: String,
        message: String,
    },
}

impl ServerWsMessage {
    /// Construct a `Hello` frame stamping current server time. The `protocol`
    /// string `jeryu.ws.v1` is the W-F-06 wire identity.
    pub fn hello(current_seq: u64) -> Self {
        Self::Hello {
            server_time: Utc::now(),
            current_seq,
            protocol: "jeryu.ws.v1".to_string(),
        }
    }

    pub fn event(event: WebEvent) -> Self {
        Self::Event { event }
    }

    /// Serialize to JSON. Panics on serialization failure — `WebEvent` and the
    /// frame envelopes are simple owned types and cannot fail to serialize.
    pub fn unwrap_json(&self) -> String {
        serde_json::to_string(self).expect("serialize ws message")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct WebEvent {
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
    pub scope: String,
    pub kind: String,
    pub entity: String,
    pub summary: String,
    /// Free-form payload carrying severity, parent, evidence refs, etc.
    #[ts(type = "Record<string, unknown>")]
    #[schemars(with = "serde_json::Value")]
    #[schema(value_type = serde_json::Value)]
    pub payload: Value,
}

impl From<TuiEvent> for WebEvent {
    /// Map an internal `TuiEvent` to the wire-format `WebEvent`. The scope is
    /// derived from the entity (`{kind}:{id}`); other context lands in the
    /// JSON payload. Per §35.1.15 the BFF replaces this with a richer
    /// scope-vocabulary projection in W-F-06; this `From` impl is the seed.
    fn from(event: TuiEvent) -> Self {
        let scope = format!("{}:{}", event.entity.kind.label(), event.entity.id);
        let kind = event.kind.label().to_string();
        Self {
            seq: event.seq,
            timestamp: event.timestamp,
            scope,
            kind,
            entity: event.entity.display(),
            summary: event.summary,
            payload: serde_json::json!({
                "severity": event.severity,
                "parent": event.parent,
                "correlation_id": event.correlation_id,
                "evidence_refs": event.evidence_refs,
                "next_actions": event.next_actions,
                "stale_after_ms": event.stale_after_ms,
            }),
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::{EntityKind, EntityRef, Severity};
    use crate::api::events::{TuiEvent, TuiEventKind};

    #[test]
    fn client_ws_message_hello_roundtrips() {
        let m = ClientWsMessage::Hello {
            resume_from: Some(42),
            subscriptions: vec![SubscriptionSpec {
                scope: "global.activity".into(),
                filters: serde_json::json!({}),
            }],
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"type\":\"hello\""));
        assert!(json.contains("\"resume_from\":42"));
        let back: ClientWsMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientWsMessage::Hello {
                resume_from,
                subscriptions,
            } => {
                assert_eq!(resume_from, Some(42));
                assert_eq!(subscriptions.len(), 1);
                assert_eq!(subscriptions[0].scope, "global.activity");
            }
            _ => panic!("expected Hello"),
        }
    }

    #[test]
    fn client_ws_message_subscribe_unsubscribe_ack_ping_roundtrip() {
        for m in [
            ClientWsMessage::Subscribe {
                subscriptions: vec![SubscriptionSpec {
                    scope: "repo.r1".into(),
                    filters: serde_json::json!({}),
                }],
            },
            ClientWsMessage::Unsubscribe {
                scopes: vec!["repo.r1".into()],
            },
            ClientWsMessage::Ack { seq: 7 },
            ClientWsMessage::Ping {
                nonce: "n-1".into(),
            },
        ] {
            let json = serde_json::to_string(&m).unwrap();
            let _: ClientWsMessage = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn server_ws_message_hello_protocol_is_jeryu_ws_v1() {
        let m = ServerWsMessage::hello(10);
        let json = m.unwrap_json();
        assert!(json.contains("\"protocol\":\"jeryu.ws.v1\""));
        assert!(json.contains("\"current_seq\":10"));
        let back: ServerWsMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerWsMessage::Hello {
                protocol,
                current_seq,
                ..
            } => {
                assert_eq!(protocol, "jeryu.ws.v1");
                assert_eq!(current_seq, 10);
            }
            _ => panic!("expected Hello"),
        }
    }

    #[test]
    fn server_ws_message_snapshot_required_and_error_roundtrip() {
        for m in [
            ServerWsMessage::SnapshotRequired {
                reason: "lag".into(),
                current_seq: 100,
            },
            ServerWsMessage::Error {
                code: "subscribe_forbidden".into(),
                message: "no access to repo.r1".into(),
            },
        ] {
            let json = serde_json::to_string(&m).unwrap();
            let _: ServerWsMessage = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn web_event_from_tui_event_uses_entity_scope() {
        let ev = TuiEvent {
            seq: 5,
            timestamp: chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            kind: TuiEventKind::JobStarted,
            severity: Severity::Info,
            entity: EntityRef::new(EntityKind::Job, "14445"),
            parent: None,
            summary: "job started".into(),
            correlation_id: None,
            evidence_refs: vec![],
            next_actions: vec![],
            stale_after_ms: 5_000,
        };
        let web: WebEvent = ev.into();
        assert_eq!(web.scope, "job:14445");
        assert_eq!(web.kind, "job.started");
        assert_eq!(web.seq, 5);
        assert!(web.payload.get("severity").is_some());
    }

    #[test]
    fn server_ws_message_event_carries_web_event() {
        let event = WebEvent {
            seq: 1,
            timestamp: chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            scope: "repo:abc".into(),
            kind: "repo.created".into(),
            entity: "repo:abc".into(),
            summary: "created".into(),
            payload: serde_json::json!({}),
        };
        let m = ServerWsMessage::event(event);
        let json = m.unwrap_json();
        let back: ServerWsMessage = serde_json::from_str(&json).unwrap();
        if let ServerWsMessage::Event { event } = back {
            assert_eq!(event.kind, "repo.created");
        } else {
            panic!("expected Event variant");
        }
    }
}
