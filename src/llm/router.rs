//! Per-role fallback router.
//!
//! Walks a chain of (provider, model, params) entries and returns the first
//! successful response. On `Auth` it stops (key is bad globally); on
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
                Err(e) if e.is_retryable_on_fallback() => {
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
    use async_trait::async_trait;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockProvider {
        id: String,
        data_use: DataUse,
        outcomes: Vec<Result<CallResponse, LlmError>>,
        call_idx: AtomicUsize,
    }

    impl MockProvider {
        fn new(id: &str, du: DataUse, outcomes: Vec<Result<CallResponse, LlmError>>) -> Self {
            Self {
                id: id.into(),
                data_use: du,
                outcomes,
                call_idx: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        fn id(&self) -> &str {
            &self.id
        }
        fn data_use(&self) -> DataUse {
            self.data_use
        }
        async fn call(
            &self,
            _m: &[ChatMessage],
            _p: &CallParams,
        ) -> Result<CallResponse, LlmError> {
            let i = self.call_idx.fetch_add(1, Ordering::SeqCst);
            match self.outcomes.get(i) {
                Some(Ok(r)) => Ok(r.clone()),
                Some(Err(e)) => Err(match e {
                    LlmError::Auth => LlmError::Auth,
                    LlmError::RateLimited { retry_after_ms } => LlmError::RateLimited {
                        retry_after_ms: *retry_after_ms,
                    },
                    LlmError::Transient(s) => LlmError::Transient(s.clone()),
                    LlmError::Permanent(s) => LlmError::Permanent(s.clone()),
                    LlmError::Parse(s) => LlmError::Parse(s.clone()),
                    LlmError::BudgetExhausted(s) => LlmError::BudgetExhausted(s.clone()),
                    LlmError::PolicyViolation(s) => LlmError::PolicyViolation(s.clone()),
                }),
                None => Err(LlmError::Permanent("mock outcomes exhausted".into())),
            }
        }
    }

    fn ok_response(provider: &str) -> CallResponse {
        CallResponse {
            provider: provider.into(),
            model: "test".into(),
            content: "{\"ok\":true}".into(),
            prompt_tokens: Some(1),
            completion_tokens: Some(1),
            raw_response_sha: "sha256:00".into(),
            latency_ms: 1,
        }
    }

    #[tokio::test]
    async fn router_falls_back_on_rate_limit() {
        let p1 = Arc::new(MockProvider::new(
            "a",
            DataUse::NoTrain,
            vec![Err(LlmError::RateLimited {
                retry_after_ms: 100,
            })],
        ));
        let p2 = Arc::new(MockProvider::new(
            "b",
            DataUse::NoTrain,
            vec![Ok(ok_response("b"))],
        ));
        let mut chain = RoleChain {
            role: "reviewer".into(),
            entries: vec![],
            forbid_train_on_input: false,
        };
        chain.entries.push(RoleChainEntry {
            provider: p1,
            params: CallParams::default(),
        });
        chain.entries.push(RoleChainEntry {
            provider: p2,
            params: CallParams::default(),
        });
        let mut r = LlmRouter::new();
        r.add_chain(chain);
        let resp = r.dispatch("reviewer", &[]).await.unwrap();
        assert_eq!(resp.provider, "b");
    }

    #[tokio::test]
    async fn router_stops_on_auth_error() {
        let p1 = Arc::new(MockProvider::new(
            "a",
            DataUse::NoTrain,
            vec![Err(LlmError::Auth)],
        ));
        let p2 = Arc::new(MockProvider::new(
            "b",
            DataUse::NoTrain,
            vec![Ok(ok_response("b"))],
        ));
        let mut chain = RoleChain {
            role: "reviewer".into(),
            entries: vec![],
            forbid_train_on_input: false,
        };
        chain.entries.push(RoleChainEntry {
            provider: p1,
            params: CallParams::default(),
        });
        chain.entries.push(RoleChainEntry {
            provider: p2,
            params: CallParams::default(),
        });
        let mut r = LlmRouter::new();
        r.add_chain(chain);
        let err = r.dispatch("reviewer", &[]).await.unwrap_err();
        assert!(matches!(err, LlmError::Auth));
    }

    #[tokio::test]
    async fn router_skips_train_on_input_when_forbidden() {
        let p1 = Arc::new(MockProvider::new(
            "trainer",
            DataUse::TrainOnInput,
            vec![Ok(ok_response("trainer"))],
        ));
        let p2 = Arc::new(MockProvider::new(
            "safe",
            DataUse::NoTrain,
            vec![Ok(ok_response("safe"))],
        ));
        let mut chain = RoleChain {
            role: "reviewer".into(),
            entries: vec![],
            forbid_train_on_input: true,
        };
        chain.entries.push(RoleChainEntry {
            provider: p1,
            params: CallParams::default(),
        });
        chain.entries.push(RoleChainEntry {
            provider: p2,
            params: CallParams::default(),
        });
        let mut r = LlmRouter::new();
        r.add_chain(chain);
        let resp = r.dispatch("reviewer", &[]).await.unwrap();
        assert_eq!(resp.provider, "safe");
    }
}
