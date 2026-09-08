//! LLM transport plane — the reviewer-call engine, behind thin trait seams.
//!
//! Invariants:
//!   - Every reviewer call is preceded by a [`scrub::scrub_diff`] pass that
//!     fails closed on any finding.
//!   - `data_use: train_on_input` providers are refused when the chain forbids
//!     them.
//!
//! Everything is OpenAI-compatible at the transport level; "Claude/GPT/Gemini"
//! is a config concern (per-role chains), not separate code paths. The real
//! provider impl lands behind the [`LlmProvider`] trait; tests use a
//! deterministic in-memory provider.

pub mod budget;
pub mod router;
pub mod scrub;

pub use budget::{Budget, BudgetLedger, TokenUsage};
pub use router::{LlmRouter, RoleChain, RoleChainEntry};
pub use scrub::{ScrubFinding, ScrubReport, scrub_diff};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// One chat message, OpenAI-style.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    pub provider: String,
    pub model: String,
    pub content: String,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    /// SHA-256 of `content` as `sha256:<hex>` — for receipt audit replay.
    pub raw_response_sha: String,
    pub latency_ms: u64,
}

/// Categorical error so the router can decide whether to try the next entry.
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
    pub fn is_retryable_on_failover(&self) -> bool {
        matches!(
            self,
            LlmError::RateLimited { .. } | LlmError::Transient(_) | LlmError::Permanent(_)
        )
    }
}

/// Per-provider data-use policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DataUse {
    NoTrain,
    TrainOnInput,
    #[default]
    Unknown,
}

/// Thin seam over an LLM endpoint. The reviewer-call engine only ever sees this
/// trait; the real OpenAI-compatible client implements it in the fused
/// workspace, tests use a deterministic in-memory provider.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Stable provider id.
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
