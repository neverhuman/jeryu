//! Owner: Evidence Gate / escalation surface (Wave 5)
//! Proof: `cargo nextest run -p jeryu -- autonomy::escalation`
//! Invariants:
//!   - `RequireHuman` verdicts and `KillBellEngaged` ledger events MUST be
//!     deliverable to one or more webhooks. Without this, "Needs You" only
//!     surfaces when a human polls the TUI (tip1 Law 9).
//!   - Webhook URLs are resolved through the same canonical secret chain as
//!     LLM provider keys (`src/llm/secrets.rs`); URLs never appear inline in
//!     `.jeryu/autonomy/autonomy.yml`.
//!   - A failure (network or secret-missing) on one webhook NEVER aborts the
//!     others. Each `DispatchResult` records the outcome for one webhook.
//!   - This module never mutates global state; the caller decides whether to
//!     write a `LaunchLedgerEntry` for the dispatch attempt itself.
//!
//! The real `ReqwestDispatcher` performs live HTTP and is left untested in
//! this slice (no test server is wired up here). The dispatch fan-out logic,
//! payload shaping, and `on_events` filtering ARE covered via `FakeDispatcher`.

use crate::autonomy::types::VibeGateVerdict;
use crate::llm::secrets::{SecretResolver, resolve_secret};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Schema (deserialized from `.jeryu/autonomy/autonomy.yml::escalation`)
// ---------------------------------------------------------------------------

/// Supported webhook integrations. `generic_json` is the escape hatch for
/// anything that just wants the event verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationKind {
    Slack,
    /// Deserializes from both `pagerduty` (canonical YAML spelling) and
    /// `pager_duty` (accepted snake_case spelling). Serializes back as `pagerduty`.
    #[serde(rename = "pagerduty", alias = "pager_duty")]
    PagerDuty,
    GenericJson,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WebhookConfig {
    pub kind: EscalationKind,
    /// Name of the env / secret variable holding the actual webhook URL.
    /// Resolved at dispatch time through the 6-tier chain in `llm::secrets`.
    pub url_secret_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct EscalationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub on_events: Vec<String>,
    #[serde(default)]
    pub webhooks: Vec<WebhookConfig>,
}

