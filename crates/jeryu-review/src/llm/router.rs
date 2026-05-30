//! Per-role failover router.
//!
//! Walks a chain of (provider, params) entries and returns the first successful
//! response. On `Auth` it stops (key is bad globally); on
//! `RateLimited`/`Transient`/`Permanent` it hops to the next entry.

use crate::llm::{CallParams, CallResponse, ChatMessage, DataUse, LlmError, LlmProvider};

#[derive(Clone)]
pub struct RoleChainEntry {
    pub provider: std::sync::Arc<dyn LlmProvider>,
    pub params: CallParams,
}

impl std::fmt::Debug for RoleChainEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoleChainEntry")
            .field("provider_id", &self.provider.id())
            .field("params", &self.params)
            .finish()
    }
}

#[derive(Default, Clone, Debug)]
pub struct RoleChain {
    pub role: String,
    pub entries: Vec<RoleChainEntry>,
    /// If true, refuse any entry whose provider declares `data_use: train_on_input`.
    pub forbid_train_on_input: bool,
}

#[derive(Default, Debug)]
pub struct LlmRouter {
    chains: std::collections::HashMap<String, RoleChain>,
}

impl LlmRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_chain(&mut self, chain: RoleChain) {
        self.chains.insert(chain.role.clone(), chain);
    }

    pub fn chain(&self, role: &str) -> Option<&RoleChain> {
        self.chains.get(role)
    }

    pub async fn dispatch(
        &self,
        role: &str,
        messages: &[ChatMessage],
    ) -> Result<CallResponse, LlmError> {
        let chain = self
            .chains
            .get(role)
            .ok_or_else(|| LlmError::Permanent(format!("no chain configured for role '{role}'")))?;
        let mut last_err: Option<LlmError> = None;
        for entry in &chain.entries {
            if chain.forbid_train_on_input && entry.provider.data_use() == DataUse::TrainOnInput {
                continue;
            }
            match entry.provider.call(messages, &entry.params).await {
                Ok(r) => return Ok(r),
                Err(e @ LlmError::Auth) => {
                    last_err = Some(e);
                    break;
                }
                Err(e) if e.is_retryable_on_failover() => {
                    last_err = Some(e);
                    continue;
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| LlmError::Permanent("empty chain".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::CallParams;
    use async_trait::async_trait;
    use std::sync::Arc;

    struct P {
        id: String,
        outcome: u8, // 0 ok, 1 rate-limited, 2 auth
    }

    #[async_trait]
    impl LlmProvider for P {
        fn id(&self) -> &str {
            &self.id
        }
        fn data_use(&self) -> DataUse {
            DataUse::NoTrain
        }
        async fn call(
            &self,
            _m: &[ChatMessage],
            _p: &CallParams,
        ) -> Result<CallResponse, LlmError> {
            match self.outcome {
                0 => Ok(CallResponse {
                    provider: self.id.clone(),
                    model: "m".into(),
                    content: "{}".into(),
                    prompt_tokens: Some(1),
                    completion_tokens: Some(1),
                    raw_response_sha: "sha256:0".into(),
                    latency_ms: 0,
                }),
                1 => Err(LlmError::RateLimited { retry_after_ms: 10 }),
                _ => Err(LlmError::Auth),
            }
        }
    }

    fn chain(entries: Vec<(&str, u8)>) -> LlmRouter {
        let mut c = RoleChain {
            role: "reviewer-security".into(),
            entries: vec![],
            forbid_train_on_input: false,
        };
        for (id, outcome) in entries {
            c.entries.push(RoleChainEntry {
                provider: Arc::new(P {
                    id: id.into(),
                    outcome,
                }),
                params: CallParams::default(),
            });
        }
        let mut r = LlmRouter::new();
        r.add_chain(c);
        r
    }

    #[tokio::test]
    async fn failover_hops_past_rate_limited() {
        let r = chain(vec![("p1", 1), ("p2", 0)]);
        let resp = r.dispatch("reviewer-security", &[]).await.unwrap();
        assert_eq!(resp.provider, "p2");
    }

    #[tokio::test]
    async fn auth_error_stops_chain() {
        let r = chain(vec![("p1", 2), ("p2", 0)]);
        let err = r.dispatch("reviewer-security", &[]).await.unwrap_err();
        assert!(matches!(err, LlmError::Auth));
    }

    #[tokio::test]
    async fn missing_chain_errors() {
        let r = LlmRouter::new();
        assert!(r.dispatch("nope", &[]).await.is_err());
    }
}
