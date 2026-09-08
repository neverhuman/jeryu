//! Deterministic testing double for the reviewer orchestrator.
//!
//! [`FakeReviewerOrchestrator`] returns canned receipts (or default
//! pass/abstain receipts) without touching the LLM router or budget ledger,
//! and records which roles were invoked so tests can assert on coordination.

use super::ReviewerOrchestrator;
use super::receipt::{default_abstain_receipt, default_pass_receipt};
use crate::schema::{AgentApprovalReceipt, EvidencePack, ReviewerRole};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct FakeReviewerOrchestrator {
    pub canned_receipts: Arc<Mutex<HashMap<ReviewerRole, AgentApprovalReceipt>>>,
    pub recorded_calls: Arc<Mutex<Vec<ReviewerRole>>>,
    pub error_on: Arc<Mutex<Option<ReviewerRole>>>,
    pub latency_ms: Arc<Mutex<HashMap<ReviewerRole, u64>>>,
}

impl Default for FakeReviewerOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeReviewerOrchestrator {
    pub fn new() -> Self {
        Self {
            canned_receipts: Arc::new(Mutex::new(HashMap::new())),
            recorded_calls: Arc::new(Mutex::new(Vec::new())),
            error_on: Arc::new(Mutex::new(None)),
            latency_ms: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_canned(self, role: ReviewerRole, receipt: AgentApprovalReceipt) -> Self {
        self.canned_receipts.lock().unwrap().insert(role, receipt);
        self
    }

    pub fn error_on(self, role: ReviewerRole) -> Self {
        *self.error_on.lock().unwrap() = Some(role);
        self
    }

    pub fn with_latency(self, role: ReviewerRole, ms: u64) -> Self {
        self.latency_ms.lock().unwrap().insert(role, ms);
        self
    }
}

#[async_trait]
impl ReviewerOrchestrator for FakeReviewerOrchestrator {
    async fn run_all(
        &self,
        pack: &EvidencePack,
        required_roles: &[ReviewerRole],
        _diff_text: &str,
    ) -> Result<Vec<AgentApprovalReceipt>> {
        let mut handles = Vec::with_capacity(required_roles.len());
        for &role in required_roles {
            let canned = self.canned_receipts.clone();
            let recorded = self.recorded_calls.clone();
            let error_on = self.error_on.clone();
            let latencies = self.latency_ms.clone();
            let pack_id = pack.id.clone();
            let head_sha = pack.head_sha.clone();
            let policy_sha = pack.policy_sha.clone();
            handles.push(tokio::spawn(async move {
                let sleep_ms = latencies.lock().unwrap().get(&role).copied().unwrap_or(0);
                if sleep_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
                }
                recorded.lock().unwrap().push(role);
                let is_error = error_on.lock().unwrap().map(|r| r == role).unwrap_or(false);
                if is_error {
                    return default_abstain_receipt(role, &pack_id, &head_sha, &policy_sha);
                }
                if let Some(r) = canned.lock().unwrap().get(&role) {
                    return r.clone();
                }
                default_pass_receipt(role, &pack_id, &head_sha, &policy_sha)
            }));
        }
        let mut out = Vec::with_capacity(handles.len());
        for h in handles {
            out.push(h.await.expect("fake reviewer task panicked"));
        }
        Ok(out)
    }
}
