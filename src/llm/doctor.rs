//! Provider doctor: verifies every configured LLM provider responds to a
//! minimal ping. Used by Phase 2.5 `jeryu autonomy doctor` (CLI wiring is
//! a thin wrapper around `sweep_providers`).

use crate::llm::{
    BalancedOpenAiCompatibleClient, BalancerHealth, CallParams, ChatMessage, DataUse, JekkoKeyPool,
    LlmError, LlmProvider, OpenAiCompatibleClient, SecretResolver,
    provider_chains::{ProviderEntry, ProvidersConfig},
    resolve_secret,
};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct ProviderCheckResult {
    pub provider_id: String,
    pub status: ProviderStatus,
    pub model_tried: String,
    pub latency_ms: u128,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    let pool = if !resolver.ci_mode
        || std::env::var("JERYU_LLM_BALANCER_IN_CI")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    {
        Some(JekkoKeyPool::from_env_or_default())
    } else {
        None
    };
    // Two explicit outcomes for the balancer health probe: a successful
    // poll returns its sample list; an error means we have no observations
    // to display this cycle (still useful — the caller distinguishes "no
    // pool" from "pool errored").
    let balancer_health = match &pool {
        Some(pool) if pool.has_secret_candidates(&probe.api_env_var) => {
            match pool
                .health(&probe.api_env_var, probe.provider_name(), &probe.model)
                .await
            {
                Ok(samples) => samples,
                Err(_) => Vec::new(),
            }
        }
        _ => Vec::new(),
    };
    let key = match resolve_secret(&probe.api_env_var, resolver) {
        Some(k) => k.value,
        None => {
            if let Some(pool) = &pool
                && pool.has_secret_candidates(&probe.api_env_var)
            {
                return probe_one_balanced(probe, pool.clone(), balancer_health).await;
            }
            let note = append_balancer_note(
                format!("{} not found in secrets chain", probe.api_env_var),
                None,
                None,
                &balancer_health,
            );
            return ProviderCheckResult {
                provider_id: probe.provider_id.clone(),
                status: ProviderStatus::NoKey,
                model_tried: probe.model.clone(),
                latency_ms: 0,
                note,
            };
        }
    };
    if let Some(pool) = pool
        && pool.has_secret_candidates(&probe.api_env_var)
    {
        return probe_one_balanced(probe, pool, balancer_health).await;
    }
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
        note: append_balancer_note(note, None, None, &balancer_health),
    }
}

async fn probe_one_balanced(
    probe: &DoctorProbe,
    pool: JekkoKeyPool,
    balancer_health: Vec<BalancerHealth>,
) -> ProviderCheckResult {
    let mut client = BalancedOpenAiCompatibleClient::new(
        probe.provider_name(),
        &probe.base_url,
        &probe.api_env_var,
        Arc::new(pool),
    )
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
    let result = tokio::time::timeout(
        Duration::from_millis(25_000),
        client.call_with_metadata(&messages, &params),
    )
    .await;
    let latency_ms = start.elapsed().as_millis();
    let (status, note, selected_user_id, key_source_path) = match result {
        Ok(Ok((resp, meta))) => (
            ProviderStatus::Ok,
            format!(
                "ok via balanced key (model={}, content={:?})",
                resp.model,
                resp.content.chars().take(40).collect::<String>()
            ),
            meta.as_ref().and_then(|m| m.user_id.clone()),
            meta.as_ref().and_then(|m| m.key_source_path.clone()),
        ),
        Ok(Err(LlmError::Auth)) => (
            ProviderStatus::Auth,
            "auth failed for selected balanced key".into(),
            None,
            None,
        ),
        Ok(Err(LlmError::RateLimited { retry_after_ms })) => (
            ProviderStatus::RateLimited,
            format!("429; retry-after {} ms", retry_after_ms),
            None,
            None,
        ),
        Ok(Err(e)) => (
            ProviderStatus::Unavailable,
            format!("error: {e}"),
            None,
            None,
        ),
        Err(_) => (
            ProviderStatus::Unavailable,
            "timed out (>25s)".into(),
            None,
            None,
        ),
    };
    ProviderCheckResult {
        provider_id: probe.provider_id.clone(),
        status,
        model_tried: probe.model.clone(),
        latency_ms,
        note: append_balancer_note(note, selected_user_id, key_source_path, &balancer_health),
    }
}

impl DoctorProbe {
    pub fn provider_name(&self) -> &str {
        self.provider_id
            .rsplit_once(':')
            .map(|(_, provider)| provider)
            .unwrap_or(&self.provider_id)
    }
}

fn append_balancer_note(
    note: String,
    selected_user_id: Option<String>,
    key_source_path: Option<String>,
    balancer_health: &[BalancerHealth],
) -> String {
    if balancer_health.is_empty() {
        return note;
    }
    format!(
        "{note}; balancer candidates={} selected_user={} key_source={}",
        balancer_health.len(),
        selected_user_id.as_deref().unwrap_or("none"),
        key_source_path.as_deref().unwrap_or("none")
    )
}

#[path = "doctor_render.rs"]
mod render;
pub use render::render_report;
