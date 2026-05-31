//! Escalation surface — the "Needs You" transport.
//!
//! Invariants:
//!   - `RequireHuman` verdicts and `KillBellEngaged` events MUST be deliverable
//!     to one or more webhooks.
//!   - A failure (network or secret-missing) on one webhook NEVER aborts the
//!     others. Each `DispatchResult` records the outcome for one webhook.
//!   - This module never mutates global state; the caller decides whether to
//!     write a ledger entry for the dispatch attempt itself.
//!
//! The live HTTP transport is a thin [`crate::seam::EscalationSink`] the fused
//! product implements; this crate ships a recording sink for tests. The fan-out
//! logic, payload shaping, and `on_events` filtering are fully ported.
//!
//! The surface is split by responsibility: [`config`] (YAML schema), [`event`]
//! (the events), [`payload`] (per-transport shaping), and [`dispatch`]
//! (fan-out). Everything is re-exported here so the public paths are unchanged.

mod config;
mod dispatch;
mod event;
mod payload;

pub use config::{EscalationConfig, EscalationKind, WebhookConfig};
pub use dispatch::{DispatchResult, EscalationError, dispatch_all};
pub use event::EscalationEvent;
pub use payload::build_payload;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::Signature;
    use crate::types::{GateDecision, RiskTier, SchemaTag, VerdictReceiptRef, VibeGateVerdict};
    use async_trait::async_trait;
    use chrono::{TimeZone, Utc};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct RecordingSink {
        calls: Arc<Mutex<Vec<(WebhookConfig, serde_json::Value)>>>,
        outcomes: Vec<Result<u16, EscalationError>>,
        idx: Mutex<usize>,
    }

    impl RecordingSink {
        fn new() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                outcomes: Vec::new(),
                idx: Mutex::new(0),
            }
        }
        fn with_outcomes(outcomes: Vec<Result<u16, EscalationError>>) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                outcomes,
                idx: Mutex::new(0),
            }
        }
        fn calls(&self) -> Vec<(WebhookConfig, serde_json::Value)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl crate::seam::EscalationSink for RecordingSink {
        async fn deliver(
            &self,
            webhook: &WebhookConfig,
            payload: serde_json::Value,
        ) -> Result<u16, EscalationError> {
            self.calls
                .lock()
                .unwrap()
                .push((webhook.clone(), payload.clone()));
            let mut i = self.idx.lock().unwrap();
            let outcome = self.outcomes.get(*i).cloned();
            *i += 1;
            match outcome {
                Some(Ok(s)) => Ok(s),
                Some(Err(e)) => Err(e),
                None => Ok(200),
            }
        }
    }

    fn sample_verdict() -> VibeGateVerdict {
        VibeGateVerdict {
            schema: SchemaTag::new(),
            id: "vgv_01HXABCDEFGHJKMNPQRSTVWXYZ".into(),
            evidence_pack_id: "evp_01HXABCDEFGHJKMNPQRSTVWXYZ".into(),
            pull_request: Some("org/proj!42".into()),
            repo: "org/proj".into(),
            target_branch: "main".into(),
            head_sha: "abcdef1234567890abcdef1234567890abcdef12".into(),
            policy_sha: "c".repeat(40),
            evidence_pack_digest: format!("sha256:00{}", "0".repeat(62)),
            risk: RiskTier::R3,
            hard_stops: vec!["protected_path_touched".into()],
            required_reviews: vec![],
            approval_receipts: Vec::<VerdictReceiptRef>::new(),
            decision: GateDecision::RequireHuman,
            valid_for_head_sha_only: true,
            rebind_on_train: true,
            expires_at: Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 0).unwrap(),
            created_at: Utc.with_ymd_and_hms(2026, 5, 16, 0, 0, 0).unwrap(),
            signature: Signature::unsigned(),
        }
    }

    fn require_human_event() -> EscalationEvent {
        EscalationEvent::RequireHuman {
            verdict: Box::new(sample_verdict()),
        }
    }

    fn kill_bell_event() -> EscalationEvent {
        EscalationEvent::KillBellEngaged {
            reason: "operator pressed the bell".into(),
            paused_by: "alice@veox.ai".into(),
        }
    }

    fn all_webhooks() -> Vec<WebhookConfig> {
        vec![
            WebhookConfig {
                kind: EscalationKind::Slack,
                url_secret_name: "SLACK_WEBHOOK_URL".into(),
                channel: Some("#jeryu-needs-you".into()),
                severity: None,
                headers: HashMap::new(),
            },
            WebhookConfig {
                kind: EscalationKind::PagerDuty,
                url_secret_name: "PAGERDUTY_INTEGRATION_URL".into(),
                channel: None,
                severity: Some("warning".into()),
                headers: HashMap::new(),
            },
            WebhookConfig {
                kind: EscalationKind::GenericJson,
                url_secret_name: "ESCALATION_WEBHOOK_URL".into(),
                channel: None,
                severity: None,
                headers: HashMap::from([("X-Source".into(), "jeryu".into())]),
            },
        ]
    }

    #[test]
    fn parse_minimal_yaml_round_trips() {
        let yaml = r##"
enabled: true
on_events: [require_human, kill_bell_engaged]
webhooks:
  - kind: slack
    url_secret_name: SLACK_WEBHOOK_URL
    channel: "#jeryu-needs-you"
  - kind: pagerduty
    url_secret_name: PAGERDUTY_INTEGRATION_URL
    severity: warning
  - kind: generic_json
    url_secret_name: ESCALATION_WEBHOOK_URL
    headers:
      X-Source: jeryu
"##;
        let cfg: EscalationConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.on_events, vec!["require_human", "kill_bell_engaged"]);
        assert_eq!(cfg.webhooks.len(), 3);
        assert_eq!(cfg.webhooks[0].kind, EscalationKind::Slack);
        assert_eq!(cfg.webhooks[1].kind, EscalationKind::PagerDuty);
        assert_eq!(cfg.webhooks[2].kind, EscalationKind::GenericJson);
        let yaml2 = serde_yaml::to_string(&cfg).unwrap();
        let cfg2: EscalationConfig = serde_yaml::from_str(&yaml2).unwrap();
        assert_eq!(cfg, cfg2);
    }

    #[test]
    fn parse_empty_escalation_disables_it() {
        let yaml = "enabled: true\nwebhooks: []\n";
        let cfg: EscalationConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.enabled);
        assert!(cfg.on_events.is_empty());
        assert!(!cfg.permits("require_human"));
    }

    #[test]
    fn build_slack_payload_uses_text_field() {
        let payload = build_payload(&require_human_event(), EscalationKind::Slack);
        let text = payload.get("text").and_then(|v| v.as_str()).expect("text");
        assert!(text.contains("RequireHuman"), "got: {text}");
        assert!(text.contains("org/proj"), "got: {text}");
    }

    #[test]
    fn build_pagerduty_payload_uses_event_action_trigger() {
        let payload = build_payload(&kill_bell_event(), EscalationKind::PagerDuty);
        assert_eq!(payload["event_action"], "trigger");
        assert_eq!(payload["payload"]["source"], "jeryu");
        assert_eq!(payload["payload"]["severity"], "critical");
        assert_eq!(
            payload["payload"]["custom_details"]["event"],
            "kill_bell_engaged"
        );
    }

    #[test]
    fn build_generic_payload_includes_full_event_json() {
        let payload = build_payload(&require_human_event(), EscalationKind::GenericJson);
        assert_eq!(payload["event_name"], "require_human");
        let verdict = &payload["event"]["verdict"];
        assert_eq!(verdict["id"], "vgv_01HXABCDEFGHJKMNPQRSTVWXYZ");
        assert_eq!(verdict["decision"], "require_human");
        assert_eq!(verdict["risk"], "R3");
        // D4 wire field.
        assert_eq!(verdict["pull_request"], "org/proj!42");
    }

    #[tokio::test]
    async fn dispatch_all_filters_by_on_events() {
        let cfg = EscalationConfig {
            enabled: true,
            on_events: vec!["require_human".into()],
            webhooks: all_webhooks(),
        };
        let sink = RecordingSink::new();
        let results = dispatch_all(&cfg, &kill_bell_event(), &sink).await;
        assert!(results.is_empty(), "kill_bell_engaged is not allowlisted");
        assert!(sink.calls().is_empty());
    }

    #[tokio::test]
    async fn dispatch_all_fans_out_to_all_webhooks_for_matching_event() {
        let cfg = EscalationConfig {
            enabled: true,
            on_events: vec!["require_human".into(), "kill_bell_engaged".into()],
            webhooks: all_webhooks(),
        };
        let sink = RecordingSink::new();
        let results = dispatch_all(&cfg, &require_human_event(), &sink).await;
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.error.is_none()));
        assert!(results.iter().all(|r| r.status == Some(200)));
        let calls = sink.calls();
        assert_eq!(calls.len(), 3);
        assert!(calls[0].1.get("text").is_some(), "slack payload");
        assert_eq!(calls[1].1["event_action"], "trigger");
        assert_eq!(calls[2].1["event_name"], "require_human");
    }

    #[tokio::test]
    async fn dispatch_all_continues_on_individual_webhook_failure() {
        let cfg = EscalationConfig {
            enabled: true,
            on_events: vec!["require_human".into()],
            webhooks: all_webhooks(),
        };
        let sink = RecordingSink::with_outcomes(vec![
            Err(EscalationError::Transport("connection refused".into())),
            Ok(202),
            Ok(200),
        ]);
        let results = dispatch_all(&cfg, &require_human_event(), &sink).await;
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].webhook_kind, EscalationKind::Slack);
        assert!(results[0].error.as_deref().unwrap().contains("transport"));
        assert_eq!(results[0].status, None);
        assert_eq!(results[1].status, Some(202));
        assert!(results[1].error.is_none());
        assert_eq!(results[2].status, Some(200));
        // Critical invariant: webhook[1] and [2] were called even though [0] failed.
        assert_eq!(sink.calls().len(), 3);
    }

    #[tokio::test]
    async fn secret_resolution_failure_surfaces_as_dispatch_result_error() {
        let cfg = EscalationConfig {
            enabled: true,
            on_events: vec!["require_human".into()],
            webhooks: vec![WebhookConfig {
                kind: EscalationKind::Slack,
                url_secret_name: "DEFINITELY_NOT_SET_4F2B".into(),
                channel: None,
                severity: None,
                headers: HashMap::new(),
            }],
        };
        let sink = RecordingSink::with_outcomes(vec![Err(EscalationError::SecretMissing(
            "DEFINITELY_NOT_SET_4F2B".into(),
        ))]);
        let results = dispatch_all(&cfg, &require_human_event(), &sink).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, None);
        assert_eq!(
            results[0].error.as_deref(),
            Some("secret not resolvable: DEFINITELY_NOT_SET_4F2B")
        );
    }

    #[tokio::test]
    async fn disabled_config_returns_empty_dispatch_results() {
        let cfg = EscalationConfig {
            enabled: false,
            on_events: vec!["require_human".into()],
            webhooks: all_webhooks(),
        };
        let sink = RecordingSink::new();
        let results = dispatch_all(&cfg, &require_human_event(), &sink).await;
        assert!(results.is_empty());
        assert!(sink.calls().is_empty());
    }

    #[test]
    fn event_names_match_yaml_allowlist_strings() {
        assert_eq!(require_human_event().name(), "require_human");
        assert_eq!(kill_bell_event().name(), "kill_bell_engaged");
    }

    #[test]
    fn slack_payload_escapes_special_chars_via_serde_json() {
        let event = EscalationEvent::KillBellEngaged {
            reason: "broken \"prod\" \\ and a\nnewline".into(),
            paused_by: "ops".into(),
        };
        let payload = build_payload(&event, EscalationKind::Slack);
        let json = serde_json::to_string(&payload).expect("serialize");
        let _: serde_json::Value = serde_json::from_str(&json).expect("round-trips");
        assert!(json.contains("\\\""));
        assert!(json.contains("\\n"));
    }

    #[test]
    fn webhook_config_with_empty_headers_serializes_compactly() {
        let wh = WebhookConfig {
            kind: EscalationKind::Slack,
            url_secret_name: "FOO".into(),
            channel: None,
            severity: None,
            headers: HashMap::new(),
        };
        let yaml = serde_yaml::to_string(&wh).unwrap();
        assert!(!yaml.contains("headers"));
        assert!(!yaml.contains("channel"));
        assert!(!yaml.contains("severity"));
    }
}
