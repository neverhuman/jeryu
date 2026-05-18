//! Additional coverage tests for edge cases not hit by the per-module tests.
//!
//! Focus areas:
//!   - LLM router behavior when all providers fail
//!   - SchemaTag rejecting bad schema ids
//!   - Conditions registry: every named condition can be looked up
//!   - Evidence Pack: signature absent/present/stub variants
//!   - Glob compiler: tricky characters

use async_trait::async_trait;
use jeryu::autonomy::{
    EvidenceInputs, build_evidence_pack,
    conditions::ConditionRegistry,
    risk::compile_glob,
    signing::{Signature, SigningKey, sha256_digest},
    types::{
        AgentApprovalReceipt, ChangedFile, GateDecision, IntentCard, ReviewerRole, RiskTier,
        RollbackSection, RollbackStrategy, ScanOutcome, SchemaTag, SecuritySection,
        SupplyChainSection, TestsSection, VibeGateVerdict,
    },
};
use jeryu::llm::{
    CallParams, CallResponse, ChatMessage, DataUse, LlmError, LlmProvider, LlmRouter, RoleChain,
    RoleChainEntry,
};
use std::sync::Arc;

struct AlwaysFails(LlmError);

#[async_trait]
impl LlmProvider for AlwaysFails {
    fn id(&self) -> &str {
        "always-fails"
    }
    fn data_use(&self) -> DataUse {
        DataUse::NoTrain
    }
    async fn call(&self, _m: &[ChatMessage], _p: &CallParams) -> Result<CallResponse, LlmError> {
        Err(match &self.0 {
            LlmError::RateLimited { retry_after_ms } => LlmError::RateLimited {
                retry_after_ms: *retry_after_ms,
            },
            LlmError::Transient(s) => LlmError::Transient(s.clone()),
            LlmError::Permanent(s) => LlmError::Permanent(s.clone()),
            LlmError::Auth => LlmError::Auth,
            LlmError::Parse(s) => LlmError::Parse(s.clone()),
            LlmError::BudgetExhausted(s) => LlmError::BudgetExhausted(s.clone()),
            LlmError::PolicyViolation(s) => LlmError::PolicyViolation(s.clone()),
        })
    }
}

#[tokio::test]
async fn router_propagates_last_error_when_all_fail() {
    let a = Arc::new(AlwaysFails(LlmError::RateLimited { retry_after_ms: 1 }));
    let b = Arc::new(AlwaysFails(LlmError::Transient("network".into())));
    let mut chain = RoleChain {
        role: "r".into(),
        entries: vec![],
        forbid_train_on_input: false,
    };
    chain.entries.push(RoleChainEntry {
        provider: a,
        params: CallParams::default(),
    });
    chain.entries.push(RoleChainEntry {
        provider: b,
        params: CallParams::default(),
    });
    let mut router = LlmRouter::new();
    router.add_chain(chain);
    let err = router.dispatch("r", &[]).await.unwrap_err();
    assert!(matches!(err, LlmError::Transient(_)));
}

#[tokio::test]
async fn router_unknown_role_returns_permanent_error() {
    let r = LlmRouter::new();
    let err = r.dispatch("not-configured", &[]).await.unwrap_err();
    assert!(matches!(err, LlmError::Permanent(_)));
}

#[test]
fn schema_tag_round_trips_via_serde_json() {
    let s: SchemaTag<jeryu::autonomy::types::IntentCardTag> = SchemaTag::new();
    let j = serde_json::to_string(&s).unwrap();
    assert_eq!(j, "\"vibegate.intent_card.v1\"");
    let back: SchemaTag<jeryu::autonomy::types::IntentCardTag> = serde_json::from_str(&j).unwrap();
    let _ = back;
}

#[test]
fn schema_tag_rejects_wrong_tag() {
    let j = "\"vibegate.totally_wrong.v1\"";
    let err =
        serde_json::from_str::<SchemaTag<jeryu::autonomy::types::EvidencePackTag>>(j).unwrap_err();
    assert!(err.to_string().contains("schema mismatch"));
}

#[test]
fn conditions_registry_lookup_finds_every_registered_name() {
    let r = ConditionRegistry::default();
    for name in r.names() {
        assert!(
            r.lookup(name).is_some(),
            "name {name} should be in registry"
        );
    }
}

#[test]
fn conditions_registry_unknown_name_produces_fail_closed_entry() {
    let r = ConditionRegistry::default();
    let pack = synth_pack(RiskTier::R0, true);
    let hits = r.evaluate(&["never_defined_42".into()], &pack, &[]);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].name.starts_with("unknown_condition:"));
}

#[test]
fn signing_key_verify_round_trip() {
    let k = SigningKey::new("k1", b"secret".to_vec());
    let sig = k.sign(b"payload");
    assert!(k.verify(b"payload", &sig));
    assert!(!k.verify(b"wrong", &sig));
}

#[test]
fn stub_signature_consistently_round_trips() {
    let s = Signature::stub();
    assert_eq!(s.algo, "stub");
    let json = serde_json::to_string(&s).unwrap();
    let back: Signature = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}

