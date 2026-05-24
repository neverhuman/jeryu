//! Native bridge to Jekko's per-user LLM key pool.
//! Proof: `cargo test -p jeryu --lib llm::key_pool`
//!
//! JeRyu consumes `/home/ubuntu/jekko/users/<user>/llm.env` directly and writes
//! provider/model outcomes to each user's sibling `state.sqlite` table. Key
//! values stay in memory only; rendered health and receipt metadata use user
//! ids and file paths, never secret bytes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::jekko_llm_pool_usage::{
    FailureUpdate, KeyUsage, ensure_key_usage_schema, load_key_usage, record_key_failure,
    record_key_success,
};
use crate::llm::key_pool_env::parse_llm_env;
use crate::llm::{
    CallParams, CallResponse, ChatMessage, DataUse, LlmCallMetadata, LlmError, LlmProvider,
    OpenAiCompatibleClient,
};

pub const DEFAULT_JEKKO_USERS_ROOT: &str = "/home/ubuntu/jekko/users";

#[derive(Debug, Clone)]
pub struct JekkoKeyPool {
    users_root: PathBuf,
}

impl JekkoKeyPool {
    pub fn new(users_root: impl Into<PathBuf>) -> Self {
        Self {
            users_root: users_root.into(),
        }
    }

    pub fn from_env_or_default() -> Self {
        let root = std::env::var_os("JERYU_LLM_USERS_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_JEKKO_USERS_ROOT));
        Self::new(root)
    }

    pub fn users_root(&self) -> &Path {
        &self.users_root
    }

    pub fn has_secret_candidates(&self, secret_name: &str) -> bool {
        self.discover_secret_candidates(secret_name)
            .map(|c| !c.is_empty())
            .unwrap_or(false)
    }

    pub fn candidate_users(&self, secret_name: &str) -> Result<BTreeSet<String>> {
        Ok(self
            .discover_secret_candidates(secret_name)?
            .into_iter()
            .map(|candidate| candidate.user_id)
            .collect())
    }

    pub async fn health(
        &self,
        secret_name: &str,
        provider: &str,
        model: &str,
    ) -> Result<Vec<BalancerHealth>> {
        let mut health = Vec::new();
        for candidate in self.discover_secret_candidates(secret_name)? {
            let _ = ensure_key_usage_schema(&candidate.state_path).await;
            let usage = load_key_usage(&candidate.state_path, provider, model)
                .await
                .unwrap_or_else(|_| KeyUsage::ready(provider, model));
            let score = score_usage(&usage, Utc::now().timestamp());
            health.push(BalancerHealth {
                user_id: candidate.user_id,
                provider: provider.to_string(),
                model: model.to_string(),
                key_source_path: candidate.llm_env_path.display().to_string(),
                state_path: candidate.state_path.display().to_string(),
                status: usage.status,
                attempts: usage.attempts,
                failures: usage.failures,
                last_failure_at: usage.last_failure_at,
                cooldown_until: usage.cooldown_until,
                ready: score.is_some(),
                score,
            });
        }
        Ok(health)
    }

    pub async fn select(
        &self,
        secret_name: &str,
        provider: &str,
        model: &str,
    ) -> Result<Option<SelectedKey>> {
        let now = Utc::now().timestamp();
        let mut scored = Vec::new();
        for candidate in self.discover_secret_candidates(secret_name)? {
            ensure_key_usage_schema(&candidate.state_path).await?;
            let usage = load_key_usage(&candidate.state_path, provider, model)
                .await
                .unwrap_or_else(|_| KeyUsage::ready(provider, model));
            if let Some(score) = score_usage(&usage, now) {
                scored.push((score, candidate, usage));
            }
        }
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.user_id.cmp(&b.1.user_id))
        });
        let Some((score, candidate, usage)) = scored.into_iter().next() else {
            return Ok(None);
        };
        Ok(Some(SelectedKey {
            user_id: candidate.user_id,
            secret_name: secret_name.to_string(),
            key_value: candidate.key_value,
            key_source_path: candidate.llm_env_path,
            state_path: candidate.state_path,
            provider: provider.to_string(),
            model: model.to_string(),
            attempts_before: usage.attempts,
            failures_before: usage.failures,
            score,
        }))
    }

    fn discover_secret_candidates(&self, secret_name: &str) -> Result<Vec<SecretCandidate>> {
        let entries = match std::fs::read_dir(&self.users_root) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => {
                return Err(err).with_context(|| format!("read {}", self.users_root.display()));
            }
        };
        let mut dirs = Vec::new();
        for entry in entries {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if !file_type.is_dir() {
                continue;
            }
            dirs.push(entry.path());
        }
        dirs.sort();

        let mut out = Vec::new();
        for user_dir in dirs {
            let Some(user_id) = user_dir
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToString::to_string)
            else {
                continue;
            };
            let llm_env_path = user_dir.join("llm.env");
            let contents = match std::fs::read_to_string(&llm_env_path) {
                Ok(contents) => contents,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => {
                    return Err(err).with_context(|| format!("read {}", llm_env_path.display()));
                }
            };
            let values = parse_llm_env(&contents);
            if let Some(value) = values.get(secret_name)
                && !value.trim().is_empty()
            {
                out.push(SecretCandidate {
                    user_id,
                    key_value: value.clone(),
                    llm_env_path,
                    state_path: user_dir.join("state.sqlite"),
                });
            }
        }
        Ok(out)
    }
}

