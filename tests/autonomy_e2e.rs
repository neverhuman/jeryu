//! Phase 0-3 end-to-end integration test.
//!
//! Synthesizes an Evidence Pack, runs the security reviewer with a mock
//! provider, fuses the receipt through the Judge against the real
//! `.jeryu/autonomy/policies/` bundle, and asserts the verdict shape.
//!
//! Runs in the normal `cargo test` profile (no live LLM, no network).

use async_trait::async_trait;
use jeryu::agent_review::{
    JudgeInputs, judge, run_security_review, security::SecurityReviewInputs,
};
use jeryu::autonomy::types::{
    ChangedFile, GateDecision, RiskTier, RollbackSection, RollbackStrategy, ScanOutcome,
    SecuritySection, SupplyChainSection, TestsSection,
};
use jeryu::autonomy::{EvidenceInputs, PolicyBundle, build_evidence_pack, signing::Signature};
use jeryu::llm::{
    CallParams, CallResponse, ChatMessage, DataUse, LlmError, LlmProvider, LlmRouter, RoleChain,
    RoleChainEntry,
};
use std::sync::Arc;

struct MockProvider {
    payload: String,
}

#[async_trait]
impl LlmProvider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }
    fn data_use(&self) -> DataUse {
        DataUse::NoTrain
    }
    async fn call(&self, _m: &[ChatMessage], _p: &CallParams) -> Result<CallResponse, LlmError> {
        Ok(CallResponse {
            provider: "mock".into(),
            model: "mock-1".into(),
            content: self.payload.clone(),
            prompt_tokens: Some(100),
            completion_tokens: Some(40),
            raw_response_sha: "sha256:beef".into(),
            latency_ms: 1,
        })
    }
}

fn router_returning(payload: &str) -> LlmRouter {
    let mock = Arc::new(MockProvider {
        payload: payload.into(),
    });
    let mut chain = RoleChain {
        role: "reviewer-security".into(),
        entries: vec![],
        forbid_train_on_input: false,
    };
    chain.entries.push(RoleChainEntry {
        provider: mock,
        params: CallParams::default(),
    });
    let mut r = LlmRouter::new();
    r.add_chain(chain);
    r
}

fn load_real_policies() -> PolicyBundle {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".jeryu/autonomy/policies");
    PolicyBundle::from_dir(&dir).expect("loads real .jeryu/autonomy/policies/")
}

