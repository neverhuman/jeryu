//! Wave 8 — Auto-rejudge ReviewerOrchestrator.
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
//!   - Every synthesized receipt has `not_author: true` (we're reviewing, not
//!     authoring the change).
//!   - Synthesized abstain receipts are signed with the orchestrator's
//!     ed25519 key so the judge's `evidence_signature_invalid` condition
//!     accepts them in enforcement mode.

use crate::agent_review::lockfile::{LockfileReviewInputs, run_lockfile_review};
use crate::agent_review::runner::ReviewerRoleId;
use crate::agent_review::runtime::{RuntimeReviewInputs, run_runtime_review};
use crate::agent_review::security::{SecurityReviewInputs, run_security_review};
use crate::agent_review::test_integrity::{TestIntegrityReviewInputs, run_test_integrity_review};
use crate::autonomy::signing::{EdSigningKey, Signature};
use crate::autonomy::types::{
    AgentApprovalReceipt, EvidencePack, ReviewDecision, ReviewerRole, SchemaTag, TokenCounts,
};
use crate::llm::{Budget, BudgetLedger, LlmRouter};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Estimated micro-USD cost of one reviewer call. Used to decide whether the
/// next call would exceed the daily cap. Conservative ceiling — actual usage
/// is recorded after the call returns via `BudgetLedger::record`.
pub const ESTIMATED_REVIEWER_COST_MICRO_USD: u64 = 5_000;

#[async_trait]
pub trait ReviewerOrchestrator: Send + Sync {
    /// Run every reviewer whose role is in `required_roles`. Return one
    /// `AgentApprovalReceipt` per role attempted. If a single role fails
    /// (LLM error, budget exhausted, etc.) it produces an `abstain` receipt;
    /// it does NOT abort the whole batch.
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
    /// Daily budget cap (used by `would_exceed`). Defaults are generous so the
    /// gate is informational unless the operator tightens them.
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

        // Spawn one task per required role. Each task is fully self-contained
        // (owns its inputs) so we can join them concurrently.
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
            // Clone the small string fields we need into owned Strings so the
            // spawned task does not borrow from `pack`.
            let pack_id = pack.id.clone();
            let repo = pack.repo.clone();
            let head_sha = pack.head_sha.clone();
            let policy_sha = pack.policy_sha.clone();
            let target_branch = pack.target_branch.clone();
            let diff = diff_text.to_string();