impl EscalationConfig {
    /// True if this event name is in the `on_events` allowlist AND the config
    /// is enabled. An empty `on_events` means "nothing fires" (fail-closed).
    pub fn permits(&self, event_name: &str) -> bool {
        self.enabled && self.on_events.iter().any(|e| e == event_name)
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// A single escalation moment. New variants must add a matching string in
/// `EscalationEvent::name()` and document it in `.jeryu/autonomy/autonomy.yml`.
#[derive(Debug, Clone)]
pub enum EscalationEvent {
    RequireHuman { verdict: Box<VibeGateVerdict> },
    KillBellEngaged { reason: String, paused_by: String },
}

impl EscalationEvent {
    /// Stable string used in `on_events` allowlists. Must match
    /// `.jeryu/autonomy/autonomy.yml::escalation.on_events` entries.
    pub fn name(&self) -> &'static str {
        match self {
            EscalationEvent::RequireHuman { .. } => "require_human",
            EscalationEvent::KillBellEngaged { .. } => "kill_bell_engaged",
        }
    }

    /// Short human-readable summary used in webhook message bodies.
    pub fn summary(&self) -> String {
        match self {
            EscalationEvent::RequireHuman { verdict } => format!(
                "[jeryu] RequireHuman on {repo} @ {head} (risk={risk:?}, verdict={vid})",
                repo = verdict.repo,
                head = short_sha(&verdict.head_sha),
                risk = verdict.risk,
                vid = verdict.id,
            ),
            EscalationEvent::KillBellEngaged { reason, paused_by } => {
                format!("[jeryu] KillBellEngaged by {paused_by}: {reason}")
            }
        }
    }

    /// Self-describing JSON form (used by generic_json webhooks and as
    /// PagerDuty's `payload.custom_details`).
    pub fn as_json(&self) -> serde_json::Value {
        match self {
            EscalationEvent::RequireHuman { verdict } => serde_json::json!({
                "event": "require_human",
                "verdict": verdict,
            }),
            EscalationEvent::KillBellEngaged { reason, paused_by } => serde_json::json!({
                "event": "kill_bell_engaged",
                "reason": reason,
                "paused_by": paused_by,
            }),
        }
    }
}

fn short_sha(sha: &str) -> &str {
    if sha.len() >= 7 { &sha[..7] } else { sha }
}

// ---------------------------------------------------------------------------
// Payload shaping
// ---------------------------------------------------------------------------

/// Format `event` for the wire format `kind` expects. Each integration has
/// its own envelope:
///
///   - Slack: `{"text": "..."}` (incoming-webhook minimum).
///   - PagerDuty Events API v2: `{"event_action":"trigger","payload":{...}}`.
///   - generic_json: `{"event_name": "...", "summary": "...", "event": <full>}`.
pub fn build_payload(event: &EscalationEvent, kind: EscalationKind) -> serde_json::Value {
    match kind {
        EscalationKind::Slack => serde_json::json!({
            "text": event.summary(),
        }),
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

// ---------------------------------------------------------------------------
// Dispatcher trait + real reqwest impl
// ---------------------------------------------------------------------------

/// Per-webhook dispatch outcome. Aggregated by `dispatch_all`.
#[derive(Debug, Clone, PartialEq)]
pub struct DispatchResult {
    pub webhook_kind: EscalationKind,
    /// HTTP status code if a request actually fired. `None` on
    /// secret-resolution failure or transport-level error.
    pub status: Option<u16>,
    /// Human-readable error string. `None` on success (2xx status).
    pub error: Option<String>,
}

impl DispatchResult {
    pub fn ok(kind: EscalationKind, status: u16) -> Self {
        Self {
            webhook_kind: kind,
            status: Some(status),
            error: None,
        }
    }
    pub fn err(kind: EscalationKind, status: Option<u16>, error: impl Into<String>) -> Self {
        Self {
            webhook_kind: kind,
            status,
            error: Some(error.into()),
        }
    }
}

/// Abstraction over "POST a JSON body to the URL that lives behind this
/// webhook's secret name". Real implementation in `ReqwestDispatcher`; tests
/// use `FakeDispatcher`.
#[async_trait]
pub trait EscalationDispatcher: Send + Sync {
    /// Returns the HTTP status code on success. Implementations are
    /// responsible for resolving the webhook URL via their own secret chain
    /// and applying any kind-specific headers.
    async fn post(
        &self,
        webhook: &WebhookConfig,
        payload: serde_json::Value,
    ) -> Result<u16, EscalationError>;
}

/// Minimal error type so we can distinguish "couldn't even start" from
/// "started but non-2xx". Stays local to this module — the public surface
/// is `DispatchResult`.
#[derive(Debug, Clone, PartialEq)]
pub enum EscalationError {
    SecretMissing(String),
    Transport(String),
    HttpStatus { code: u16, body: String },
}

impl std::fmt::Display for EscalationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EscalationError::SecretMissing(name) => write!(f, "secret not resolvable: {name}"),
            EscalationError::Transport(s) => write!(f, "transport error: {s}"),
            EscalationError::HttpStatus { code, body } => {
                write!(f, "non-2xx status {code}: {body}")
            }
        }
    }
}

impl std::error::Error for EscalationError {}

/// Production dispatcher backed by `reqwest`. Uses the same client
/// construction pattern as `src/llm/openai_compatible.rs`.
///
/// Live HTTP is intentionally not exercised in unit tests; see module
/// docstring. Wire it up in the orchestrator entry point.
pub struct ReqwestDispatcher {
    pub client: reqwest::Client,
    pub secret_resolver: Arc<SecretResolver>,
}

impl ReqwestDispatcher {
    pub fn new(secret_resolver: Arc<SecretResolver>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("jeryu-evidence-gate/0.1")
                .build()
                .expect("reqwest client build"),
            secret_resolver,
        }
    }
}

#[async_trait]
impl EscalationDispatcher for ReqwestDispatcher {
    async fn post(
        &self,
        webhook: &WebhookConfig,
        payload: serde_json::Value,
    ) -> Result<u16, EscalationError> {
        let url = resolve_secret(&webhook.url_secret_name, &self.secret_resolver)
            .ok_or_else(|| EscalationError::SecretMissing(webhook.url_secret_name.clone()))?
            .value;

        let mut req = self.client.post(&url).json(&payload);
        for (k, v) in &webhook.headers {
            req = req.header(k, v);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| EscalationError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            let body = match resp.text().await {
                Ok(body) => body,
                Err(_) => String::new(),
            };
            return Err(EscalationError::HttpStatus { code: status, body });
        }
        Ok(status)
    }
}

// ---------------------------------------------------------------------------
// Fan-out
// ---------------------------------------------------------------------------

