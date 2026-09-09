use std::collections::BTreeMap;

use crate::AgentStreamError;

/// Required stream adapter configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerConfig {
    /// Bootstrap servers.
    pub bootstrap: String,
    /// Client id.
    pub client_id: String,
    /// Extra adapter options.
    pub options: BTreeMap<String, String>,
}

impl BrokerConfig {
    /// Read required stream settings from an environment map.
    pub fn from_env(env: &BTreeMap<String, String>) -> Result<Self, AgentStreamError> {
        let bootstrap = env
            .get("JERYU_AGENT_STREAM_BOOTSTRAP")
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .ok_or_else(missing_required_stream)?;
        let client_id = env
            .get("JERYU_AGENT_STREAM_CLIENT_ID")
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "jeryu-agent".to_string());
        let options = env
            .iter()
            .filter(|(key, _)| key.starts_with("JERYU_AGENT_STREAM_"))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        Ok(Self {
            bootstrap,
            client_id,
            options,
        })
    }
}

/// Typed error for missing required production stream.
#[must_use]
pub fn missing_required_stream() -> AgentStreamError {
    AgentStreamError::new(
        "agent_stream_required_unavailable",
        "verify required agent-run stream",
        "JERYU_AGENT_STREAM_BOOTSTRAP is not configured",
        &[
            "configure JERYU_AGENT_STREAM_BOOTSTRAP before starting production agent work",
            "set stream.required=false only for local contract tests that do not launch tools",
        ],
        "docs/testing.md#workcells",
        "rerun cargo test -p jeryu-agent-stream --jobs 40",
    )
}