            handles.push(tokio::spawn(async move {
                // 1. Budget gate — fires BEFORE the LLM router is called, so
                //    an exhausted ledger short-circuits with a synthetic
                //    abstain receipt and the router is never invoked.
                if ledger.would_exceed(&budget, ESTIMATED_REVIEWER_COST_MICRO_USD) {
                    let r = synth_abstain(
                        role,
                        &pack_id,
                        &head_sha,
                        &policy_sha,
                        "budget exhausted: would_exceed daily cap".to_string(),
                        &signing_key,
                    );
                    return (role, r);
                }

                // 2. Dispatch to the role-specific reviewer. Each arm maps
                //    its provider-specific error to a String so the outcome
                //    arms share a single type.
                let outcome: Result<AgentApprovalReceipt, String> = match role {
                    ReviewerRole::Security => {
                        let inputs = SecurityReviewInputs {
                            repo: &repo,
                            head_sha: &head_sha,
                            policy_sha: &policy_sha,
                            target_branch: &target_branch,
                            evidence_pack_id: &pack_id,
                            diff: &diff,
                            system_prompt_markdown: &prompt,
                            evidence_pack_json: None,
                        };
                        run_security_review(&router, &inputs)
                            .await
                            .map_err(|e| e.to_string())
                    }
                    ReviewerRole::TestIntegrity => {
                        let inputs = TestIntegrityReviewInputs {
                            repo: &repo,
                            head_sha: &head_sha,
                            policy_sha: &policy_sha,
                            target_branch: &target_branch,
                            evidence_pack_id: &pack_id,
                            diff: &diff,
                            system_prompt_markdown: &prompt,
                            evidence_pack_json: None,
                            signing_key: Some(&signing_key),
                        };
                        run_test_integrity_review(&router, &inputs)
                            .await
                            .map_err(|e| e.to_string())
                    }
                    ReviewerRole::Runtime => {
                        let inputs = RuntimeReviewInputs {
                            repo: &repo,
                            head_sha: &head_sha,
                            policy_sha: &policy_sha,
                            target_branch: &target_branch,
                            evidence_pack_id: &pack_id,
                            diff: &diff,
                            system_prompt_markdown: &prompt,
                            evidence_pack_json: None,
                            signing_key: Some(&signing_key),
                        };
                        run_runtime_review(&router, &inputs)
                            .await
                            .map_err(|e| e.to_string())
                    }
                    ReviewerRole::Lockfile => {
                        let inputs = LockfileReviewInputs {
                            repo: &repo,
                            head_sha: &head_sha,
                            policy_sha: &policy_sha,
                            target_branch: &target_branch,
                            evidence_pack_id: &pack_id,
                            diff: &diff,
                            system_prompt_markdown: &prompt,
                            evidence_pack_json: None,
                            signing_key: Some(&signing_key),
                        };
                        run_lockfile_review(&router, &inputs)
                            .await
                            .map_err(|e| e.to_string())
                    }
                    // Roles that this orchestrator does not run (judge,
                    // release_shepherd, nightwatch) become abstain receipts
                    // so the caller still sees an entry per required role.
                    other => {
                        let r = synth_abstain(
                            other,
                            &pack_id,
                            &head_sha,
                            &policy_sha,
                            format!("role {other:?} is not handled by ReviewerOrchestrator"),
                            &signing_key,
                        );
                        return (other, r);
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

                // 3. Record the spend so subsequent runs in the same process
                //    see updated usage. This is best-effort: the prompt-token
                //    count is what the provider reported.
                ledger.record(crate::llm::budget::TokenUsage {
                    prompt_tokens: receipt.tokens.prompt as u64,
                    completion_tokens: receipt.tokens.completion as u64,
                    estimated_micro_usd: ESTIMATED_REVIEWER_COST_MICRO_USD,
                });

                // 4. Ensure the receipt is signed with the real ed25519 key.
                //    `run_security_review` doesn't take a signing_key, so its
                //    receipt may carry the default unsigned signature; re-sign
                //    here. The unsigned algo marker stays as the wire value
                //    "stub" so existing refuse lists keep working (see
                //    `signing::Signature::default_unsigned`).
                if receipt.signature.algo == "stub" {
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
                    // We've lost the role for this slot (the panic ate the
                    // tuple). Synthesize a best-effort abstain for the first
                    // unfilled role. Security is the conservative default when
                    // every role has already been filled (no slots left to
                    // attribute the panic to); abstain on Security is the
                    // safest signal to the judge.
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
    /// Optional per-role artificial latency. Useful for the concurrency test.
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
        // Spawn each role-handling step concurrently so the concurrency test
        // can verify wall-clock parallelism.
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
                // 0ms is the documented default latency when the test hasn't
                // registered an artificial per-role delay.
                let sleep_ms = latencies.lock().unwrap().get(&role).copied().unwrap_or(0);
                if sleep_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
                }
                recorded.lock().unwrap().push(role);
                // Default to "not an error" when no error_on role is set.
                let is_error = error_on.lock().unwrap().map(|r| r == role).unwrap_or(false);
                if is_error {
                    // Synthetic abstain — no signing key in the fake; just
                    // a stub signature is fine (tests inspect role/reason).
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
        // Judge and ReleaseShepherd don't have ReviewerRoleId entries; fall
        // back to Security's id (only used for the prompt path, which the
        // synth_abstain path doesn't read).
        ReviewerRole::Judge | ReviewerRole::ReleaseShepherd => ReviewerRoleId::Security,
    }
}

fn agent_id_for(role: ReviewerRole) -> &'static str {
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
    let now = Utc::now();
    let ts = now.timestamp_millis();
    format!(
        "aar_{role:?}_{pack}_{ts}",
        role = role,
        pack = pack_id.chars().take(12).collect::<String>(),
        ts = ts
    )
}

/// Build an Abstain receipt synthesized by the orchestrator (i.e. NOT
/// produced by the per-role reviewer). Used for budget short-circuits and
/// post-failure recovery.
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
        signature: Signature::default_unsigned(),
    };
    r.signature = sign_canonical(&r, signing_key);
    r
}

/// Sign the canonical JSON projection of `r` (everything except the signature
/// itself, which would be circular). Matches `runner::sign_receipt`.
fn sign_canonical(r: &AgentApprovalReceipt, key: &EdSigningKey) -> Signature {
    let mut clone = r.clone();
    clone.signature = Signature::default_unsigned();
    // AgentApprovalReceipt is a plain struct with only serde-friendly fields;
    // serialization cannot fail in practice. Falling back to an empty body
    // would produce a valid-looking signature over no data, which is worse
    // than a panic — surface the impossible case loudly instead.
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
        signature: Signature::default_unsigned(),
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
        signature: Signature::default_unsigned(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::signing::EdVerifier;
    use crate::autonomy::types::{
        RiskTier, RollbackSection, RollbackStrategy, ScanOutcome, SecuritySection,
        SupplyChainSection, TestsSection,
    };
    use std::time::Instant;

    /// Canonical pack-builder, mirrors `conditions::tests::pack_with_security`.
    fn mint_pack() -> EvidencePack {
        EvidencePack {
            schema: SchemaTag::new(),
            id: "evp_orchestrator_test".into(),
            intent_id: None,
            repo: "org/proj".into(),
            source_branch: "agent/x".into(),
            target_branch: "main".into(),
            head_sha: "a".repeat(40),
            base_sha: "b".repeat(40),
            policy_sha: "c".repeat(40),
            author_agent: Some("builder.x".into()),
            risk: RiskTier::R2,
            changed_files: vec![],
            claims: vec![],
            tests: TestsSection {
                targeted: vec![],
                full_required: false,
                skipped: vec![],
                coverage_delta: None,
            },
            security: SecuritySection {
                sast: ScanOutcome::Passed,
                dependency_scan: ScanOutcome::Passed,
                secret_scan: ScanOutcome::Passed,
            },
            supply_chain: SupplyChainSection::default(),
            rollback: RollbackSection {
                strategy: RollbackStrategy::RevertCommit,
                feature_flag: None,
                data_migration_reversible: Some(true),
            },
            gate_receipts: vec![],
            evidence_digest: format!("sha256:{}", "0".repeat(64)),
            created_at: Utc::now(),
            signature: None,
        }
    }

    fn canned(role: ReviewerRole, decision: ReviewDecision) -> AgentApprovalReceipt {
        AgentApprovalReceipt {
            schema: SchemaTag::new(),
            id: format!("aar_canned_{role:?}"),
            evidence_pack_id: "evp_orchestrator_test".into(),
            role,
            agent_id: agent_id_for(role).into(),
            prompt_sha: Some("sha256:abc".into()),
            provider: Some("canned".into()),
            model: Some("canned-model".into()),
            temperature: Some(0.0),
            seed: None,
            raw_response_sha: Some("sha256:def".into()),
            head_sha: "a".repeat(40),
            policy_sha: "c".repeat(40),
            decision,
            reason: Some("canned".into()),
            findings: vec![],
            not_author: true,
            tokens: TokenCounts::default(),
            created_at: Utc::now(),
            signature: Signature::stub(),
        }
    }

    // ---- 1. Fake returns canned receipts -------------------------------

    #[tokio::test]
    async fn fake_orchestrator_returns_canned_receipts() {
        let orch = FakeReviewerOrchestrator::new().with_canned(
            ReviewerRole::Security,
            canned(ReviewerRole::Security, ReviewDecision::Block),
        );
        let pack = mint_pack();
        let out = orch
            .run_all(&pack, &[ReviewerRole::Security], "diff")
            .await
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, ReviewerRole::Security);
        assert!(matches!(out[0].decision, ReviewDecision::Block));
        assert_eq!(out[0].id, "aar_canned_Security");
    }

    // ---- 2. Fake records each required role ---------------------------

    #[tokio::test]
    async fn fake_orchestrator_records_each_required_role() {
        let orch = FakeReviewerOrchestrator::new();
        let pack = mint_pack();
        let roles = vec![
            ReviewerRole::Security,
            ReviewerRole::TestIntegrity,
            ReviewerRole::Runtime,
            ReviewerRole::Lockfile,
        ];
        let _ = orch.run_all(&pack, &roles, "diff").await.unwrap();
        let mut recorded = orch.recorded_calls.lock().unwrap().clone();
        recorded.sort_by_key(|r| format!("{r:?}"));
        let mut expected = roles.clone();
        expected.sort_by_key(|r| format!("{r:?}"));
        assert_eq!(recorded, expected);
    }

    // ---- 3. error_on returns abstain for that role --------------------

    #[tokio::test]
    async fn fake_orchestrator_error_on_returns_abstain_for_that_role() {
        let orch = FakeReviewerOrchestrator::new()
            .with_canned(
                ReviewerRole::TestIntegrity,
                canned(ReviewerRole::TestIntegrity, ReviewDecision::Pass),
            )
            .error_on(ReviewerRole::TestIntegrity);
        let pack = mint_pack();
        let out = orch
            .run_all(
                &pack,
                &[ReviewerRole::TestIntegrity, ReviewerRole::Runtime],
                "diff",
            )
            .await
            .unwrap();
        let ti = out
            .iter()
            .find(|r| r.role == ReviewerRole::TestIntegrity)
            .unwrap();
        assert!(matches!(ti.decision, ReviewDecision::Abstain));
        // Other role still returned a non-abstain default-pass receipt.
        let rt = out
            .iter()
            .find(|r| r.role == ReviewerRole::Runtime)
            .unwrap();
        assert!(matches!(rt.decision, ReviewDecision::Pass));
    }

    // ---- 4. Unknown role returns default Pass receipt -----------------

    #[tokio::test]
    async fn fake_orchestrator_unknown_role_returns_default_pass_receipt() {
        // No canned receipt registered for Lockfile; must get a default Pass.
        let orch = FakeReviewerOrchestrator::new();
        let pack = mint_pack();
        let out = orch
            .run_all(&pack, &[ReviewerRole::Lockfile], "diff")
            .await
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, ReviewerRole::Lockfile);
        assert!(matches!(out[0].decision, ReviewDecision::Pass));
        assert_eq!(out[0].agent_id, "reviewer-lockfile.v1");
    }

    // ---- 5. One receipt per required role -----------------------------

    #[tokio::test]
    async fn run_all_returns_one_receipt_per_required_role() {
        let orch = FakeReviewerOrchestrator::new();
        let pack = mint_pack();
        let roles = vec![
            ReviewerRole::Security,
            ReviewerRole::TestIntegrity,
            ReviewerRole::Runtime,
            ReviewerRole::Lockfile,
        ];
        let out = orch.run_all(&pack, &roles, "diff").await.unwrap();
        assert_eq!(out.len(), roles.len(), "exactly one receipt per role");
    }

    // ---- 6. Empty required_roles → empty Vec --------------------------

    #[tokio::test]
    async fn run_all_empty_required_roles_returns_empty_vec() {
        let orch = FakeReviewerOrchestrator::new();
        let pack = mint_pack();
        let out = orch.run_all(&pack, &[], "diff").await.unwrap();
        assert!(out.is_empty());
    }

    // ---- 7. Concurrent reviewers complete in parallel ----------------

    #[tokio::test]
    async fn run_all_with_concurrent_roles_completes_all_in_parallel() {
        let orch = FakeReviewerOrchestrator::new()
            .with_latency(ReviewerRole::Security, 50)
            .with_latency(ReviewerRole::TestIntegrity, 50)
            .with_latency(ReviewerRole::Runtime, 50)
            .with_latency(ReviewerRole::Lockfile, 50);
        let pack = mint_pack();
        let roles = vec![
            ReviewerRole::Security,
            ReviewerRole::TestIntegrity,
            ReviewerRole::Runtime,
            ReviewerRole::Lockfile,
        ];
        let started = Instant::now();
        let out = orch.run_all(&pack, &roles, "diff").await.unwrap();
        let elapsed = started.elapsed();
        assert_eq!(out.len(), 4);
        assert!(
            elapsed.as_millis() < 200,
            "4 x 50ms reviewers should run concurrently; took {elapsed:?}"
        );
    }

    // ---- 8. Production orchestrator + exhausted budget → all abstain --

    #[tokio::test]
    async fn production_orchestrator_with_exhausted_budget_returns_abstain_for_all_roles() {
        // Fresh ledger pre-loaded with usage that already exceeds the cap.
        let ledger = Arc::new(BudgetLedger::new());
        ledger.record(crate::llm::budget::TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            estimated_micro_usd: 10_000_000,
        });
        let key = Arc::new(EdSigningKey::from_seed("orchestrator.test", [1u8; 32]));
        // The router is unreachable: the budget gate fires BEFORE dispatch.
        let router = Arc::new(LlmRouter::new());
        let orch = ProductionReviewerOrchestrator::new(
            router,
            ledger,
            PathBuf::from("/tmp/does-not-exist"),
            key,
        )
        .with_budget(Budget {
            daily_micro_usd_cap: 1_000, // Already exceeded by recorded usage.
            per_pr_micro_usd_cap: 500,
        });
        let pack = mint_pack();
        let roles = vec![
            ReviewerRole::Security,
            ReviewerRole::TestIntegrity,
            ReviewerRole::Runtime,
            ReviewerRole::Lockfile,
        ];
        let out = orch.run_all(&pack, &roles, "diff").await.unwrap();
        assert_eq!(out.len(), 4);
        for r in &out {
            assert!(
                matches!(r.decision, ReviewDecision::Abstain),
                "exhausted budget should abstain {:?}, got {:?}",
                r.role,
                r.decision
            );
            assert!(
                r.reason
                    .as_deref()
                    .unwrap_or("")
                    .contains("budget exhausted"),
                "abstain reason should explain budget: {:?}",
                r.reason
            );
        }
    }

