//! Per-role failover router.
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
#[path = "router_tests.rs"]
mod tests;
