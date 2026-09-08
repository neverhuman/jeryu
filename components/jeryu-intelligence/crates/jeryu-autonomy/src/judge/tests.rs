use super::*;
use crate::conditions::HardStop;
use crate::signing::Signature;
use crate::types::*;
use chrono::Utc;

use crate::test_support::{bundle, pack_at_tier};

fn receipt(
    role: ReviewerRole,
    agent: &str,
    decision: ReviewDecision,
    pack: &EvidencePack,
) -> AgentApprovalReceipt {
    AgentApprovalReceipt {
        schema: SchemaTag::new(),
        id: format!("aar_{agent}"),
        evidence_pack_id: pack.id.clone(),
        role,
        agent_id: agent.into(),
        prompt_sha: None,
        provider: None,
        model: None,
        temperature: None,
        seed: None,
        raw_response_sha: Some("sha256:beef".into()),
        head_sha: pack.head_sha.clone(),
        policy_sha: pack.policy_sha.clone(),
        decision,
        reason: None,
        findings: vec![],
        not_author: true,
        tokens: TokenCounts::default(),
        created_at: Utc::now(),
        signature: Signature {
            key_id: format!("{agent}.ed25519"),
            algo: "hmac-sha256-insecure".into(),
            value: "0".repeat(64),
        },
    }
}

#[test]
fn allow_merge_when_quorum_met_no_hard_stops() {
    let b = bundle();
    let p = pack_at_tier(RiskTier::R2, true, false);
    let receipts = vec![
        receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
        receipt(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        ),
    ];
    let out = judge(JudgeInputs {
        pack: &p,
        receipts: &receipts,
        policy: &b,
        repo: "org/p",
        target_branch: "main",
        pull_request: Some("!1"),
        author_agent: Some("builder.x"),
        external_hard_stops: &[],
    });
    assert_eq!(out.verdict.decision, GateDecision::AllowMerge);
    assert!(out.verdict.hard_stops.is_empty());
    assert_eq!(out.dropped_receipts.len(), 0);
}

#[test]
fn one_blocking_reviewer_rejects_via_hard_stop() {
    let b = bundle();
    let p = pack_at_tier(RiskTier::R2, true, false);
    let receipts = vec![
        receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Block, &p),
        receipt(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        ),
        receipt(ReviewerRole::Runtime, "rt.v1", ReviewDecision::Pass, &p),
    ];
    let out = judge(JudgeInputs {
        pack: &p,
        receipts: &receipts,
        policy: &b,
        repo: "org/p",
        target_branch: "main",
        pull_request: None,
        author_agent: None,
        external_hard_stops: &[],
    });
    assert_eq!(out.verdict.decision, GateDecision::Reject);
    assert!(
        out.verdict
            .hard_stops
            .contains(&"reviewer_blocked".to_string())
    );
}

#[test]
fn secret_scan_failure_rejects_even_with_unanimous_approval() {
    let b = bundle();
    let p = pack_at_tier(RiskTier::R2, true, true);
    let receipts = vec![
        receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
        receipt(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        ),
    ];
    let out = judge(JudgeInputs {
        pack: &p,
        receipts: &receipts,
        policy: &b,
        repo: "org/p",
        target_branch: "main",
        pull_request: None,
        author_agent: None,
        external_hard_stops: &[],
    });
    assert_eq!(out.verdict.decision, GateDecision::Reject);
    assert!(
        out.verdict
            .hard_stops
            .iter()
            .any(|n| n == "secret_scan_failed")
    );
}

#[test]
fn sha_drift_drops_receipt() {
    let b = bundle();
    let p = pack_at_tier(RiskTier::R2, true, false);
    let mut bad = receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p);
    bad.head_sha = "d".repeat(40);
    let good = receipt(
        ReviewerRole::TestIntegrity,
        "test.v1",
        ReviewDecision::Pass,
        &p,
    );
    let receipts = vec![bad, good];
    let out = judge(JudgeInputs {
        pack: &p,
        receipts: &receipts,
        policy: &b,
        repo: "org/p",
        target_branch: "main",
        pull_request: None,
        author_agent: None,
        external_hard_stops: &[],
    });
    // Drift drops the security receipt → missing role → require_human.
    assert_eq!(out.dropped_receipts.len(), 1);
    assert_eq!(out.verdict.decision, GateDecision::RequireHuman);
}

#[test]
fn unsigned_pack_fails_closed_via_evidence_signature_invalid() {
    let b = bundle();
    let p = pack_at_tier(RiskTier::R2, false, false);
    let receipts = vec![
        receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
        receipt(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        ),
    ];
    let out = judge(JudgeInputs {
        pack: &p,
        receipts: &receipts,
        policy: &b,
        repo: "org/p",
        target_branch: "main",
        pull_request: None,
        author_agent: None,
        external_hard_stops: &[],
    });
    assert_eq!(out.verdict.decision, GateDecision::Reject);
    assert!(
        out.verdict
            .hard_stops
            .iter()
            .any(|n| n == "evidence_signature_invalid")
    );
}

#[test]
fn injected_codeowners_not_satisfied_forces_reject() {
    let b = bundle();
    let p = pack_at_tier(RiskTier::R2, true, false);
    let receipts = vec![
        receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
        receipt(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        ),
    ];
    let injected = [HardStop {
        name: "codeowners_not_satisfied".into(),
        reason: "no @security approval on src/auth/login.rs".into(),
        details: serde_json::json!({"path": "src/auth/login.rs"}),
    }];
    let out = judge(JudgeInputs {
        pack: &p,
        receipts: &receipts,
        policy: &b,
        repo: "org/p",
        target_branch: "main",
        pull_request: None,
        author_agent: None,
        external_hard_stops: &injected,
    });
    assert_eq!(out.verdict.decision, GateDecision::Reject);
    assert!(
        out.verdict
            .hard_stops
            .iter()
            .any(|n| n == "codeowners_not_satisfied")
    );
}

#[test]
fn r4_protected_path_requires_human_even_with_all_passes() {
    let b = bundle();
    let p = pack_at_tier(RiskTier::R4, true, false);
    let out = judge(JudgeInputs {
        pack: &p,
        receipts: &[],
        policy: &b,
        repo: "org/p",
        target_branch: "main",
        pull_request: None,
        author_agent: None,
        external_hard_stops: &[],
    });
    assert_eq!(out.verdict.decision, GateDecision::RequireHuman);
}

#[test]
fn mint_verdict_id_is_30_chars_and_prefixed() {
    let now = chrono::Utc::now();
    let head = "f".repeat(40);
    let id = mint_verdict_id(now, &head);
    assert!(id.starts_with("vgv_"), "id must start with vgv_; got {id}");
    assert_eq!(id.len(), 30, "id must be exactly 30 chars; got `{id}`");
    let id2 = mint_verdict_id(now, "abc");
    assert!(id2.starts_with("vgv_"));
    assert_eq!(id2.len(), 30);
}
