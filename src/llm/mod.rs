//! Owner: Evidence Gate / LLM provider plane
//! Proof: `cargo nextest run -p jeryu -- llm::`
//! Invariants:
//!   - Secrets never appear in error strings or logs (use `redact::*` if needed).
//!   - Every call is preceded by a `scrub::scrub_diff` pass and fails closed on findings.
//!   - `data_use: train_on_input` providers are refused unless explicit opt-in.
//!
//! Public surface for talking to LLM providers (OpenRouter, Groq, Gemini, Cerebras, Nvidia,
//! Fireworks, Ollama, …). Everything is OpenAI-compatible at the transport level.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod budget;
pub mod doctor;
pub mod openai_compatible;
pub mod provider_chains;
pub mod router;
pub mod scrub;
pub mod secrets;
// Wave 8.D: restart-safe SQL successor to in-memory `BudgetLedger`. Module
// declared here so its `#[cfg(test)]` block participates in `cargo test`.
// Public re-exports are intentionally NOT added — call sites that need the
// SQL impl import via the full path until the integration wave wires it in.
pub mod sql_budget_ledger;

pub use budget::{Budget, BudgetLedger, BudgetTracker, TokenUsage};
pub use doctor::{
    DoctorProbe, ProviderCheckResult, ProviderStatus, render_report, sweep_providers,
};
pub use openai_compatible::OpenAiCompatibleClient;
pub use router::{LlmRouter, RoleChain, RoleChainEntry};
pub use scrub::{ScrubFinding, ScrubReport, scrub_diff};
pub use secrets::{SecretResolver, SecretSource, resolve_secret};

/// One chat message, OpenAI-style.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }
}

/// Parameters for a single completion call.
#[derive(Debug, Clone)]
pub struct CallParams {
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub timeout_ms: u64,
    pub seed: Option<u64>,
    /// Extra HTTP headers (e.g. OpenRouter `HTTP-Referer`).
    pub extra_headers: Vec<(String, String)>,
}

impl Default for CallParams {
    fn default() -> Self {
        Self {
            model: String::new(),
            temperature: 0.0,
            max_tokens: 1024,
            timeout_ms: 30_000,
            seed: None,
            extra_headers: Vec::new(),
        }
    }
}

/// One chat completion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallResponse {
    /// Provider id (e.g. "openrouter", "groq").
    pub provider: String,
    /// Model id actually used (provider may resolve aliases).
    pub model: String,
    /// Assistant message content.
    pub content: String,
    /// Token usage if reported by provider.
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    /// SHA-256 of `content` as `sha256:<hex>` — for receipt audit replay.
    pub raw_response_sha: String,
    /// Wall-clock latency in ms.
    pub latency_ms: u64,
}

/// Categorical error so the router can decide whether to fall back.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("provider auth failed (do NOT retry on next provider)")]
    Auth,
    #[error("provider rate-limited; retry after {retry_after_ms} ms")]
    RateLimited { retry_after_ms: u64 },
    #[error("provider transient error: {0}")]
    Transient(String),
    #[error("provider permanent error: {0}")]
    Permanent(String),
    #[error("response parse error: {0}")]
    Parse(String),
    #[error("budget exhausted ({0})")]
    BudgetExhausted(String),
    #[error("policy violation: {0}")]
    PolicyViolation(String),
}

impl LlmError {
    /// True if the router should hop to the next provider in the chain.
    pub fn is_retryable_on_fallback(&self) -> bool {
        matches!(
            self,
            LlmError::RateLimited { .. } | LlmError::Transient(_) | LlmError::Permanent(_)
        )
    }
}

/// Per-provider data-use policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum DataUse {
    NoTrain,
    TrainOnInput,
    #[default]
    Unknown,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Stable provider id (matches `.autonomy/providers/llm.yml` `id` field).
    fn id(&self) -> &str;
    /// Provider-declared training-data policy.
    fn data_use(&self) -> DataUse;
    /// One chat completion call.
    async fn call(
        &self,
        messages: &[ChatMessage],
        params: &CallParams,
    ) -> Result<CallResponse, LlmError>;
}
