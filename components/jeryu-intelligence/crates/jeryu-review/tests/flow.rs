//! End-to-end: the Production orchestrator collects signed receipts from a
//! deterministic LLM-backed reviewer set (real prompt assets), then the judge
//! fuses them into a verdict. Exercises the collect → SHA-bind → quorum →
//! verdict path against the public crate surface.

mod common;

use async_trait::async_trait;
use common::{assets_prompts_dir, pack_with};
use jeryu_review::judge::{JudgeInputs, judge};
use jeryu_review::llm::{
    BudgetLedger, CallParams, CallResponse, ChatMessage, DataUse, LlmError, LlmProvider, LlmRouter,
    RoleChain, RoleChainEntry,
};
use jeryu_review::orchestrator::{ProductionReviewerOrchestrator, ReviewerOrchestrator};
use jeryu_review::policy::PolicyBundle;
use jeryu_review::schema::{GateDecision, ReviewerRole, RiskTier, ScanOutcome};
use jeryu_review::signing::EdSigningKey;
use std::sync::Arc;

/// Deterministic provider that returns a fixed receipt JSON for every call.
struct CannedProvider {
    payload: String,
}

#[async_trait]
impl LlmProvider for CannedProvider {
    fn id(&self) -> &str {
        "canned"
    }
    fn data_use(&self) -> DataUse {
        DataUse::NoTrain
    }
    async fn call(&self, _m: &[ChatMessage], _p: &CallParams) -> Result<CallResponse, LlmError> {
        Ok(CallResponse {
            provider: "canned".into(),
            model: "canned-model".into(),
            content: self.payload.clone(),
            prompt_tokens: Some(10),
            completion_tokens: Some(5),
            raw_response_sha: "sha256:canned".into(),
            latency_ms: 1,
        })
    }
}

fn router_passing() -> Arc<LlmRouter> {
    let mut router = LlmRouter::new();
    for role in [
        "reviewer-security",
        "reviewer-test-integrity",
        "reviewer-runtime",
        "reviewer-lockfile",
    ] {
        let mut chain = RoleChain {
            role: role.into(),
            entries: vec![],
            forbid_train_on_input: false,
        };
        chain.entries.push(RoleChainEntry {
            provider: Arc::new(CannedProvider {
                payload: r#"{"role":"x","decision":"pass","reason":"looks fine"}"#.into(),
            }),
            params: CallParams::default(),
        });
        router.add_chain(chain);
    }
    Arc::new(router)
}

#[tokio::test]
async fn orchestrator_collects_signed_receipts_and_judge_allows_merge() {
    let router = router_passing();
    let ledger = Arc::new(BudgetLedger::new());
    let key = Arc::new(EdSigningKey::from_seed("orchestrator.flow", [9u8; 32]));
    let orch = ProductionReviewerOrchestrator::new(router, ledger, assets_prompts_dir(), key);

    let pack = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let roles = vec![ReviewerRole::Security, ReviewerRole::TestIntegrity];
    let receipts = orch.run_all(&pack, &roles, "+ fn add() {}").await.unwrap();

    assert_eq!(receipts.len(), 2);
    // Every receipt records the replay anchors and binds to the pack.
    for r in &receipts {
        assert!(r.prompt_sha.is_some(), "prompt_sha must be recorded");
        assert!(r.provider.is_some());
        assert!(r.raw_response_sha.is_some());
        assert_eq!(r.head_sha, pack.head_sha);
        assert_eq!(r.signature.algo, "ed25519");
    }

    let policy = PolicyBundle::default_enforcing();
    let out = judge(JudgeInputs {
        pack: &pack,
        receipts: &receipts,
        policy: &policy,
        repo: "org/proj",
        target_branch: "main",
        pull_request: Some("pr-7"),
        author_agent: Some("builder.x"),
        external_hard_stops: &[],
    });
    assert_eq!(out.verdict.decision, GateDecision::AllowMerge);
    assert!(out.verdict.hard_stops.is_empty());
    assert_eq!(out.dropped_receipts.len(), 0);
}

#[tokio::test]
async fn orchestrator_block_from_one_reviewer_rejects_via_judge() {
    // Security returns block; test-integrity passes.
    let mut router = LlmRouter::new();
    let mut sec = RoleChain {
        role: "reviewer-security".into(),
        entries: vec![],
        forbid_train_on_input: false,
    };
    sec.entries.push(RoleChainEntry {
        provider: Arc::new(CannedProvider {
            payload: r#"{"role":"security","decision":"block","reason":"authz bypass"}"#.into(),
        }),
        params: CallParams::default(),
    });
    router.add_chain(sec);
    let mut ti = RoleChain {
        role: "reviewer-test-integrity".into(),
        entries: vec![],
        forbid_train_on_input: false,
    };
    ti.entries.push(RoleChainEntry {
        provider: Arc::new(CannedProvider {
            payload: r#"{"role":"test_integrity","decision":"pass"}"#.into(),
        }),
        params: CallParams::default(),
    });
    router.add_chain(ti);

    let orch = ProductionReviewerOrchestrator::new(
        Arc::new(router),
        Arc::new(BudgetLedger::new()),
        assets_prompts_dir(),
        Arc::new(EdSigningKey::from_seed("orchestrator.flow", [10u8; 32])),
    );
    let pack = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let receipts = orch
        .run_all(
            &pack,
            &[ReviewerRole::Security, ReviewerRole::TestIntegrity],
            "+ fn x() {}",
        )
        .await
        .unwrap();

    let policy = PolicyBundle::default_enforcing();
    let out = judge(JudgeInputs::new(
        &pack, &receipts, &policy, "org/proj", "main",
    ));
    assert_eq!(out.verdict.decision, GateDecision::Reject);
    assert!(
        out.verdict
            .hard_stops
            .iter()
            .any(|n| n == "reviewer_blocked")
    );
}