fn synth_pack(risk: RiskTier, signed: bool) -> jeryu::autonomy::EvidencePack {
    let mut pack = build_evidence_pack(EvidenceInputs {
        repo: "jeryu/e2e",
        source_branch: "agent/test",
        target_branch: "main",
        head_sha: "a".repeat(40).leak(),
        base_sha: "b".repeat(40).leak(),
        policy_sha: "c".repeat(40).leak(),
        author_agent: Some("builder.e2e"),
        intent_id: None,
        risk,
        changed_files: vec![ChangedFile {
            path: "src/api/users.rs".into(),
            risk_tags: vec!["auth".into()],
            lines_added: 12,
            lines_removed: 4,
        }],
        claims: vec!["fix lookup bug".into()],
        tests: TestsSection {
            targeted: vec!["users::lookup".into()],
            full_required: false,
            skipped: vec![],
            coverage_delta: Some(0.1),
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
    });
    if signed {
        // Use the real ed25519 algo so the judge's
        // `evidence_signature_invalid` condition accepts the pack.
        pack.signature = Some(Signature {
            key_id: "evidence-builder.v1".into(),
            algo: "ed25519".into(),
            value: "0".repeat(128),
        });
    }
    pack
}

const SQL_INJECTION_DIFF: &str = "diff --git a/src/api/users.rs b/src/api/users.rs
@@ -38,7 +38,7 @@
-    let users = sqlx::query_as!(User, \"SELECT id, name FROM users WHERE name = $1\", req.name).fetch_all(pool).await?;
+    let q = format!(\"SELECT id, name FROM users WHERE name = '{}'\", req.name);
+    let users: Vec<User> = sqlx::query_as(&q).fetch_all(pool).await?;
";

const CLEAN_DIFF: &str = "diff --git a/docs/intro.md b/docs/intro.md
@@ -1,3 +1,4 @@
 # Intro
+welcome line added
";

const BLOCK_RESPONSE: &str = r#"{"role":"security","decision":"block","reason":"SQL string interpolation","findings":[{"severity":"critical","class":"injection-sql","file":"src/api/users.rs","range":[39,40],"evidence":"format!(\"SELECT … WHERE name = '{}'\", req.name)","recommendation":"Use bind parameters."}]}"#;
const PASS_RESPONSE: &str =
    r#"{"role":"security","decision":"pass","reason":"docs-only change","findings":[]}"#;

#[tokio::test]
async fn sqli_diff_lands_reject_through_full_pipeline() {
    let policies = load_real_policies();
    let pack = synth_pack(RiskTier::R2, true);
    let router = router_returning(BLOCK_RESPONSE);

    let inputs = SecurityReviewInputs {
        repo: "jeryu/e2e",
        head_sha: &pack.head_sha,
        policy_sha: &pack.policy_sha,
        target_branch: "main",
        evidence_pack_id: &pack.id,
        diff: SQL_INJECTION_DIFF,
        system_prompt_markdown: "Reviewer-security.v1.",
        evidence_pack_json: None,
    };
    let security_receipt = run_security_review(&router, &inputs).await.expect("review");

    // The test pipeline supplies only the security receipt. R2 requires
    // {security, test_integrity}. Without test_integrity, quorum is
    // insufficient → RequireHuman is the most lenient possible outcome.
    // But the security receipt blocked, so the hard_stop `reviewer_blocked`
    // wins and the verdict is Reject.
    let outcome = judge(JudgeInputs {
        pack: &pack,
        receipts: std::slice::from_ref(&security_receipt),
        policy: &policies,
        repo: "jeryu/e2e",
        target_branch: "main",
        merge_request: Some("!42"),
        author_agent: Some("builder.e2e"),
        external_hard_stops: &[],
    });
    assert_eq!(outcome.verdict.decision, GateDecision::Reject);
    assert!(
        outcome
            .verdict
            .hard_stops
            .iter()
            .any(|h| h == "reviewer_blocked")
    );
    assert_eq!(outcome.verdict.head_sha, pack.head_sha);
    assert_eq!(outcome.verdict.evidence_pack_id, pack.id);
    eprintln!(
        "verdict: {}",
        serde_json::to_string_pretty(&outcome.verdict).unwrap()
    );
}

#[tokio::test]
async fn docs_only_with_pass_lands_allow_merge_at_r0() {
    let policies = load_real_policies();
    let mut pack = synth_pack(RiskTier::R0, true);
    pack.changed_files = vec![ChangedFile {
        path: "docs/intro.md".into(),
        risk_tags: vec![],
        lines_added: 1,
        lines_removed: 0,
    }];

    // R0 quorum needs 0 approvals; we run no reviewers and expect AllowMerge.
    let outcome = judge(JudgeInputs {
        pack: &pack,
        receipts: &[],
        policy: &policies,
        repo: "jeryu/e2e",
        target_branch: "main",
        merge_request: Some("!docs"),
        author_agent: Some("builder.e2e"),
        external_hard_stops: &[],
    });
    assert_eq!(outcome.verdict.decision, GateDecision::AllowMerge);
    assert!(outcome.verdict.hard_stops.is_empty());
}

#[tokio::test]
async fn pass_only_security_at_r2_requires_human_due_to_missing_test_integrity() {
    let policies = load_real_policies();
    let pack = synth_pack(RiskTier::R2, true);
    let router = router_returning(PASS_RESPONSE);
    let inputs = SecurityReviewInputs {
        repo: "jeryu/e2e",
        head_sha: &pack.head_sha,
        policy_sha: &pack.policy_sha,
        target_branch: "main",
        evidence_pack_id: &pack.id,
        diff: CLEAN_DIFF,
        system_prompt_markdown: "Reviewer-security.v1.",
        evidence_pack_json: None,
    };
    let receipt = run_security_review(&router, &inputs).await.expect("review");
    let outcome = judge(JudgeInputs {
        pack: &pack,
        receipts: &[receipt],
        policy: &policies,
        repo: "jeryu/e2e",
        target_branch: "main",
        merge_request: Some("!1"),
        author_agent: Some("builder.e2e"),
        external_hard_stops: &[],
    });
    // R2 needs {security, test_integrity}; only security present → require_human.
    assert_eq!(outcome.verdict.decision, GateDecision::RequireHuman);
}

#[tokio::test]
async fn fail_closed_on_unsigned_evidence_pack() {
    let policies = load_real_policies();
    let pack = synth_pack(RiskTier::R2, false); // unsigned
    let outcome = judge(JudgeInputs {
        pack: &pack,
        receipts: &[],
        policy: &policies,
        repo: "jeryu/e2e",
        target_branch: "main",
        merge_request: None,
        author_agent: None,
        external_hard_stops: &[],
    });
    assert_eq!(outcome.verdict.decision, GateDecision::Reject);
    assert!(
        outcome
            .verdict
            .hard_stops
            .iter()
            .any(|n| n == "evidence_signature_invalid")
    );
}
