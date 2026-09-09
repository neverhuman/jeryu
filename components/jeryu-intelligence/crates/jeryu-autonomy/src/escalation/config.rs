//! Escalation configuration schema, deserialized from
//! `.jeryu/autonomy/autonomy.yml::escalation`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationKind {
    Slack,
    /// Deserializes from both `pagerduty` and `pager_duty`; serializes as `pagerduty`.
    #[serde(rename = "pagerduty", alias = "pager_duty")]
    PagerDuty,
    GenericJson,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WebhookConfig {
    pub kind: EscalationKind,
    /// Name of the env / secret variable holding the actual webhook URL.
    /// Resolved at dispatch time by the [`crate::seam::EscalationSink`]
    /// implementation.
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
    /// True if this event name is in the `on_events` allowlist AND the config is
    /// enabled. An empty `on_events` means "nothing fires" (fail-closed).
    pub fn permits(&self, event_name: &str) -> bool {
        self.enabled && self.on_events.iter().any(|e| e == event_name)
    }
}
