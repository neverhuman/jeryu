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

#[cfg(test)]
mod provider_config_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn probes_are_derived_from_provider_config_in_stable_order() {
        let mut headers = HashMap::new();
        headers.insert("X-Title".to_string(), "jeryu".to_string());
        headers.insert(
            "HTTP-Referer".to_string(),
            "https://example.com".to_string(),
        );
        let config = ProvidersConfig {
            schema: "vibegate.providers.v1".to_string(),
            default_role_chain: vec!["reviewer-security".to_string()],
            chains: HashMap::from([
                (
                    "reviewer-runtime".to_string(),
                    vec![ProviderEntry {
                        provider: "openrouter".to_string(),
                        base_url: "https://openrouter.ai/api/v1".to_string(),
                        model_id: "openai/gpt-oss-120b:free".to_string(),
                        api_key_secret: "OPENROUTER_API_KEY".to_string(),
                        data_use: "no_train".to_string(),
                        temperature: 0.0,
                        timeout_ms: 30_000,
                        max_tokens: 800,
                        extra_headers: HashMap::new(),
                    }],
                ),
                (
                    "reviewer-security".to_string(),
                    vec![ProviderEntry {
                        provider: "openrouter".to_string(),
                        base_url: "https://openrouter.ai/api/v1".to_string(),
                        model_id: "nvidia/nemotron-3-super-120b-a12b:free".to_string(),
                        api_key_secret: "OPENROUTER_API_KEY".to_string(),
                        data_use: "no_train".to_string(),
                        temperature: 0.0,
                        timeout_ms: 30_000,
                        max_tokens: 800,
                        extra_headers: headers,
                    }],
                ),
            ]),
        };

        let probes = DoctorProbe::from_providers_config(&config);

        assert_eq!(probes.len(), 2);
        assert_eq!(probes[0].provider_id, "reviewer-runtime#1:openrouter");
        assert_eq!(probes[1].provider_id, "reviewer-security#1:openrouter");
        assert_eq!(
            probes[1].extra_headers,
            vec![
                (
                    "HTTP-Referer".to_string(),
                    "https://example.com".to_string()
                ),
                ("X-Title".to_string(), "jeryu".to_string()),
            ]
        );
    }
}

/// Pretty-print a sweep report (one provider per line; safe to redirect to a log).
pub fn render_report(results: &[ProviderCheckResult]) -> String {
    let mut s = String::new();
    s.push_str("jeryu autonomy doctor — provider sweep\n");
    s.push_str("──────────────────────────────────────\n");
    for r in results {
        let glyph = match r.status {
            ProviderStatus::Ok => "✓ OK   ",
            ProviderStatus::NoKey => "○ NOKEY",
            ProviderStatus::Auth => "✗ AUTH ",
            ProviderStatus::RateLimited => "△ RATE ",
            ProviderStatus::Unavailable => "✗ DOWN ",
            ProviderStatus::Skipped => "— SKIP ",
        };
        s.push_str(&format!(
            "{glyph}  {:<10}  model={:<60}  {:>5}ms  {}\n",
            r.provider_id, r.model_tried, r.latency_ms, r.note
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_handles_all_statuses() {
        let results = vec![
            ProviderCheckResult {
                provider_id: "openrouter".into(),
                status: ProviderStatus::Ok,
                model_tried: "x:free".into(),
                latency_ms: 1234,
                note: "ok".into(),
            },
            ProviderCheckResult {
                provider_id: "groq".into(),
                status: ProviderStatus::Auth,
                model_tried: "y".into(),
                latency_ms: 100,
                note: "401".into(),
            },
            ProviderCheckResult {
                provider_id: "cerebras".into(),
                status: ProviderStatus::NoKey,
                model_tried: "z".into(),
                latency_ms: 0,
                note: "no key".into(),
            },
        ];
        let r = render_report(&results);
        assert!(r.contains("OK"));
        assert!(r.contains("AUTH"));
        assert!(r.contains("NOKEY"));
    }
}
