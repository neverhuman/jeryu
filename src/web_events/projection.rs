//! Convert TUI/control-plane events into web-shaped events.
//!
//! The wire-format `WebEvent` already has a `From<TuiEvent>` impl in
//! `src/api/websocket.rs` (W-F-03). This module exposes a borrowing variant
//! so callers (the future BFF dispatcher in W-B-04) can fan out a single
//! `TuiEvent` to multiple WS subscribers without cloning the event payload
//! ahead of time. The field map matches FINAL spec §6.4.

use crate::api::events::TuiEvent;

use super::protocol::WebEvent;

/// Project a `TuiEvent` into the wire-format `WebEvent`.
///
/// The `seq` field is left set to the value from the source `TuiEvent`; the
/// bus reassigns it on `publish` so callers can pass any value (typically
/// the source seq, for traceability in logs).
pub fn tui_event_to_web_event(event: &TuiEvent) -> WebEvent {
    let scope = format!("{}:{}", event.entity.kind.label(), event.entity.id);
    let kind = event.kind.label().to_string();
    WebEvent {
        seq: event.seq,
        timestamp: event.timestamp,
        scope,
        kind,
        entity: event.entity.display(),
        summary: event.summary.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::{EntityKind, EntityRef, Severity};
    use crate::api::events::TuiEventKind;
    use chrono::{TimeZone, Utc};

    fn sample_tui_event() -> TuiEvent {
        TuiEvent {
            seq: 11,
            timestamp: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            kind: TuiEventKind::JobStarted,
            severity: Severity::Info,
            entity: EntityRef::new(EntityKind::Job, "14445"),
            parent: None,
            summary: "job started".into(),
            correlation_id: Some("corr-1".into()),
            evidence_refs: vec!["capsule:1".into()],
            next_actions: vec![],
            stale_after_ms: 5_000,
        }
    }

    #[test]
    fn projection_preserves_scope_and_kind_labels() {
        let ev = sample_tui_event();
        let web = tui_event_to_web_event(&ev);
        assert_eq!(web.scope, "job:14445");
        assert_eq!(web.kind, "job.started");
        assert_eq!(web.entity, "job:14445");
        assert_eq!(web.summary, "job started");
        assert_eq!(web.seq, 11);
    }

    #[test]
    fn projection_payload_carries_severity_and_evidence() {
        let ev = sample_tui_event();
        let web = tui_event_to_web_event(&ev);
        assert_eq!(
            web.payload
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "info"
        );
        assert_eq!(
            web.payload
                .get("correlation_id")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "corr-1"
        );
        let evidence = web
            .payload
            .get("evidence_refs")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert_eq!(evidence.len(), 1);
    }
}
