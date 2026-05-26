//! Re-exports of the WS wire types defined in `crate::api::websocket`.
//!
//! The wire types (`ClientWsMessage`, `ServerWsMessage`, `WebEvent`,
//! `SubscriptionSpec`) physically live in `src/api/websocket.rs` per W-F-03,
//! because the API surface is single-sourced under `src/api/`. Call sites in
//! the WS handler and the bus can use either path; both refer to the same
//! types. Per WEB_WORK_CLAUDE.md §35.1.15 this module is the canonical entry
//! point for code under `web_events`.

pub use crate::api::websocket::{
    ClientWsMessage, ServerWsMessage, SubscriptionSpec, WebEvent,
};

/// Per-event priority class. Used by `WebEventBus` to decide what to drop
/// when the broadcast channel is saturated. `High` events MUST never be
/// dropped: action results, audit/security, direct mutation receipts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventPriority {
    High,
    Medium,
    Low,
}

impl EventPriority {
    /// Classify a `WebEvent.kind` string into a priority class.
    ///
    /// Known High kinds (never dropped): action results, audit/security
    /// events, direct mutation receipts. Known Low kinds (first to drop):
    /// log chunks, low-value activity, periodic health updates. Anything
    /// else is Medium.
    pub fn from_kind(kind: &str) -> Self {
        match kind {
            "action.executed"
            | "action.failed"
            | "audit.event.created"
            | "secret.audit.created"
            | "secret.access.denied"
            | "policy.violation"
            | "mr.approved"
            | "mr.merged"
            | "settings.changed"
            | "repo.settings.changed" => Self::High,
            "job.log.chunk"
            | "job.log.annotation"
            | "repo.activity"
            | "system.health"
            | "system.health.updated" => Self::Low,
            _ => Self::Medium,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_priority_kinds_classify_high() {
        for k in [
            "action.executed",
            "audit.event.created",
            "mr.approved",
            "mr.merged",
            "settings.changed",
            "repo.settings.changed",
        ] {
            assert_eq!(EventPriority::from_kind(k), EventPriority::High, "kind={k}");
        }
    }

    #[test]
    fn low_priority_kinds_classify_low() {
        for k in ["job.log.chunk", "repo.activity", "system.health"] {
            assert_eq!(EventPriority::from_kind(k), EventPriority::Low, "kind={k}");
        }
    }

    #[test]
    fn unknown_kind_defaults_to_medium() {
        assert_eq!(
            EventPriority::from_kind("nonsense.kind"),
            EventPriority::Medium
        );
        assert_eq!(
            EventPriority::from_kind("pipeline.created"),
            EventPriority::Medium
        );
    }
}
