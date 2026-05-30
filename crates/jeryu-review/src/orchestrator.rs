//! Multi-reviewer orchestrator.
//!
//! Runs the required reviewer agents (Security / TestIntegrity / Runtime /
//! Lockfile) concurrently against a single `EvidencePack`, gated by the
//! `BudgetLedger`, and returns one signed `AgentApprovalReceipt` per role.
//!
//! Invariants:
//!   - One reviewer failing (LLM error, parse error, budget exhausted) NEVER
//!     aborts the whole batch — it becomes an `Abstain` receipt instead.
//!   - Every synthesized receipt carries the input pack's `evidence_pack_id`,
//!     `head_sha`, and `policy_sha`, so the judge's SHA-binding doesn't drop
//!     them later.
//!   - Every synthesized receipt has `not_author: true`.
//!   - Synthesized abstain receipts are signed with the orchestrator's ed25519
//!     key so the judge's `evidence_signature_invalid` condition accepts them.

use crate::llm::{Budget, BudgetLedger, LlmRouter, TokenUsage};
use crate::reviewers::lockfile::{LockfileReviewInputs, run_lockfile_review};
use crate::reviewers::runner::ReviewerRoleId;
use crate::reviewers::runtime::{RuntimeReviewInputs, run_runtime_review};
use crate::reviewers::security::{SecurityReviewInputs, run_security_review};
use crate::reviewers::test_integrity::{TestIntegrityReviewInputs, run_test_integrity_review};
use crate::schema::{
    AgentApprovalReceipt, EvidencePack, ReviewDecision, ReviewerRole, SchemaTag, TokenCounts,
};
use crate::signing::{EdSigningKey, Signature};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Estimated micro-USD cost of one reviewer call. Used to decide whether the
/// next call would exceed the daily cap; actual usage is recorded after.
pub const ESTIMATED_REVIEWER_COST_MICRO_USD: u64 = 5_000;

#[async_trait]
pub trait ReviewerOrchestrator: Send + Sync {
    /// Run every reviewer whose role is in `required_roles`. Return one receipt
    /// per role attempted. If a single role fails it produces an `Abstain`
    /// receipt; it does NOT abort the batch.
    async fn run_all(
        &self,
        pack: &EvidencePack,
        required_roles: &[ReviewerRole],
        diff_text: &str,
    ) -> Result<Vec<AgentApprovalReceipt>>;
}

// ---------------------------------------------------------------------------
// ProductionReviewerOrchestrator
// ---------------------------------------------------------------------------

pub struct ProductionReviewerOrchestrator {
    pub router: Arc<LlmRouter>,
    pub budget_ledger: Arc<BudgetLedger>,
    pub autonomy_dir: PathBuf,
    pub signing_key: Arc<EdSigningKey>,
    pub budget: Budget,
}

impl ProductionReviewerOrchestrator {
    pub fn new(
        router: Arc<LlmRouter>,
        budget_ledger: Arc<BudgetLedger>,
        autonomy_dir: PathBuf,
        signing_key: Arc<EdSigningKey>,
    ) -> Self {
        Self {
            router,
            budget_ledger,
            autonomy_dir,
            signing_key,
            budget: Budget {
                daily_micro_usd_cap: 1_000_000_000,
                per_pr_micro_usd_cap: 50_000_000,
            },
        }
    }

    /// Override the budget cap (useful for tests + tight CI policies).
    pub fn with_budget(mut self, budget: Budget) -> Self {
        self.budget = budget;
        self
    }

    /// Load the markdown prompt for `role` from `autonomy_dir`.
    fn load_prompt(&self, role: ReviewerRole) -> Result<String> {
        let rid = receipt_role_to_id(role);
        let path = self.autonomy_dir.join(rid.prompt_path());
        std::fs::read_to_string(&path)
            .map_err(|err| anyhow::anyhow!("missing reviewer prompt {}: {err}", path.display()))
    }
}

