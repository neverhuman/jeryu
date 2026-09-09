//! Production reviewer coordination.
//!
//! [`ProductionReviewerOrchestrator`] runs the required reviewer agents
//! concurrently against a single `EvidencePack`, gated by the `BudgetLedger`,
//! and returns one signed `AgentApprovalReceipt` per role.
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

use super::receipt::{receipt_role_to_id, sign_canonical, synth_abstain};
use super::{ESTIMATED_REVIEWER_COST_MICRO_USD, ReviewerOrchestrator};
use crate::llm::{Budget, BudgetLedger, LlmRouter, TokenUsage};
use crate::reviewers::lockfile::{LockfileReviewInputs, run_lockfile_review};
use crate::reviewers::runtime::{RuntimeReviewInputs, run_runtime_review};
use crate::reviewers::security::{SecurityReviewInputs, run_security_review};
use crate::reviewers::test_integrity::{TestIntegrityReviewInputs, run_test_integrity_review};
use crate::schema::{AgentApprovalReceipt, EvidencePack, ReviewerRole};
use crate::signing::EdSigningKey;
use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

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