    // ---- 9. Production orchestrator constructs ------------------------

    #[tokio::test]
    async fn production_orchestrator_construct_with_required_fields() {
        let router = Arc::new(LlmRouter::new());
        let ledger = Arc::new(BudgetLedger::new());
        let key = Arc::new(EdSigningKey::from_seed("orchestrator.test", [2u8; 32]));
        let orch = ProductionReviewerOrchestrator::new(
            router.clone(),
            ledger.clone(),
            PathBuf::from(".jeryu/autonomy"),
            key.clone(),
        );
        assert!(Arc::ptr_eq(&orch.router, &router));
        assert!(Arc::ptr_eq(&orch.budget_ledger, &ledger));
        assert!(Arc::ptr_eq(&orch.signing_key, &key));
        assert_eq!(orch.autonomy_dir, PathBuf::from(".jeryu/autonomy"));
    }

    // ---- 10. Synthesized abstain carries correct role ----------------

    #[tokio::test]
    async fn abstain_receipt_for_role_carries_role_field() {
        let ledger = Arc::new(BudgetLedger::new());
        ledger.record(crate::llm::budget::TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            estimated_micro_usd: 10_000_000,
        });
        let key = Arc::new(EdSigningKey::from_seed("orchestrator.test", [3u8; 32]));
        let router = Arc::new(LlmRouter::new());
        let orch = ProductionReviewerOrchestrator::new(
            router,
            ledger,
            PathBuf::from("/tmp/does-not-exist"),
            key,
        )
        .with_budget(Budget {
            daily_micro_usd_cap: 1_000,
            per_pr_micro_usd_cap: 500,
        });
        let pack = mint_pack();
        for role in [
            ReviewerRole::Security,
            ReviewerRole::TestIntegrity,
            ReviewerRole::Runtime,
            ReviewerRole::Lockfile,
        ] {
            let out = orch.run_all(&pack, &[role], "diff").await.unwrap();
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].role, role, "abstain receipt role must match request");
        }
    }

    // ---- 11. Abstain receipt signature verifies under ed25519 --------

    #[tokio::test]
    async fn abstain_receipt_for_role_signature_is_valid_ed25519() {
        let ledger = Arc::new(BudgetLedger::new());
        ledger.record(crate::llm::budget::TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            estimated_micro_usd: 10_000_000,
        });
        let key = Arc::new(EdSigningKey::from_seed("orchestrator.test", [4u8; 32]));
        let verifier = key.verifier();
        let router = Arc::new(LlmRouter::new());
        let orch = ProductionReviewerOrchestrator::new(
            router,
            ledger,
            PathBuf::from("/tmp/does-not-exist"),
            key,
        )
        .with_budget(Budget {
            daily_micro_usd_cap: 1_000,
            per_pr_micro_usd_cap: 500,
        });
        let pack = mint_pack();
        let out = orch
            .run_all(&pack, &[ReviewerRole::Security], "diff")
            .await
            .unwrap();
        assert_eq!(out.len(), 1);
        let r = &out[0];
        assert_eq!(r.signature.algo, "ed25519");
        // Reconstruct the canonical body (signature stubbed) and verify.
        let mut clone = r.clone();
        clone.signature = Signature::stub();
        let body = serde_json::to_string(&clone).unwrap();
        assert!(
            verifier.verify(body.as_bytes(), &r.signature),
            "abstain receipt signature must verify under the orchestrator's ed25519 key"
        );
    }

    // ---- 12. Receipt's evidence_pack_id matches input pack ------------

    #[tokio::test]
    async fn receipt_evidence_pack_id_matches_input_pack() {
        let ledger = Arc::new(BudgetLedger::new());
        ledger.record(crate::llm::budget::TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            estimated_micro_usd: 10_000_000,
        });
        let key = Arc::new(EdSigningKey::from_seed("orchestrator.test", [5u8; 32]));
        let router = Arc::new(LlmRouter::new());
        let orch = ProductionReviewerOrchestrator::new(
            router,
            ledger,
            PathBuf::from("/tmp/does-not-exist"),
            key,
        )
        .with_budget(Budget {
            daily_micro_usd_cap: 1_000,
            per_pr_micro_usd_cap: 500,
        });
        let pack = mint_pack();
        let out = orch
            .run_all(
                &pack,
                &[ReviewerRole::Security, ReviewerRole::TestIntegrity],
                "diff",
            )
            .await
            .unwrap();
        for r in &out {
            assert_eq!(
                r.evidence_pack_id, pack.id,
                "receipt must bind to the input pack id"
            );
        }
    }

    // ---- 13. Receipt SHAs match pack so judge accepts them ------------

    #[tokio::test]
    async fn receipt_head_sha_and_policy_sha_match_pack() {
        let ledger = Arc::new(BudgetLedger::new());
        ledger.record(crate::llm::budget::TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            estimated_micro_usd: 10_000_000,
        });
        let key = Arc::new(EdSigningKey::from_seed("orchestrator.test", [6u8; 32]));
        let router = Arc::new(LlmRouter::new());
        let orch = ProductionReviewerOrchestrator::new(
            router,
            ledger,
            PathBuf::from("/tmp/does-not-exist"),
            key,
        )
        .with_budget(Budget {
            daily_micro_usd_cap: 1_000,
            per_pr_micro_usd_cap: 500,
        });
        let pack = mint_pack();
        let out = orch
            .run_all(
                &pack,
                &[
                    ReviewerRole::Security,
                    ReviewerRole::TestIntegrity,
                    ReviewerRole::Runtime,
                    ReviewerRole::Lockfile,
                ],
                "diff",
            )
            .await
            .unwrap();
        for r in &out {
            assert_eq!(r.head_sha, pack.head_sha, "head_sha must match pack");
            assert_eq!(r.policy_sha, pack.policy_sha, "policy_sha must match pack");
        }
    }

    // ---- 14. not_author flag is true on all synthesized receipts -----

    #[tokio::test]
    async fn not_author_flag_is_true_on_all_synthesized_receipts() {
        let ledger = Arc::new(BudgetLedger::new());
        ledger.record(crate::llm::budget::TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            estimated_micro_usd: 10_000_000,
        });
        let key = Arc::new(EdSigningKey::from_seed("orchestrator.test", [7u8; 32]));
        let router = Arc::new(LlmRouter::new());
        let orch = ProductionReviewerOrchestrator::new(
            router,
            ledger,
            PathBuf::from("/tmp/does-not-exist"),
            key,
        )
        .with_budget(Budget {
            daily_micro_usd_cap: 1_000,
            per_pr_micro_usd_cap: 500,
        });
        let pack = mint_pack();
        let out = orch
            .run_all(
                &pack,
                &[
                    ReviewerRole::Security,
                    ReviewerRole::TestIntegrity,
                    ReviewerRole::Runtime,
                    ReviewerRole::Lockfile,
                ],
                "diff",
            )
            .await
            .unwrap();
        for r in &out {
            assert!(
                r.not_author,
                "synthesized receipt for {:?} must set not_author=true",
                r.role
            );
        }
    }

    // ---- Sanity: EdVerifier import isn't dead -------------------------

    #[test]
    fn _ed_verifier_import_used() {
        let k = EdSigningKey::from_seed("k", [0u8; 32]);
        let v: EdVerifier = k.verifier();
        let sig = k.sign_raw(b"x");
        assert!(v.verify(b"x", &sig));
    }
}