#[derive(Debug, Clone)]
pub struct SelectedKey {
    pub user_id: String,
    pub secret_name: String,
    pub key_value: String,
    pub key_source_path: PathBuf,
    pub state_path: PathBuf,
    pub provider: String,
    pub model: String,
    pub attempts_before: u64,
    pub failures_before: u64,
    pub score: f64,
}

impl SelectedKey {
    fn success_metadata(&self, resp: &CallResponse) -> LlmCallMetadata {
        LlmCallMetadata {
            provider: self.provider.clone(),
            model: resp.model.clone(),
            user_id: Some(self.user_id.clone()),
            key_source_path: Some(self.key_source_path.display().to_string()),
            status: "success".to_string(),
            prompt_tokens: resp.prompt_tokens.unwrap_or(0),
            completion_tokens: resp.completion_tokens.unwrap_or(0),
            estimated_micro_usd: 0,
            failure_reason: None,
        }
    }

    fn failure_metadata(&self, err: &LlmError, status: &str) -> LlmCallMetadata {
        LlmCallMetadata {
            provider: self.provider.clone(),
            model: self.model.clone(),
            user_id: Some(self.user_id.clone()),
            key_source_path: Some(self.key_source_path.display().to_string()),
            status: status.to_string(),
            prompt_tokens: 0,
            completion_tokens: 0,
            estimated_micro_usd: 0,
            failure_reason: Some(redacted_failure_reason(err)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BalancerHealth {
    pub user_id: String,
    pub provider: String,
    pub model: String,
    pub key_source_path: String,
    pub state_path: String,
    pub status: String,
    pub attempts: u64,
    pub failures: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_until: Option<i64>,
    pub ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

#[derive(Debug, Clone)]
struct SecretCandidate {
    user_id: String,
    key_value: String,
    llm_env_path: PathBuf,
    state_path: PathBuf,
}

#[derive(Clone)]
pub struct BalancedOpenAiCompatibleClient {
    provider: String,
    base_url: String,
    api_key_secret: String,
    default_headers: Vec<(String, String)>,
    data_use: DataUse,
    pool: Arc<JekkoKeyPool>,
}

impl std::fmt::Debug for BalancedOpenAiCompatibleClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BalancedOpenAiCompatibleClient")
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .field("api_key_secret", &self.api_key_secret)
            .field("users_root", &self.pool.users_root())
            .finish_non_exhaustive()
    }
}

impl BalancedOpenAiCompatibleClient {
    pub fn new(
        provider: impl Into<String>,
        base_url: impl Into<String>,
        api_key_secret: impl Into<String>,
        pool: Arc<JekkoKeyPool>,
    ) -> Self {
        Self {
            provider: provider.into(),
            base_url: base_url.into(),
            api_key_secret: api_key_secret.into(),
            default_headers: Vec::new(),
            data_use: DataUse::Unknown,
            pool,
        }
    }

    pub fn with_header(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.default_headers.push((k.into(), v.into()));
        self
    }

    pub fn with_data_use(mut self, data_use: DataUse) -> Self {
        self.data_use = data_use;
        self
    }

    pub async fn call_with_metadata(
        &self,
        messages: &[ChatMessage],
        params: &CallParams,
    ) -> Result<(CallResponse, Option<LlmCallMetadata>), LlmError> {
        let max_attempts = self
            .pool
            .candidate_users(&self.api_key_secret)
            .map(|users| users.len().max(1))
            .unwrap_or(1);
        let mut last_err = None;
        for _ in 0..max_attempts {
            let selected = self
                .pool
                .select(&self.api_key_secret, &self.provider, &params.model)
                .await
                .map_err(|err| LlmError::Transient(format!("key-pool selection failed: {err}")))?;
            let Some(selected) = selected else {
                break;
            };

            let mut client = OpenAiCompatibleClient::new(&self.provider, &self.base_url)
                .with_api_key(selected.key_value.clone())
                .with_data_use(self.data_use);
            for (key, value) in &self.default_headers {
                client = client.with_header(key, value);
            }

            match client.call(messages, params).await {
                Ok(resp) => {
                    if let Err(err) = record_key_success(
                        &selected.state_path,
                        &selected.provider,
                        &selected.model,
                    )
                    .await
                    {
                        tracing::warn!(
                            user_id = selected.user_id.as_str(),
                            provider = selected.provider.as_str(),
                            model = selected.model.as_str(),
                            "failed to record LLM key success: {err}"
                        );
                    }
                    let metadata = selected.success_metadata(&resp);
                    return Ok((resp, Some(metadata)));
                }
                Err(err) => {
                    let update = classify_failure(&err);
                    if let Err(record_err) = record_key_failure(
                        &selected.state_path,
                        &selected.provider,
                        &selected.model,
                        &update,
                    )
                    .await
                    {
                        tracing::warn!(
                            user_id = selected.user_id.as_str(),
                            provider = selected.provider.as_str(),
                            model = selected.model.as_str(),
                            "failed to record LLM key failure: {record_err}"
                        );
                    }
                    let metadata = selected.failure_metadata(&err, &update.status);
                    tracing::warn!(
                        user_id = metadata.user_id.as_deref().unwrap_or("unknown"),
                        provider = metadata.provider.as_str(),
                        model = metadata.model.as_str(),
                        status = metadata.status.as_str(),
                        reason = metadata.failure_reason.as_deref().unwrap_or("unknown"),
                        "balanced LLM call failed"
                    );
                    last_err = Some(err);
                }
            }
        }
        Err(last_err.unwrap_or(LlmError::RateLimited {
            retry_after_ms: 60_000,
        }))
    }
}

#[async_trait]
impl LlmProvider for BalancedOpenAiCompatibleClient {
    fn id(&self) -> &str {
        &self.provider
    }

    fn data_use(&self) -> DataUse {
        self.data_use
    }

    async fn call(
        &self,
        messages: &[ChatMessage],
        params: &CallParams,
    ) -> Result<CallResponse, LlmError> {
        let (resp, _metadata) = self.call_with_metadata(messages, params).await?;
        Ok(resp)
    }
}

fn classify_failure(err: &LlmError) -> FailureUpdate {
    let now = Utc::now().timestamp();
    let (status, cooldown_secs) = match err {
        LlmError::Auth => ("auth_failed", None),
        LlmError::RateLimited { retry_after_ms } => {
            let secs = ((*retry_after_ms).saturating_add(999) / 1_000).max(60);
            ("rate_limited", Some(secs as i64))
        }
        LlmError::Transient(message) if message.starts_with("server ") => {
            ("server_error", Some(10))
        }
        LlmError::Transient(_) | LlmError::Parse(_) => ("temporary_failure", Some(30)),
        LlmError::Permanent(_) => ("temporary_failure", Some(30)),
        LlmError::BudgetExhausted(_) | LlmError::PolicyViolation(_) => {
            ("temporary_failure", Some(30))
        }
    };
    FailureUpdate {
        status: status.to_string(),
        failed_at: now,
        cooldown_until: cooldown_secs.map(|seconds| now.saturating_add(seconds)),
    }
}

fn redacted_failure_reason(err: &LlmError) -> String {
    match err {
        LlmError::Auth => "auth_failed".to_string(),
        LlmError::RateLimited { .. } => "rate_limited".to_string(),
        LlmError::Transient(message) if message.starts_with("server ") => {
            "server_error".to_string()
        }
        LlmError::Transient(_) => "temporary_failure".to_string(),
        LlmError::Permanent(_) => "permanent_provider_error".to_string(),
        LlmError::Parse(_) => "response_parse_error".to_string(),
        LlmError::BudgetExhausted(_) => "budget_exhausted".to_string(),
        LlmError::PolicyViolation(_) => "policy_violation".to_string(),
    }
}

fn score_usage(usage: &KeyUsage, now: i64) -> Option<f64> {
    if usage.status == "auth_failed" {
        return None;
    }
    if let Some(cooldown_until) = usage.cooldown_until
        && cooldown_until > now
    {
        return None;
    }
    let base = match usage.status.as_str() {
        "ready" => 100.0,
        "server_error" => 45.0,
        "temporary_failure" => 50.0,
        "rate_limited" => 35.0,
        _ => 40.0,
    };
    let attempt_weight = (usage.attempts.saturating_add(1) as f64).powi(2);
    let failure_penalty = if usage.failures == 0 {
        1.0
    } else {
        1.0 / (usage.failures.saturating_add(1) as f64)
    };
    let recent_penalty = match usage.last_failure_at {
        Some(last) if now.saturating_sub(last) < 300 => 0.5,
        _ => 1.0,
    };
    Some(base * failure_penalty * recent_penalty / attempt_weight)
}

#[cfg(test)]
#[path = "key_pool_tests.rs"]
mod tests;
