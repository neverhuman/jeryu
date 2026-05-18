#![allow(clippy::field_reassign_with_default)]
//! Live end-to-end test: real LLM reviewer → real policy bundle → real judge.
//!
//! Proves the Evidence Gate spine works against a real provider (not a mock).
//! Gated on `JERYU_LLM_LIVE=1`. Uses `OPENROUTER_API_KEY` from `~/llm.env`.

use jeryu::agent_review::{
    JudgeInputs, judge, run_security_review, security::SecurityReviewInputs,
};
use jeryu::autonomy::types::{
    ChangedFile, GateDecision, RiskTier, RollbackSection, RollbackStrategy, ScanOutcome,
    SecuritySection, SupplyChainSection, TestsSection,
};
use jeryu::autonomy::{EvidenceInputs, PolicyBundle, build_evidence_pack, signing::Signature};
use jeryu::llm::{
    CallParams, DataUse, LlmRouter, OpenAiCompatibleClient, RoleChain, RoleChainEntry,
    SecretResolver, resolve_secret,
};
use std::sync::Arc;

const SQL_INJECTION_DIFF: &str = r#"diff --git a/src/api/users.rs b/src/api/users.rs
--- a/src/api/users.rs
+++ b/src/api/users.rs
@@ -38,7 +38,12 @@ pub async fn lookup_by_name(pool: &PgPool, req: LookupReq) -> Result<Vec<User>>
-    let users = sqlx::query_as!(User, "SELECT id, name FROM users WHERE name = $1", req.name)
-        .fetch_all(pool)
-        .await?;
+    // PATCH: skip bind parameter for "performance"
+    let q = format!("SELECT id, name FROM users WHERE name = '{}'", req.name);
+    let users: Vec<User> = sqlx::query_as(&q)
+        .fetch_all(pool)
+        .await?;
     Ok(users)
 }
"#;

fn build_live_router() -> LlmRouter {
    let resolver = SecretResolver::from_env();
    let key = resolve_secret("OPENROUTER_API_KEY", &resolver).expect("OPENROUTER_API_KEY required");
    let client = OpenAiCompatibleClient::new("openrouter", "https://openrouter.ai/api/v1")
        .with_api_key(key.value)
        .with_header("HTTP-Referer", "https://github.com/jeryu/jeryu")
        .with_header("X-Title", "jeryu-evidence-gate-live-e2e")
        .with_data_use(DataUse::NoTrain);
    let client_arc = Arc::new(client);
    let mut primary = CallParams::default();
    primary.model = "nvidia/nemotron-3-super-120b-a12b:free".into();
    primary.temperature = 0.0;
    primary.max_tokens = 600;
    primary.timeout_ms = 60_000;
    let mut fallback = CallParams::default();
    fallback.model = "openai/gpt-oss-120b:free".into();
    fallback.temperature = 0.0;
    fallback.max_tokens = 600;
    fallback.timeout_ms = 60_000;
    let mut chain = RoleChain {
        role: "reviewer-security".into(),
        entries: vec![],
        forbid_train_on_input: true,
    };
    chain.entries.push(RoleChainEntry {
        provider: client_arc.clone(),
        params: primary,
    });
    chain.entries.push(RoleChainEntry {
        provider: client_arc,
        params: fallback,
    });
    let mut r = LlmRouter::new();
    r.add_chain(chain);
    r
}

fn synth_signed_pack() -> jeryu::autonomy::EvidencePack {
    let mut pack = build_evidence_pack(EvidenceInputs {
        repo: "jeryu/live-e2e",
        source_branch: "agent/sqli-fix",
        target_branch: "main",
        head_sha: "a".repeat(40).leak(),
        base_sha: "b".repeat(40).leak(),
        policy_sha: "c".repeat(40).leak(),
        author_agent: Some("builder.live-e2e"),
        intent_id: None,
        risk: RiskTier::R2,
        changed_files: vec![ChangedFile {
            path: "src/api/users.rs".into(),
            risk_tags: vec!["auth".into()],
            lines_added: 6,
            lines_removed: 3,
        }],
        claims: vec!["fix lookup for fast-path callers".into()],
        tests: TestsSection {
            targeted: vec!["users::lookup_by_name".into()],
            full_required: false,
            skipped: vec![],
            coverage_delta: Some(-0.01),
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
        legacy_receipts: vec![],
    });
    pack.signature = Some(Signature {
        key_id: "evidence-builder.v1".into(),
        algo: "ed25519".into(),
        value: "0".repeat(128),
    });
    pack
}

#[tokio::test]
#[ignore = "live LLM call; set JERYU_LLM_LIVE=1 to run"]
async fn full_spine_live_sqli_lands_reject() {
    if std::env::var("JERYU_LLM_LIVE").as_deref() != Ok("1") {
        eprintln!("JERYU_LLM_LIVE not set; skipping");
        return;
    }

    let router = build_live_router();
    let pack = synth_signed_pack();
    let policies = PolicyBundle::from_dir(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".autonomy/policies"),
    )
    .expect("loads policies");
    let prompt = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".autonomy/prompts/reviewer-security.md"),
    )
    .expect("loads system prompt");

    let inputs = SecurityReviewInputs {
        repo: "jeryu/live-e2e",
        head_sha: &pack.head_sha,
        policy_sha: &pack.policy_sha,
        target_branch: "main",
        evidence_pack_id: &pack.id,
        diff: SQL_INJECTION_DIFF,
        system_prompt_markdown: &prompt,
        evidence_pack_json: None,
    };

    eprintln!("[live-e2e] calling reviewer...");
    let receipt = run_security_review(&router, &inputs)
        .await
        .expect("reviewer");
    eprintln!("[live-e2e] reviewer decision: {:?}", receipt.decision);
    eprintln!(
        "[live-e2e] reviewer findings: {} item(s)",
        receipt.findings.len()
    );

    eprintln!("[live-e2e] judging...");
    let outcome = judge(JudgeInputs {
        pack: &pack,
        receipts: std::slice::from_ref(&receipt),
        policy: &policies,
        repo: "jeryu/live-e2e",
        target_branch: "main",
        merge_request: Some("!live-e2e"),
        author_agent: Some("builder.live-e2e"),
        external_hard_stops: &[],
    });
    eprintln!("[live-e2e] verdict: {:?}", outcome.verdict.decision);
    eprintln!("[live-e2e] hard_stops: {:?}", outcome.verdict.hard_stops);
    eprintln!(
        "[live-e2e] full verdict JSON:\n{}",
        serde_json::to_string_pretty(&outcome.verdict).unwrap()
    );

    // The reviewer should have flagged the SQLi; the judge should Reject
    // because of `reviewer_blocked` hard-stop. Even if the reviewer downgrades
    // to Concern, R2 missing `test_integrity` still escalates to RequireHuman,
    // which is also acceptable (never AllowMerge for an SQLi diff).
    assert_ne!(
        outcome.verdict.decision,
        GateDecision::AllowMerge,
        "an SQLi diff must never be auto-merged"
    );
}