/// Fan out the event to every configured webhook whose `on_events` allowlist
/// matches. Failures in one webhook do not abort the others.
pub async fn dispatch_all(
    config: &EscalationConfig,
    event: &EscalationEvent,
    dispatcher: &dyn EscalationDispatcher,
) -> Vec<DispatchResult> {
    if !config.permits(event.name()) {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(config.webhooks.len());
    for wh in &config.webhooks {
        let payload = build_payload(event, wh.kind);
        let res = dispatcher.post(wh, payload).await;
        let result = match res {
            Ok(status) => DispatchResult::ok(wh.kind, status),
            Err(EscalationError::SecretMissing(name)) => {
                DispatchResult::err(wh.kind, None, format!("secret not resolvable: {name}"))
            }
            Err(EscalationError::Transport(msg)) => {
                DispatchResult::err(wh.kind, None, format!("transport error: {msg}"))
            }
            Err(EscalationError::HttpStatus { code, body }) => {
                DispatchResult::err(wh.kind, Some(code), format!("http {code}: {body}"))
            }
        };
        out.push(result);
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::signing::Signature;
    use crate::autonomy::types::{
        GateDecision, RiskTier, SchemaTag, VerdictReceiptRef, VibeGateVerdict,
    };
    use chrono::{TimeZone, Utc};
    use std::sync::Mutex;
    use tokio::runtime::Runtime;

    // -- Fakes ---------------------------------------------------------

    struct FakeDispatcher {
        calls: Arc<Mutex<Vec<(WebhookConfig, serde_json::Value)>>>,
        /// Per-call outcome by index; missing -> Ok(200).
        outcomes: Vec<Result<u16, EscalationError>>,
        idx: Mutex<usize>,
    }

    impl FakeDispatcher {
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
    impl EscalationDispatcher for FakeDispatcher {
        async fn post(
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

    // -- Helpers -------------------------------------------------------

    fn rt() -> Runtime {
        Runtime::new().unwrap()
    }

    fn sample_verdict() -> VibeGateVerdict {
        VibeGateVerdict {
            schema: SchemaTag::new(),
            id: "vgv_01HXABCDEFGHJKMNPQRSTVWXYZ".into(),
            evidence_pack_id: "evp_01HXABCDEFGHJKMNPQRSTVWXYZ".into(),
            merge_request: Some("org/proj!42".into()),
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
            signature: Signature::stub(),
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

    // -- YAML round-trip ----------------------------------------------

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
        assert_eq!(cfg.webhooks[0].channel.as_deref(), Some("#jeryu-needs-you"));
        assert_eq!(cfg.webhooks[1].kind, EscalationKind::PagerDuty);
        assert_eq!(cfg.webhooks[1].severity.as_deref(), Some("warning"));
        assert_eq!(cfg.webhooks[2].kind, EscalationKind::GenericJson);
        assert_eq!(
            cfg.webhooks[2].headers.get("X-Source"),
            Some(&"jeryu".to_string())
        );

        // Re-serialize and parse again — losslessly.
        let yaml2 = serde_yaml::to_string(&cfg).unwrap();
        let cfg2: EscalationConfig = serde_yaml::from_str(&yaml2).unwrap();
        assert_eq!(cfg, cfg2);
    }

    #[test]
    fn parse_empty_escalation_disables_it() {
        // The Wave 0 default in `.jeryu/autonomy/autonomy.yml` was just `enabled: true`
        // with an empty webhook list. That MUST parse and MUST result in
        // dispatch_all returning empty (no webhooks = nothing to send).
        let yaml = "enabled: true\nwebhooks: []\n";
        let cfg: EscalationConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.enabled);
        assert!(cfg.on_events.is_empty());
        assert!(cfg.webhooks.is_empty());
        // permits() is false because on_events is empty (fail-closed).
        assert!(!cfg.permits("require_human"));
    }

    // -- Payload shaping ----------------------------------------------

    #[test]
    fn build_slack_payload_uses_text_field() {
        let payload = build_payload(&require_human_event(), EscalationKind::Slack);
        let obj = payload.as_object().expect("object");
        assert_eq!(obj.len(), 1);
        let text = obj.get("text").and_then(|v| v.as_str()).expect("text");
        assert!(text.contains("RequireHuman"), "got: {text}");
        assert!(text.contains("org/proj"), "got: {text}");
    }

    #[test]
    fn build_pagerduty_payload_uses_event_action_trigger() {
        let payload = build_payload(&kill_bell_event(), EscalationKind::PagerDuty);
        assert_eq!(payload["event_action"], "trigger");
        assert_eq!(payload["payload"]["source"], "jeryu");
        // KillBellEngaged is critical; RequireHuman is warning.
        assert_eq!(payload["payload"]["severity"], "critical");
        let summary = payload["payload"]["summary"]
            .as_str()
            .expect("summary string");
        assert!(summary.contains("KillBellEngaged"));
        assert!(summary.contains("alice@veox.ai"));
        // custom_details carries the full structured event.
        assert_eq!(
            payload["payload"]["custom_details"]["event"],
            "kill_bell_engaged"
        );
    }

    #[test]
    fn build_generic_payload_includes_full_event_json() {
        let payload = build_payload(&require_human_event(), EscalationKind::GenericJson);
        assert_eq!(payload["event_name"], "require_human");
        // Full verdict embedded under `event.verdict`.
        let verdict = &payload["event"]["verdict"];
        assert_eq!(verdict["id"], "vgv_01HXABCDEFGHJKMNPQRSTVWXYZ");
        assert_eq!(verdict["decision"], "require_human");
        assert_eq!(verdict["risk"], "R3");
        assert!(
            payload["summary"]
                .as_str()
                .unwrap()
                .contains("RequireHuman")
        );
    }

    // -- Dispatch fan-out ---------------------------------------------

    #[test]
    fn dispatch_all_filters_by_on_events() {
        let cfg = EscalationConfig {
            enabled: true,
            on_events: vec!["require_human".into()],
            webhooks: all_webhooks(),
        };
        let fake = FakeDispatcher::new();
        let results = rt().block_on(dispatch_all(&cfg, &kill_bell_event(), &fake));
        assert!(results.is_empty(), "kill_bell_engaged is not allowlisted");
        assert!(fake.calls().is_empty(), "no POST should fire");
    }

    #[test]
    fn dispatch_all_fans_out_to_all_webhooks_for_matching_event() {
        let cfg = EscalationConfig {
            enabled: true,
            on_events: vec!["require_human".into(), "kill_bell_engaged".into()],
            webhooks: all_webhooks(),
        };
        let fake = FakeDispatcher::new();
        let results = rt().block_on(dispatch_all(&cfg, &require_human_event(), &fake));
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.error.is_none()));
        assert!(results.iter().all(|r| r.status == Some(200)));
        let calls = fake.calls();
        assert_eq!(calls.len(), 3);
        // Each webhook got its kind-specific envelope.
        assert!(calls[0].1.get("text").is_some(), "slack payload");
        assert_eq!(calls[1].1["event_action"], "trigger");
        assert_eq!(calls[2].1["event_name"], "require_human");
    }

    #[test]
    fn dispatch_all_continues_on_individual_webhook_failure() {
        let cfg = EscalationConfig {
            enabled: true,
            on_events: vec!["require_human".into()],
            webhooks: all_webhooks(),
        };
        let fake = FakeDispatcher::with_outcomes(vec![
            Err(EscalationError::Transport("connection refused".into())),
            Ok(202),
            Ok(200),
        ]);
        let results = rt().block_on(dispatch_all(&cfg, &require_human_event(), &fake));
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].webhook_kind, EscalationKind::Slack);
        assert!(
            results[0].error.as_deref().unwrap().contains("transport"),
            "got: {:?}",
            results[0].error
        );
        assert_eq!(results[0].status, None);
        assert_eq!(results[1].webhook_kind, EscalationKind::PagerDuty);
        assert_eq!(results[1].status, Some(202));
        assert!(results[1].error.is_none());
        assert_eq!(results[2].webhook_kind, EscalationKind::GenericJson);
        assert_eq!(results[2].status, Some(200));
        // Critical invariant: webhook[1] and webhook[2] were called even
        // though webhook[0] failed.
        assert_eq!(fake.calls().len(), 3);
    }

    #[test]
    fn secret_resolution_failure_surfaces_as_dispatch_result_error() {
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
        let fake = FakeDispatcher::with_outcomes(vec![Err(EscalationError::SecretMissing(
            "DEFINITELY_NOT_SET_4F2B".into(),
        ))]);
        let results = rt().block_on(dispatch_all(&cfg, &require_human_event(), &fake));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, None);
        assert_eq!(
            results[0].error.as_deref(),
            Some("secret not resolvable: DEFINITELY_NOT_SET_4F2B")
        );
    }

    #[test]
    fn disabled_config_returns_empty_dispatch_results() {
        let cfg = EscalationConfig {
            enabled: false,
            on_events: vec!["require_human".into(), "kill_bell_engaged".into()],
            webhooks: all_webhooks(),
        };
        let fake = FakeDispatcher::new();
        let results = rt().block_on(dispatch_all(&cfg, &require_human_event(), &fake));
        assert!(results.is_empty());
        assert!(fake.calls().is_empty());
    }

    // -- Bonus coverage -----------------------------------------------

    #[test]
    fn event_names_match_yaml_allowlist_strings() {
        // Guards the contract between `.jeryu/autonomy/autonomy.yml::on_events`
        // and `EscalationEvent::name()`. If a future variant lands without
        // updating both sides, this test breaks loudly.
        assert_eq!(require_human_event().name(), "require_human");
        assert_eq!(kill_bell_event().name(), "kill_bell_engaged");
    }

    // --- Wave 5 coverage-boost additions -----------------------------------

    /// Slack payloads route through `escape_label`-style logic in the
    /// renderer downstream, but the `build_payload` step embeds the summary
    /// verbatim. Special characters like double quotes, backslashes, and
    /// newlines must survive JSON serialization without corrupting the
    /// payload — they are escaped by serde_json automatically.
    #[test]
    fn slack_payload_escapes_special_chars_via_serde_json() {
        let event = EscalationEvent::KillBellEngaged {
            reason: "broken \"prod\" \\ and a\nnewline".into(),
            paused_by: "ops".into(),
        };
        let payload = build_payload(&event, EscalationKind::Slack);
        let text = payload.get("text").and_then(|v| v.as_str()).expect("text");
        assert!(text.contains("broken"), "payload preserves the reason");
        assert!(text.contains("\\"), "raw backslash preserved");
        // Serialize the whole payload to JSON and confirm it parses back —
        // i.e. the special chars are escaped, not raw-injected.
        let json = serde_json::to_string(&payload).expect("serialize");
        let _: serde_json::Value =
            serde_json::from_str(&json).expect("round-trips through JSON cleanly");
        // The JSON form must contain escape sequences, not literal quotes
        // breaking out of the string.
        assert!(
            json.contains("\\\""),
            "double quote should be JSON-escaped; got: {json}"
        );
        assert!(
            json.contains("\\n"),
            "newline should be JSON-escaped; got: {json}"
        );
    }

    /// A `WebhookConfig` with an empty headers map must still fan-out
    /// successfully (no panic, no missing-header crash) — the dispatcher
    /// iterates the map and an empty map is a no-op.
    #[test]
    fn dispatch_with_empty_headers_completes_successfully() {
        let cfg = EscalationConfig {
            enabled: true,
            on_events: vec!["require_human".into()],
            webhooks: vec![WebhookConfig {
                kind: EscalationKind::GenericJson,
                url_secret_name: "X_WEBHOOK".into(),
                channel: None,
                severity: None,
                headers: HashMap::new(),
            }],
        };
        let fake = FakeDispatcher::new();
        let results = rt().block_on(dispatch_all(&cfg, &require_human_event(), &fake));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, Some(200));
        assert!(results[0].error.is_none());
        let calls = fake.calls();
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].0.headers.is_empty(),
            "empty headers must be passed through verbatim"
        );
    }

    /// Both event variants (`require_human`, `kill_bell_engaged`) must be
    /// dispatchable when both names appear in `on_events`. This is the
    /// "happy path" cross-product check that the allowlist string match
    /// is performed correctly for every variant.
    #[test]
    fn on_events_with_both_event_names_dispatches_each() {
        let cfg = EscalationConfig {
            enabled: true,
            on_events: vec!["require_human".into(), "kill_bell_engaged".into()],
            webhooks: vec![WebhookConfig {
                kind: EscalationKind::GenericJson,
                url_secret_name: "X".into(),
                channel: None,
                severity: None,
                headers: HashMap::new(),
            }],
        };
        let fake = FakeDispatcher::new();
        let r1 = rt().block_on(dispatch_all(&cfg, &require_human_event(), &fake));
        let r2 = rt().block_on(dispatch_all(&cfg, &kill_bell_event(), &fake));
        assert_eq!(r1.len(), 1, "require_human must fire");
        assert_eq!(r2.len(), 1, "kill_bell_engaged must fire");
        let calls = fake.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1["event_name"], "require_human");
        assert_eq!(calls[1].1["event_name"], "kill_bell_engaged");
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
        // Default headers + None options elided.
        assert!(!yaml.contains("headers"), "got: {yaml}");
        assert!(!yaml.contains("channel"), "got: {yaml}");
        assert!(!yaml.contains("severity"), "got: {yaml}");
    }
}