#[async_trait]
impl ReviewerOrchestrator for ProductionReviewerOrchestrator {
    async fn run_all(
        &self,
        pack: &EvidencePack,
        required_roles: &[ReviewerRole],
        diff_text: &str,
    ) -> Result<Vec<AgentApprovalReceipt>> {
        if required_roles.is_empty() {
            return Ok(Vec::new());
        }

        let mut handles: Vec<tokio::task::JoinHandle<(ReviewerRole, AgentApprovalReceipt)>> =
            Vec::with_capacity(required_roles.len());
        let mut immediate = Vec::new();

        for &role in required_roles {
            let router = self.router.clone();
            let ledger = self.budget_ledger.clone();
            let signing_key = self.signing_key.clone();
            let budget = self.budget.clone();
            if ledger.would_exceed(&budget, ESTIMATED_REVIEWER_COST_MICRO_USD) {
                immediate.push(synth_abstain(
                    role,
                    &pack.id,
                    &pack.head_sha,
                    &pack.policy_sha,
                    "budget exhausted: would_exceed daily cap".to_string(),
                    &self.signing_key,
                ));
                continue;
            }
            let prompt = match self.load_prompt(role) {
                Ok(prompt) => prompt,
                Err(err) => {
                    immediate.push(synth_abstain(
                        role,
                        &pack.id,
                        &pack.head_sha,
                        &pack.policy_sha,
                        format!("reviewer prompt unavailable: {err}"),
                        &self.signing_key,
                    ));
                    continue;
                }
            };
            // Clone the small fields the spawned task needs as owned Strings.
            let pack_id = pack.id.clone();
            let repo = pack.repo.clone();
            let head_sha = pack.head_sha.clone();
            let policy_sha = pack.policy_sha.clone();
            let target_branch = pack.target_branch.clone();
            let diff = diff_text.to_string();

            handles.push(tokio::spawn(async move {
                // 1. Budget gate — fires BEFORE the router is called.
                if ledger.would_exceed(&budget, ESTIMATED_REVIEWER_COST_MICRO_USD) {
                    return (
                        role,
                        synth_abstain(
                            role,
                            &pack_id,
                            &head_sha,
                            &policy_sha,
                            "budget exhausted: would_exceed daily cap".to_string(),
                            &signing_key,
                        ),
                    );
                }

                // 2. Dispatch to the role-specific reviewer.
                let outcome: Result<AgentApprovalReceipt, String> = match role {
                    ReviewerRole::Security => run_security_review(
                        &router,
                        &SecurityReviewInputs {
                            repo: &repo,
                            head_sha: &head_sha,
                            policy_sha: &policy_sha,
                            target_branch: &target_branch,
                            evidence_pack_id: &pack_id,
                            diff: &diff,
                            system_prompt_markdown: &prompt,
                            evidence_pack_json: None,
                            signing_key: Some(&signing_key),
                        },
                    )
                    .await
                    .map_err(|e| e.to_string()),
                    ReviewerRole::TestIntegrity => run_test_integrity_review(
                        &router,
                        &TestIntegrityReviewInputs {
                            repo: &repo,
                            head_sha: &head_sha,
                            policy_sha: &policy_sha,
                            target_branch: &target_branch,
                            evidence_pack_id: &pack_id,
                            diff: &diff,
                            system_prompt_markdown: &prompt,
                            evidence_pack_json: None,
                            signing_key: Some(&signing_key),
                        },
                    )
                    .await
                    .map_err(|e| e.to_string()),
                    ReviewerRole::Runtime => run_runtime_review(
                        &router,
                        &RuntimeReviewInputs {
                            repo: &repo,
                            head_sha: &head_sha,
                            policy_sha: &policy_sha,
                            target_branch: &target_branch,
                            evidence_pack_id: &pack_id,
                            diff: &diff,
                            system_prompt_markdown: &prompt,
                            evidence_pack_json: None,
                            signing_key: Some(&signing_key),
                        },
                    )
                    .await
                    .map_err(|e| e.to_string()),
                    ReviewerRole::Lockfile => run_lockfile_review(
                        &router,
                        &LockfileReviewInputs {
                            repo: &repo,
                            head_sha: &head_sha,
                            policy_sha: &policy_sha,
                            target_branch: &target_branch,
                            evidence_pack_id: &pack_id,
                            diff: &diff,
                            system_prompt_markdown: &prompt,
                            evidence_pack_json: None,
                            signing_key: Some(&signing_key),
                        },
                    )
                    .await
                    .map_err(|e| e.to_string()),
                    // Roles this orchestrator doesn't run become abstains so
                    // the caller still sees an entry per required role.
                    other => {
                        return (
                            other,
                            synth_abstain(
                                other,
                                &pack_id,
                                &head_sha,
                                &policy_sha,
                                format!("role {other:?} is not handled by ReviewerOrchestrator"),
                                &signing_key,
                            ),
                        );
                    }
                };

                let mut receipt = match outcome {
                    Ok(r) => r,
                    Err(e) => synth_abstain(
                        role,
                        &pack_id,
                        &head_sha,
                        &policy_sha,
                        format!("reviewer error: {e}"),
                        &signing_key,
                    ),
                };

                // 3. Record the spend.
                ledger.record(TokenUsage {
                    prompt_tokens: receipt.tokens.prompt as u64,
                    completion_tokens: receipt.tokens.completion as u64,
                    estimated_micro_usd: ESTIMATED_REVIEWER_COST_MICRO_USD,
                });

                // 4. Ensure the receipt is signed with the real ed25519 key.
                if receipt.signature.algo == "unsigned" {
                    receipt.signature = sign_canonical(&receipt, &signing_key);
                }

                (role, receipt)
            }));
        }

        // Join all tasks. A task panic becomes an abstain entry so the batch
        // still completes — never propagate panics as orchestrator errors.
        let mut out = immediate;
        for h in handles {
            match h.await {
                Ok((_, r)) => out.push(r),
                Err(join_err) => {
                    let role = required_roles
                        .iter()
                        .copied()
                        .find(|r| !out.iter().any(|x| x.role == *r))
                        .unwrap_or(ReviewerRole::Security);
                    out.push(synth_abstain(
                        role,
                        &pack.id,
                        &pack.head_sha,
                        &pack.policy_sha,
                        format!("reviewer task panicked: {join_err}"),
                        &self.signing_key,
                    ));
                }
            }
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// FakeReviewerOrchestrator (testing double)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn receipt_role_to_id(role: ReviewerRole) -> ReviewerRoleId {
    match role {
        ReviewerRole::Security => ReviewerRoleId::Security,
        ReviewerRole::TestIntegrity => ReviewerRoleId::TestIntegrity,
        ReviewerRole::Runtime => ReviewerRoleId::Runtime,
        ReviewerRole::Lockfile => ReviewerRoleId::Lockfile,
        ReviewerRole::Nightwatch => ReviewerRoleId::Nightwatch,
        // Judge and ReleaseShepherd have no ReviewerRoleId; the synth_abstain
        // path never reads the prompt for them.
        ReviewerRole::Judge | ReviewerRole::ReleaseShepherd => ReviewerRoleId::Security,
    }
}

pub(crate) fn agent_id_for(role: ReviewerRole) -> &'static str {
    match role {
        ReviewerRole::Security => "reviewer-security.v1",
        ReviewerRole::TestIntegrity => "reviewer-test-integrity.v1",
        ReviewerRole::Runtime => "reviewer-runtime.v1",
        ReviewerRole::Lockfile => "reviewer-lockfile.v1",
        ReviewerRole::Nightwatch => "reviewer-nightwatch.v1",
        ReviewerRole::Judge => "judge.v1",
        ReviewerRole::ReleaseShepherd => "release-shepherd.v1",
    }
}

fn synth_id(role: ReviewerRole, pack_id: &str) -> String {
    let ts = Utc::now().timestamp_millis();
    format!(
        "aar_{role:?}_{pack}_{ts}",
        role = role,
        pack = pack_id.chars().take(12).collect::<String>(),
        ts = ts
    )
}

/// Build an Abstain receipt synthesized by the orchestrator (NOT produced by a
/// per-role reviewer). Used for budget short-circuits and failure recovery.
fn synth_abstain(
    role: ReviewerRole,
    pack_id: &str,
    head_sha: &str,
    policy_sha: &str,
    reason: String,
    signing_key: &EdSigningKey,
) -> AgentApprovalReceipt {
    let mut r = AgentApprovalReceipt {
        schema: SchemaTag::new(),
        id: synth_id(role, pack_id),
        evidence_pack_id: pack_id.to_string(),
        role,
        agent_id: agent_id_for(role).to_string(),
        prompt_sha: None,
        provider: None,
        model: None,
        temperature: None,
        seed: None,
        raw_response_sha: None,
        head_sha: head_sha.to_string(),
        policy_sha: policy_sha.to_string(),
        decision: ReviewDecision::Abstain,
        reason: Some(reason),
        findings: vec![],
        not_author: true,
        tokens: TokenCounts::default(),
        created_at: Utc::now(),
        signature: Signature::unsigned(),
    };
    r.signature = sign_canonical(&r, signing_key);
    r
}

/// Sign the canonical JSON projection of `r` (everything except the signature
/// itself, which would be circular).
fn sign_canonical(r: &AgentApprovalReceipt, key: &EdSigningKey) -> Signature {
    let mut clone = r.clone();
    clone.signature = Signature::unsigned();
    let body = serde_json::to_string(&clone)
        .expect("AgentApprovalReceipt JSON serialization is infallible");
    key.sign_raw(body.as_bytes())
}

/// Default Pass receipt used by the fake when no canned receipt is registered.
fn default_pass_receipt(
    role: ReviewerRole,
    pack_id: &str,
    head_sha: &str,
    policy_sha: &str,
) -> AgentApprovalReceipt {
    AgentApprovalReceipt {
        schema: SchemaTag::new(),
        id: synth_id(role, pack_id),
        evidence_pack_id: pack_id.to_string(),
        role,
        agent_id: agent_id_for(role).to_string(),
        prompt_sha: None,
        provider: Some("fake".into()),
        model: Some("fake-model".into()),
        temperature: Some(0.0),
        seed: None,
        raw_response_sha: Some(format!("sha256:0{}", "0".repeat(63))),
        head_sha: head_sha.to_string(),
        policy_sha: policy_sha.to_string(),
        decision: ReviewDecision::Pass,
        reason: Some("fake pass".into()),
        findings: vec![],
        not_author: true,
        tokens: TokenCounts::default(),
        created_at: Utc::now(),
        signature: Signature::unsigned(),
    }
}

/// Default Abstain receipt used by the fake's `error_on` path.
fn default_abstain_receipt(
    role: ReviewerRole,
    pack_id: &str,
    head_sha: &str,
    policy_sha: &str,
) -> AgentApprovalReceipt {
    AgentApprovalReceipt {
        schema: SchemaTag::new(),
        id: synth_id(role, pack_id),
        evidence_pack_id: pack_id.to_string(),
        role,
        agent_id: agent_id_for(role).to_string(),
        prompt_sha: None,
        provider: None,
        model: None,
        temperature: None,
        seed: None,
        raw_response_sha: None,
        head_sha: head_sha.to_string(),
        policy_sha: policy_sha.to_string(),
        decision: ReviewDecision::Abstain,
        reason: Some("fake error_on triggered abstain".into()),
        findings: vec![],
        not_author: true,
        tokens: TokenCounts::default(),
        created_at: Utc::now(),
        signature: Signature::unsigned(),
    }
}
