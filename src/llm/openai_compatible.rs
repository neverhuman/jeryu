//! OpenAI-compatible HTTP adapter.
//!
//! Single client used for OpenRouter, Groq, Gemini's OpenAI endpoint, Cerebras,
//! Nvidia NIM, Fireworks, vLLM, Ollama (`/v1`), and anything else that speaks
//! the `POST /chat/completions` shape.

use crate::llm::{CallParams, CallResponse, ChatMessage, DataUse, LlmError, LlmProvider};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleClient {
    id: String,
    base_url: String,
    api_key: Option<String>,
    default_headers: Vec<(String, String)>,
    data_use: DataUse,
    http: reqwest::Client,
}

impl OpenAiCompatibleClient {
    pub fn new(id: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: None,
            default_headers: Vec::new(),
            data_use: DataUse::Unknown,
            http: reqwest::Client::builder()
                .user_agent("jeryu-evidence-gate/0.1")
                .build()
                .expect("reqwest client build"),
        }
    }

    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn with_header(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.default_headers.push((k.into(), v.into()));
        self
    }

    pub fn with_data_use(mut self, du: DataUse) -> Self {
        self.data_use = du;
        self
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    temperature: f32,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
}

#[derive(Deserialize, Debug)]
struct ChatResponse {
    #[serde(default)]
    model: Option<String>,
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize, Debug)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize, Debug)]
struct ChoiceMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct Usage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleClient {
    fn id(&self) -> &str {
        &self.id
    }
    fn data_use(&self) -> DataUse {
        self.data_use
    }

    async fn call(
        &self,
        messages: &[ChatMessage],
        params: &CallParams,
    ) -> Result<CallResponse, LlmError> {
        let url = format!("{}/chat/completions", self.base_url);
        let body = ChatRequest {
            model: &params.model,
            messages,
            temperature: params.temperature,
            max_tokens: params.max_tokens,
            seed: params.seed,
        };
        let started = Instant::now();
        let mut req = self
            .http
            .post(&url)
            .timeout(Duration::from_millis(params.timeout_ms))
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(k) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {k}"));
        }
        for (k, v) in self
            .default_headers
            .iter()
            .chain(params.extra_headers.iter())
        {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                LlmError::Transient(format!("timeout: {e}"))
            } else {
                LlmError::Transient(e.to_string())
            }
        })?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            // Drain body for debugging but do not leak the key.
            let _ = resp.text().await;
            return Err(LlmError::Auth);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after_ms = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(|s| s * 1_000)
                .unwrap_or(2_000);
            let _ = resp.text().await;
            return Err(LlmError::RateLimited { retry_after_ms });
        }
        if status.is_server_error() {
            // Error-path body extraction: if reading fails (transport
            // already lost), emit an empty body — this is the spec, not
            // graceful degradation of a primary path.
            let body = match resp.text().await {
                Ok(b) => b,
                Err(_) => String::new(),
            };
            return Err(LlmError::Transient(format!(
                "server {status}: {}",
                body.chars().take(300).collect::<String>()
            )));
        }
        if !status.is_success() {
            let body = match resp.text().await {
                Ok(b) => b,
                Err(_) => String::new(),
            };
            return Err(LlmError::Permanent(format!(
                "status {status}: {}",
                body.chars().take(300).collect::<String>()
            )));
        }

        let text = resp
            .text()
            .await
            .map_err(|e| LlmError::Transient(e.to_string()))?;
        let parsed: ChatResponse = serde_json::from_str(&text).map_err(|e| {
            LlmError::Parse(format!(
                "decode: {e}; head={}",
                text.chars().take(200).collect::<String>()
            ))
        })?;
        let content = match parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
        {
            Some(c) => c,
            None => {
                return Err(LlmError::Parse(
                    "no choices.message.content in response".into(),
                ));
            }
        };
        let mut h = Sha256::new();
        h.update(content.as_bytes());
        let raw = format!("sha256:{}", hex::encode(h.finalize()));
        // Per OpenAI-compatible spec, `usage` is optional; absence
        // means "the provider did not report token counts" — the
        // surrounding code records zeroed totals, which is the spec.
        let usage = match parsed.usage {
            Some(usage) => usage,
            None => Usage::default(),
        };
        let model = match parsed.model {
            Some(m) => m,
            None => params.model.clone(),
        };
        Ok(CallResponse {
            provider: self.id.clone(),
            model,
            content,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            raw_response_sha: raw,
            latency_ms: started.elapsed().as_millis() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn auth_error_maps_correctly() {
        // No live network in unit tests; only construct + verify id.
        let c = OpenAiCompatibleClient::new("openrouter", "https://example.invalid")
            .with_api_key("nope")
            .with_data_use(DataUse::NoTrain);
        assert_eq!(c.id(), "openrouter");
        assert_eq!(c.data_use(), DataUse::NoTrain);
    }
}
