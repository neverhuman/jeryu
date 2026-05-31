//! Per-transport payload shaping: turns an [`EscalationEvent`] into the JSON
//! body each webhook kind (Slack / PagerDuty / generic) expects.

use super::config::EscalationKind;
use super::event::EscalationEvent;

pub fn build_payload(event: &EscalationEvent, kind: EscalationKind) -> serde_json::Value {
    match kind {
        EscalationKind::Slack => serde_json::json!({ "text": event.summary() }),
        EscalationKind::PagerDuty => {
            let severity = match event {
                EscalationEvent::KillBellEngaged { .. } => "critical",
                EscalationEvent::RequireHuman { .. } => "warning",
            };
            serde_json::json!({
                "event_action": "trigger",
                "payload": {
                    "summary": event.summary(),
                    "source": "jeryu",
                    "severity": severity,
                    "custom_details": event.as_json(),
                },
            })
        }
        EscalationKind::GenericJson => serde_json::json!({
            "event_name": event.name(),
            "summary": event.summary(),
            "event": event.as_json(),
        }),
    }
}