#[test]
fn sha256_digest_is_canonical() {
    assert_eq!(
        sha256_digest(b""),
        "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn glob_compile_handles_edge_cases() {
    // Empty glob → matches empty path only.
    let r = compile_glob("").unwrap();
    assert!(r.is_match(""));
    assert!(!r.is_match("anything"));

    // Special regex characters must be escaped.
    let r = compile_glob("docs/README.md").unwrap();
    assert!(r.is_match("docs/README.md"));
    assert!(
        !r.is_match("docs/READMExmd"),
        "the literal dot must NOT match arbitrary char"
    );

    // Brackets and parens are escaped.
    let r = compile_glob("a(b)[c]").unwrap();
    assert!(r.is_match("a(b)[c]"));
}

#[test]
fn evidence_pack_serializes_with_correct_schema() {
    let p = synth_pack(RiskTier::R2, true);
    let j = serde_json::to_string(&p).unwrap();
    assert!(j.contains("\"schema\":\"vibegate.evidence_pack.v1\""));
    assert!(j.contains("\"risk\":\"R2\""));
}

#[test]
fn evidence_pack_unsigned_then_signed() {
    let mut p = synth_pack(RiskTier::R2, false);
    assert!(p.signature.is_none());
    p.signature = Some(Signature::stub());
    let j = serde_json::to_string(&p).unwrap();
    assert!(j.contains("\"algo\":\"stub\""));
}

#[test]
fn intent_card_skips_optional_fields_when_empty() {
    let c = IntentCard {
        schema: SchemaTag::new(),
        id: "intent_x".into(),
        agent_id: "a".into(),
        repo: "r".into(),
        target_branch: None,
        summary: "s".into(),
        linked_issue: None,
        estimated_risk: None,
        expected_changed_paths: vec![],
        claims: vec![],
        created_at: chrono::Utc::now(),
        signature: None,
    };
    let j = serde_json::to_string(&c).unwrap();
    // Optional/empty fields should be omitted from the JSON.
    assert!(!j.contains("\"target_branch\""));
    assert!(!j.contains("\"signature\""));
    assert!(!j.contains("\"expected_changed_paths\""));
}

#[test]
fn verdict_decision_serializes_snake_case() {
    use jeryu::autonomy::types::VibeGateVerdictTag;
    let v = VibeGateVerdict {
        schema: SchemaTag::<VibeGateVerdictTag>::new(),
        id: "vgv_x".into(),
        evidence_pack_id: "evp_x".into(),
        merge_request: None,
        repo: "r".into(),
        target_branch: "main".into(),
        head_sha: "a".repeat(40),
        policy_sha: "c".repeat(40),
        evidence_pack_digest: format!("sha256:00{}", "0".repeat(62)),
        risk: RiskTier::R2,
        hard_stops: vec![],
        required_reviews: vec![ReviewerRole::Security, ReviewerRole::TestIntegrity],
        approval_receipts: vec![],
        decision: GateDecision::RequireHuman,
        valid_for_head_sha_only: true,
        rebind_on_train: true,
        expires_at: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
        signature: Signature::stub(),
    };
    let j = serde_json::to_string(&v).unwrap();
    assert!(j.contains("\"decision\":\"require_human\""));
    assert!(j.contains("\"required_reviews\":[\"security\",\"test_integrity\"]"));
}

#[test]
fn reviewer_role_lockfile_aliases() {
    // The YAML uses `lockfile_scout`; the Rust enum is `Lockfile`.
    let r: ReviewerRole = serde_yaml::from_str("lockfile_scout").unwrap();
    assert_eq!(r, ReviewerRole::Lockfile);
    let r: ReviewerRole = serde_yaml::from_str("lockfile").unwrap();
    assert_eq!(r, ReviewerRole::Lockfile);
}

fn synth_pack(risk: RiskTier, signed: bool) -> jeryu::autonomy::EvidencePack {
    let mut p = build_evidence_pack(EvidenceInputs {
        repo: "r",
        source_branch: "s",
        target_branch: "main",
        head_sha: "a".repeat(40).leak(),
        base_sha: "b".repeat(40).leak(),
        policy_sha: "c".repeat(40).leak(),
        author_agent: None,
        intent_id: None,
        risk,
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
        legacy_receipts: vec![],
    });
    if signed {
        p.signature = Some(Signature::stub());
    }
    p
}

#[test]
fn mcp_tool_descriptors_serialize() {
    let d = jeryu::autonomy::mcp_tools::descriptors();
    let j = serde_json::to_string(&d).unwrap();
    assert!(j.contains("vibegate.run_review"));
    assert!(j.contains("vibegate.doctor"));
    assert!(j.contains("vibegate.propose_autonomy_edit"));
}

#[test]
fn mcp_tool_descriptors_input_schemas_are_objects() {
    for d in jeryu::autonomy::mcp_tools::descriptors() {
        assert_eq!(
            d.input_schema["type"], "object",
            "{} schema must be object",
            d.name
        );
    }
}

// Suppress unused warning when running this test file's helpers.
#[allow(dead_code)]
fn _force_use(_a: AgentApprovalReceipt, _b: ChangedFile) {}
