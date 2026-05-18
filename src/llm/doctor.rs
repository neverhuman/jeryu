//! Provider doctor: verifies every configured LLM provider responds to a
//! minimal ping. Used by Phase 2.5 `jeryu autonomy doctor` (CLI wiring is
//! a thin wrapper around `sweep_providers`).

use crate::llm::{
    CallParams, ChatMessage, DataUse, LlmError, LlmProvider, OpenAiCompatibleClient,
    SecretResolver, resolve_secret,
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
    /// Default probe set chosen from the live-verified providers on 2026-05-16.
    /// Includes the ones in `~/llm.env` that responded successfully + the
    /// most reliable model per provider.
    pub fn default_set() -> Vec<Self> {
        vec![
            Self {
                provider_id: "openrouter".into(),
                base_url: "https://openrouter.ai/api/v1".into(),
                api_env_var: "OPENROUTER_API_KEY".into(),
                model: "nvidia/nemotron-3-super-120b-a12b:free".into(),
                extra_headers: vec![
                    (
                        "HTTP-Referer".into(),
                        "https://github.com/jeryu/jeryu".into(),
                    ),
                    ("X-Title".into(), "jeryu-evidence-gate-doctor".into()),
                ],
            },
            Self {
                provider_id: "groq".into(),
                base_url: "https://api.groq.com/openai/v1".into(),
                api_env_var: "GROQ_API_KEY".into(),
                model: "llama-3.3-70b-versatile".into(),
                extra_headers: vec![],
            },
            Self {
                provider_id: "gemini".into(),
                base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
                api_env_var: "GEMINI_API_KEY".into(),
                model: "gemini-2.0-flash".into(),
                extra_headers: vec![],
            },
            Self {
                provider_id: "cerebras".into(),
                base_url: "https://api.cerebras.ai/v1".into(),
                api_env_var: "CEREBRAS_API_KEY".into(),
                model: "llama-3.3-70b".into(),
                extra_headers: vec![],
            },
            Self {
                provider_id: "nvidia".into(),
                base_url: "https://integrate.api.nvidia.com/v1".into(),
                api_env_var: "NVIDIA_API_KEY".into(),
                model: "meta/llama-3.3-70b-instruct".into(),
                extra_headers: vec![],
            },
            Self {
                provider_id: "fireworks".into(),
                base_url: "https://api.fireworks.ai/inference/v1".into(),
                api_env_var: "FIREWORKS_API_KEY".into(),
                model: "accounts/fireworks/models/llama-v3p3-70b-instruct".into(),
                extra_headers: vec![],
            },
        ]
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
    fn default_probe_set_covers_known_providers() {
        let s = DoctorProbe::default_set();
        let ids: Vec<&str> = s.iter().map(|p| p.provider_id.as_str()).collect();
        assert!(ids.contains(&"openrouter"));
        assert!(ids.contains(&"groq"));
        assert!(ids.contains(&"gemini"));
    }

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
