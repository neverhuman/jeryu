//! Provider doctor: verifies every configured LLM provider responds to a
//! minimal ping. Used by Phase 2.5 `jeryu autonomy doctor` (CLI wiring is
//! a thin wrapper around `sweep_providers`).

use crate::llm::{
    CallParams, ChatMessage, DataUse, LlmError, LlmProvider, OpenAiCompatibleClient,
    SecretResolver,
    provider_chains::{ProviderEntry, ProvidersConfig},
    resolve_secret,
};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ProviderCheckResult {
    pub provider_id: String,
    pub status: ProviderStatus,
    pub model_tried: String,
    pub latency_ms: u128,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStatus {
    Ok,
    NoKey,
    Auth,
    RateLimited,
    Unavailable,
    Skipped,
}

/// A configured provider entry to probe.
#[derive(Debug, Clone)]
pub struct DoctorProbe {
    pub provider_id: String,
    pub base_url: String,
    pub api_env_var: String,
    pub model: String,
    pub extra_headers: Vec<(String, String)>,
}

impl DoctorProbe {
    pub fn from_providers_config(config: &ProvidersConfig) -> Vec<Self> {
        let mut roles: Vec<&String> = config.chains.keys().collect();
        roles.sort();
        let mut probes = Vec::new();
        for role in roles {
            if let Some(entries) = config.chains.get(role) {
                for (idx, entry) in entries.iter().enumerate() {
                    probes.push(Self::from_provider_entry(role, idx, entry));
                }
            }
        }
        probes
    }

    fn from_provider_entry(role: &str, idx: usize, entry: &ProviderEntry) -> Self {
        let mut extra_headers: Vec<(String, String)> = entry
            .extra_headers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        extra_headers.sort_by(|a, b| a.0.cmp(&b.0));
        Self {
            provider_id: format!("{}#{}:{}", role, idx + 1, entry.provider),
            base_url: entry.base_url.clone(),
            api_env_var: entry.api_key_secret.clone(),
            model: entry.model_id.clone(),
            extra_headers,
        }
    }
}

pub async fn sweep_providers(
    probes: &[DoctorProbe],
    resolver: &SecretResolver,
) -> Vec<ProviderCheckResult> {
    let mut results = Vec::with_capacity(probes.len());
    for probe in probes {
        results.push(probe_one(probe, resolver).await);
    }
    results
}

async fn probe_one(probe: &DoctorProbe, resolver: &SecretResolver) -> ProviderCheckResult {
    let key = match resolve_secret(&probe.api_env_var, resolver) {
        Some(k) => k.value,
        None => {
            return ProviderCheckResult {
                provider_id: probe.provider_id.clone(),
                status: ProviderStatus::NoKey,
                model_tried: probe.model.clone(),
                latency_ms: 0,
                note: format!("{} not found in secrets chain", probe.api_env_var),
            };
        }
    };
    let mut client = OpenAiCompatibleClient::new(&probe.provider_id, &probe.base_url)
        .with_api_key(key)
        .with_data_use(DataUse::Unknown);
    for (k, v) in &probe.extra_headers {
        client = client.with_header(k, v);
    }
    let client = Arc::new(client);
    let messages = vec![
        ChatMessage::system("Output exactly: PONG"),
        ChatMessage::user("PING"),
    ];
    let params = CallParams {
        model: probe.model.clone(),
        temperature: 0.0,
        max_tokens: 10,
        timeout_ms: 20_000,
        ..CallParams::default()
    };
    let start = std::time::Instant::now();
    // Race against a hard ceiling so a hanging provider doesn't stall the sweep.
    let result = tokio::time::timeout(
        Duration::from_millis(25_000),
        client.call(&messages, &params),
    )
    .await;
    let latency_ms = start.elapsed().as_millis();
    let (status, note) = match result {
        Ok(Ok(resp)) => (
            ProviderStatus::Ok,
            format!(
                "ok (model={}, content={:?})",
                resp.model,
                resp.content.chars().take(40).collect::<String>()
            ),
        ),
        Ok(Err(LlmError::Auth)) => (
            ProviderStatus::Auth,
            "auth failed (key invalid for this provider)".into(),
        ),
        Ok(Err(LlmError::RateLimited { retry_after_ms })) => (
            ProviderStatus::RateLimited,
            format!("429; retry-after {} ms", retry_after_ms),
        ),
        Ok(Err(e)) => (ProviderStatus::Unavailable, format!("error: {e}")),
        Err(_) => (ProviderStatus::Unavailable, "timed out (>25s)".into()),
    };
    ProviderCheckResult {
        provider_id: probe.provider_id.clone(),
        status,
        model_tried: probe.model.clone(),
        latency_ms,
        note,
    }
}

#[path = "doctor_render.rs"]
mod render;
pub use render::render_report;
