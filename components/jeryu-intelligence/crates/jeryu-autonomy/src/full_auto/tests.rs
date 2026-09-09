//! Unit tests for the FULL-AUTO profile loader.
//!
//! These drive the *real* pure judge against the *derived* bundle so the proof
//! is end-to-end: full-auto must make `R0..=R4` land `AllowMerge`, keep `R5`
//! `RequireHuman`, and never weaken the hard-stop / kill-bell / freeze floors.

use super::*;
use crate::conditions::HardStop;
use crate::judge::{JudgeInputs, judge};
use crate::kill_bell::KillBell;
use crate::ledger::MemoryLedger;
use crate::policy_yaml::{PolicyBundle, fixtures};
use crate::seam::VerdictLedger;
use crate::signing::{EdSigningKey, Signature};
use crate::types::*;
use chrono::Utc;
use std::sync::Arc;

use crate::test_support::{bundle, pack_at_tier};

/// All seven reviewer roles passing, so quorum is satisfiable at any tier that
/// requires agent reviewers (R3 needs 4). Distinct agent identities; none the
/// author.
fn full_passing_receipts(pack: &EvidencePack) -> Vec<AgentApprovalReceipt> {
    [
        (ReviewerRole::Security, "sec.v1"),
        (ReviewerRole::TestIntegrity, "test.v1"),
        (ReviewerRole::Runtime, "rt.v1"),
        (ReviewerRole::Lockfile, "lock.v1"),
    ]
    .into_iter()
    .map(|(role, agent)| receipt(role, agent, ReviewDecision::Pass, pack))
    .collect()
}

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
        signature: Signature::unsigned(),
    }
}

fn judge_under(profile: &FullAutoProfile, pack: &EvidencePack) -> GateDecision {
    judge_under_with_stops(profile, pack, &[])
}

fn judge_under_with_stops(
    profile: &FullAutoProfile,
    pack: &EvidencePack,
    external: &[HardStop],
) -> GateDecision {
    let derived = profile.apply();
    let receipts = full_passing_receipts(pack);
    let out = judge(JudgeInputs {
        pack,
        receipts: &receipts,
        policy: &derived,
        repo: "org/p",
        target_branch: "main",
        pull_request: None,
        author_agent: Some("builder.x"),
        external_hard_stops: external,
    });
    // The judge produces the pre-bell verdict; resolve re-asserts the floor.
    profile.resolve(pack.risk, out.verdict.decision)
}

// --- construction / validation ------------------------------------------

#[test]
fn canonical_profile_constructs_and_validates() {
    let p = FullAutoProfile::new(bundle()).expect("constructs");
    assert!(p.is_enabled());
    assert_eq!(p.max_auto_tier(), RiskTier::R4);
    assert!(p.validate().is_ok());
}

#[test]
fn descriptor_yaml_round_trips_and_loads() {
    let yaml = r#"
schema: vibegate.full_auto_profile.v1
enabled: true
max_auto_tier: R4
reason: "owner dogfoods unattended merge"
"#;
    let p = FullAutoProfile::from_yaml(yaml, bundle()).expect("loads");
    assert!(p.is_enabled());
    assert_eq!(p.max_auto_tier(), RiskTier::R4);
    assert_eq!(
        p.descriptor().reason.as_deref(),
        Some("owner dogfoods unattended merge")
    );
}

#[test]
fn descriptor_defaults_max_auto_tier_to_r4() {
    let yaml = "schema: vibegate.full_auto_profile.v1\nenabled: true\n";
    let d = FullAutoDescriptor::from_yaml(yaml).expect("parses");
    assert_eq!(d.max_auto_tier, RiskTier::R4);
}

#[test]
fn malformed_bundle_unknown_field_errors() {
    // Reuse policy_yaml's strict loader: an unknown key fails closed.
    let bad = "schema: vibegate.approvals.v1\ninvariants: {}\nquorum: {}\nbogus_key: true\n";
    let err = PolicyBundle::from_yaml_strs(
        fixtures::RISK_YML,
        bad,
        fixtures::RELEASE_YML,
        fixtures::PROTECTED_PATHS_YML,
        None,
    )
    .unwrap_err();
    assert!(err.contains("approvals.yml"), "got {err}");
}

#[test]
fn malformed_descriptor_unknown_field_errors() {
    let yaml = r#"
schema: vibegate.full_auto_profile.v1
enabled: true
max_auto_tier: R4
smuggled_knob: true
"#;
    let err = FullAutoProfile::from_yaml(yaml, bundle()).unwrap_err();
    assert!(matches!(err, FullAutoError::Parse(_)), "got {err:?}");
}

#[test]
fn wrong_schema_tag_rejected() {
    let yaml = "schema: vibegate.not_full_auto.v1\nenabled: true\n";
    let err = FullAutoProfile::from_yaml(yaml, bundle()).unwrap_err();
    assert!(
        matches!(err, FullAutoError::SchemaMismatch { .. }),
        "got {err:?}"
    );
}

#[test]
fn r5_auto_ceiling_rejected_fail_closed() {
    let yaml = "schema: vibegate.full_auto_profile.v1\nenabled: true\nmax_auto_tier: R5\n";
    let err = FullAutoProfile::from_yaml(yaml, bundle()).unwrap_err();
    assert_eq!(err, FullAutoError::R5NotFailClosed);
}

#[test]
fn bundle_without_r5_floor_rejected() {
    // Build an approvals policy whose R5 entry is absent → floor cannot be
    // guaranteed.
    let approvals = r#"
schema: vibegate.approvals.v1
invariants:
  no_self_approval: true
quorum:
  R0: { approvals_needed: 0, roles: [], human_required: false }
hard_stops: []
"#;
    let b = PolicyBundle::from_yaml_strs(
        fixtures::RISK_YML,
        approvals,
        fixtures::RELEASE_YML,
        fixtures::PROTECTED_PATHS_YML,
        None,
    )
    .expect("parses (R5 simply absent)");
    let err = FullAutoProfile::new(b).unwrap_err();
    assert_eq!(err, FullAutoError::MissingR5Floor);
}

#[test]
fn bundle_with_non_human_r5_rejected() {
    // R5 present but human_required:false → not a valid fail-closed floor.
    let approvals = r#"
schema: vibegate.approvals.v1
invariants: { no_self_approval: true }
quorum:
  R5: { approvals_needed: 0, roles: [], human_required: false }
hard_stops: []
"#;
    let b = PolicyBundle::from_yaml_strs(
        fixtures::RISK_YML,
        approvals,
        fixtures::RELEASE_YML,
        fixtures::PROTECTED_PATHS_YML,
        None,
    )
    .expect("parses");
    let err = FullAutoProfile::new(b).unwrap_err();
    assert_eq!(err, FullAutoError::MissingR5Floor);
}

// --- policy-authoritative tier eligibility ------------------------------

#[test]
fn enabled_profile_makes_r0_through_r4_eligible_r5_not() {
    let p = FullAutoProfile::new(bundle()).unwrap();
    for t in [
        RiskTier::R0,
        RiskTier::R1,
        RiskTier::R2,
        RiskTier::R3,
        RiskTier::R4,
    ] {
        assert!(p.tier_auto_merge_eligible(t), "{t:?} should be eligible");
    }
    assert!(
        !p.tier_auto_merge_eligible(RiskTier::R5),
        "R5 is never eligible"
    );
}

#[test]
fn disabled_profile_falls_back_to_crate_default() {
    let d = FullAutoDescriptor {
        schema: FULL_AUTO_SCHEMA.into(),
        enabled: false,
        max_auto_tier: RiskTier::R4,
        reason: None,
    };
    let p = FullAutoProfile::with_descriptor(d, bundle()).unwrap();
    // Crate default: R0|R1|R2 only.
    assert!(p.tier_auto_merge_eligible(RiskTier::R2));
    assert!(!p.tier_auto_merge_eligible(RiskTier::R3));
    assert!(!p.tier_auto_merge_eligible(RiskTier::R4));
    assert!(!p.tier_auto_merge_eligible(RiskTier::R5));
}

#[test]
fn lower_auto_ceiling_is_honored() {
    let d = FullAutoDescriptor {
        schema: FULL_AUTO_SCHEMA.into(),
        enabled: true,
        max_auto_tier: RiskTier::R3,
        reason: None,
    };
    let p = FullAutoProfile::with_descriptor(d, bundle()).unwrap();
    assert!(p.tier_auto_merge_eligible(RiskTier::R3));
    assert!(!p.tier_auto_merge_eligible(RiskTier::R4));
    assert!(!p.tier_auto_merge_eligible(RiskTier::R5));
}

// --- end-to-end through the real judge ----------------------------------

#[test]
fn full_auto_allows_merge_r0_through_r4() {
    let p = FullAutoProfile::new(bundle()).unwrap();
    for t in [
        RiskTier::R0,
        RiskTier::R1,
        RiskTier::R2,
        RiskTier::R3,
        RiskTier::R4,
    ] {
        let pack = pack_at_tier(t, true, false);
        assert_eq!(
            judge_under(&p, &pack),
            GateDecision::AllowMerge,
            "full-auto must AllowMerge at {t:?}"
        );
    }
}

#[test]
fn full_auto_requires_human_at_r5() {
    let p = FullAutoProfile::new(bundle()).unwrap();
    let pack = pack_at_tier(RiskTier::R5, true, false);
    assert_eq!(
        judge_under(&p, &pack),
        GateDecision::RequireHuman,
        "R5 stays fail-closed (human required) under full-auto"
    );
}

#[test]
fn r4_requires_human_without_full_auto_baseline() {
    // Sanity: the *default* bundle (no full-auto) lands RequireHuman at R4, so
    // the AllowMerge above is genuinely the profile's doing.
    let b = bundle();
    let pack = pack_at_tier(RiskTier::R4, true, false);
    let receipts = full_passing_receipts(&pack);
    let out = judge(JudgeInputs {
        pack: &pack,
        receipts: &receipts,
        policy: &b,
        repo: "org/p",
        target_branch: "main",
        pull_request: None,
        author_agent: Some("builder.x"),
        external_hard_stops: &[],
    });
    assert_eq!(out.verdict.decision, GateDecision::RequireHuman);
}

// --- safety floor: hard stops still veto at every tier ------------------

#[test]
fn hard_stop_rejects_at_every_tier_even_full_auto() {
    let p = FullAutoProfile::new(bundle()).unwrap();
    for t in [
        RiskTier::R0,
        RiskTier::R1,
        RiskTier::R2,
        RiskTier::R3,
        RiskTier::R4,
        RiskTier::R5,
    ] {
        // secret_scan_failed fires the registry hard stop.
        let pack = pack_at_tier(t, true, true);
        assert_eq!(
            judge_under(&p, &pack),
            GateDecision::Reject,
            "hard stop must Reject at {t:?} (veto > approval)"
        );
    }
}

#[test]
fn injected_external_hard_stop_rejects_under_full_auto() {
    let p = FullAutoProfile::new(bundle()).unwrap();
    let pack = pack_at_tier(RiskTier::R1, true, false);
    let injected = [HardStop {
        name: "codeowners_not_satisfied".into(),
        reason: "no codeowner approval".into(),
        details: serde_json::Value::Null,
    }];
    assert_eq!(
        judge_under_with_stops(&p, &pack, &injected),
        GateDecision::Reject
    );
}

#[test]
fn resolve_reject_is_terminal_at_every_tier() {
    let p = FullAutoProfile::new(bundle()).unwrap();
    for t in [
        RiskTier::R0,
        RiskTier::R1,
        RiskTier::R2,
        RiskTier::R3,
        RiskTier::R4,
        RiskTier::R5,
    ] {
        assert_eq!(
            p.resolve(t, GateDecision::Reject),
            GateDecision::Reject,
            "Reject must survive resolve at {t:?}"
        );
    }
}

#[test]
fn resolve_downgrades_allow_merge_at_r5() {
    let p = FullAutoProfile::new(bundle()).unwrap();
    // Even if a caller fused against the raw bundle and somehow got AllowMerge
    // at R5, resolve fails it closed.
    assert_eq!(
        p.resolve(RiskTier::R5, GateDecision::AllowMerge),
        GateDecision::RequireHuman
    );
    // And passes R0..=R4 AllowMerge through.
    assert_eq!(
        p.resolve(RiskTier::R4, GateDecision::AllowMerge),
        GateDecision::AllowMerge
    );
}

// --- safety floor: kill-bell still downgrades ---------------------------

#[tokio::test]
async fn kill_bell_downgrades_full_auto_allow_merge_to_require_human() {
    let p = FullAutoProfile::new(bundle()).unwrap();
    let pack = pack_at_tier(RiskTier::R4, true, false);
    // Pre-bell, full-auto AllowMerges at R4.
    assert_eq!(judge_under(&p, &pack), GateDecision::AllowMerge);

    let ledger: Arc<dyn VerdictLedger> = Arc::new(MemoryLedger::new());
    let bell = KillBell::new(ledger);
    let key = EdSigningKey::generate("operator.alice");
    let now = Utc::now();
    bell.pause("incident", "alice", 3600, &key, now)
        .await
        .unwrap();

    let (decision, why) = bell
        .downgrade_if_paused(GateDecision::AllowMerge, now)
        .await
        .unwrap();
    assert_eq!(
        decision,
        GateDecision::RequireHuman,
        "kill bell downgrades full-auto AllowMerge"
    );
    assert!(why.unwrap().contains("incident"));
}

// --- safety floor: derived bundle preserves R5, hard_stops, freeze ------

#[test]
fn apply_preserves_r5_human_required() {
    let p = FullAutoProfile::new(bundle()).unwrap();
    let derived = p.apply();
    let r5 = derived.quorum_for(RiskTier::R5).expect("R5 present");
    assert!(r5.human_required, "R5 stays human_required after apply");
    assert!(r5.fail_closed, "R5 stays fail_closed after apply");
}

#[test]
fn apply_clears_human_required_only_up_to_ceiling() {
    let p = FullAutoProfile::new(bundle()).unwrap();
    let derived = p.apply();
    // R3 and R4 had human_required:true in the fixture; cleared now.
    assert!(!derived.quorum_for(RiskTier::R3).unwrap().human_required);
    assert!(!derived.quorum_for(RiskTier::R4).unwrap().human_required);
    assert!(
        !derived
            .quorum_for(RiskTier::R4)
            .unwrap()
            .fail_closed_without_human
    );
}

#[test]
fn apply_preserves_hard_stops_and_quorum_roles() {
    let p = FullAutoProfile::new(bundle()).unwrap();
    let derived = p.apply();
    // Hard-stop list untouched (full registry of 15 named stops).
    assert_eq!(
        derived.approvals.hard_stops.len(),
        p.source_bundle().approvals.hard_stops.len()
    );
    // R3's role requirements (4 distinct reviewers) survive — full-auto relaxes
    // the *human* floor, not the *agent-reviewer* quorum.
    let r3 = derived.quorum_for(RiskTier::R3).unwrap();
    assert_eq!(r3.approvals_needed, 4);
    assert_eq!(r3.roles.len(), 4);
}

#[test]
fn apply_is_noop_when_disabled() {
    let d = FullAutoDescriptor {
        schema: FULL_AUTO_SCHEMA.into(),
        enabled: false,
        max_auto_tier: RiskTier::R4,
        reason: None,
    };
    let p = FullAutoProfile::with_descriptor(d, bundle()).unwrap();
    let derived = p.apply();
    // Disabled → unchanged → R4 still human_required.
    assert!(derived.quorum_for(RiskTier::R4).unwrap().human_required);
}

#[test]
fn unsigned_pack_still_rejects_under_full_auto() {
    // evidence_signature_invalid is a registry hard stop; full-auto must not
    // bypass it.
    let p = FullAutoProfile::new(bundle()).unwrap();
    let pack = pack_at_tier(RiskTier::R2, false, false);
    assert_eq!(judge_under(&p, &pack), GateDecision::Reject);
}

// --- pre-merge CI gate: required lanes must be green ---------------------
//
// These prove, through the *real* judge + registry + full-auto profile, that a
// policy declaring `required_ci_lanes` blocks auto-merge whenever a required
// lane is absent or not green — closing the hole where an R0-R4 PR could
// auto-merge with RED or MISSING required checks.

/// A bundle whose approvals policy declares `lanes` as required CI checks.
fn bundle_requiring(lanes: &[&str]) -> PolicyBundle {
    let mut b = bundle();
    b.approvals.required_ci_lanes = lanes.iter().map(|s| s.to_string()).collect();
    b
}

/// Attach a CI-status report to a pack.
fn with_ci(mut pack: EvidencePack, checks: &[(&str, CiConclusion)]) -> EvidencePack {
    pack.ci_status = checks
        .iter()
        .map(|(name, conclusion)| CiCheck {
            name: (*name).to_string(),
            conclusion: *conclusion,
        })
        .collect();
    pack
}

/// Fuse a pack through the full-auto profile derived from `bundle`, returning
/// the post-resolve decision. Mirrors [`judge_under`] but lets a test pick the
/// CI-lane-aware bundle.
fn judge_pack_under_bundle(bundle: PolicyBundle, pack: &EvidencePack) -> GateDecision {
    let profile = FullAutoProfile::new(bundle).unwrap();
    let derived = profile.apply();
    let receipts = full_passing_receipts(pack);
    let out = judge(JudgeInputs {
        pack,
        receipts: &receipts,
        policy: &derived,
        repo: "org/p",
        target_branch: "main",
        pull_request: None,
        author_agent: Some("builder.x"),
        external_hard_stops: &[],
    });
    profile.resolve(pack.risk, out.verdict.decision)
}

#[test]
fn ci_gate_all_required_lanes_green_allows_merge_r0_through_r4() {
    for t in [
        RiskTier::R0,
        RiskTier::R1,
        RiskTier::R2,
        RiskTier::R3,
        RiskTier::R4,
    ] {
        let pack = with_ci(
            pack_at_tier(t, true, false),
            &[
                ("ci-fast", CiConclusion::Success),
                ("ci-full", CiConclusion::Success),
            ],
        );
        let b = bundle_requiring(&["ci-fast", "ci-full"]);
        assert_eq!(
            judge_pack_under_bundle(b, &pack),
            GateDecision::AllowMerge,
            "all required lanes green must AllowMerge at {t:?} under full-auto"
        );
    }
}

#[test]
fn ci_gate_failed_required_lane_blocks_via_hard_stop() {
    for t in [
        RiskTier::R0,
        RiskTier::R1,
        RiskTier::R2,
        RiskTier::R3,
        RiskTier::R4,
    ] {
        let pack = with_ci(
            pack_at_tier(t, true, false),
            &[
                ("ci-fast", CiConclusion::Success),
                ("ci-full", CiConclusion::Failure),
            ],
        );
        let b = bundle_requiring(&["ci-fast", "ci-full"]);
        assert_eq!(
            judge_pack_under_bundle(b, &pack),
            GateDecision::Reject,
            "a failed required lane is a hard stop at {t:?} (veto > approval)"
        );
    }
}

#[test]
fn ci_gate_missing_required_lane_blocks_via_hard_stop() {
    for t in [
        RiskTier::R0,
        RiskTier::R1,
        RiskTier::R2,
        RiskTier::R3,
        RiskTier::R4,
    ] {
        // ci-full is required but never reported.
        let pack = with_ci(
            pack_at_tier(t, true, false),
            &[("ci-fast", CiConclusion::Success)],
        );
        let b = bundle_requiring(&["ci-fast", "ci-full"]);
        assert_eq!(
            judge_pack_under_bundle(b, &pack),
            GateDecision::Reject,
            "a missing required lane is a hard stop at {t:?} (fail-closed)"
        );
    }
}

#[test]
fn ci_gate_pending_required_lane_is_not_green() {
    // Pending is explicitly NOT green: a still-running required lane must block.
    let pack = with_ci(
        pack_at_tier(RiskTier::R2, true, false),
        &[("ci-fast", CiConclusion::Pending)],
    );
    let b = bundle_requiring(&["ci-fast"]);
    assert_eq!(judge_pack_under_bundle(b, &pack), GateDecision::Reject);
}

#[test]
fn ci_gate_no_required_lanes_declared_is_back_compat() {
    // No required_ci_lanes → no CI gate, even with a red lane present in the
    // pack. Repos that haven't opted in keep merging as before.
    let pack = with_ci(
        pack_at_tier(RiskTier::R2, true, false),
        &[("ci-fast", CiConclusion::Failure)],
    );
    let b = bundle(); // required_ci_lanes empty
    assert_eq!(judge_pack_under_bundle(b, &pack), GateDecision::AllowMerge);
}

#[test]
fn ci_gate_does_not_relax_r5_floor() {
    // Even with every required lane green, R5 stays human-required.
    let pack = with_ci(
        pack_at_tier(RiskTier::R5, true, false),
        &[("ci-fast", CiConclusion::Success)],
    );
    let b = bundle_requiring(&["ci-fast"]);
    assert_eq!(
        judge_pack_under_bundle(b, &pack),
        GateDecision::RequireHuman
    );
}

#[test]
fn ci_gate_names_surface_in_verdict_hard_stops() {
    // The verdict must name the specific CI hard stop so operators can see why.
    let pack = with_ci(
        pack_at_tier(RiskTier::R2, true, false),
        &[("ci-full", CiConclusion::Failure)],
    );
    let b = bundle_requiring(&["ci-fast", "ci-full"]);
    let profile = FullAutoProfile::new(b).unwrap();
    let derived = profile.apply();
    let receipts = full_passing_receipts(&pack);
    let out = judge(JudgeInputs {
        pack: &pack,
        receipts: &receipts,
        policy: &derived,
        repo: "org/p",
        target_branch: "main",
        pull_request: None,
        author_agent: Some("builder.x"),
        external_hard_stops: &[],
    });
    assert_eq!(out.verdict.decision, GateDecision::Reject);
    // ci-fast missing + ci-full failed → both names present.
    assert!(
        out.verdict
            .hard_stops
            .iter()
            .any(|n| n == "missing_required_ci_check"),
        "got {:?}",
        out.verdict.hard_stops
    );
    assert!(
        out.verdict
            .hard_stops
            .iter()
            .any(|n| n == "failed_required_ci_check"),
        "got {:?}",
        out.verdict.hard_stops
    );
}

#[tokio::test]
async fn ci_gate_kill_bell_still_downgrades_when_lanes_green() {
    // With all required lanes green full-auto AllowMerges; the kill bell must
    // still downgrade that AllowMerge to RequireHuman.
    let pack = with_ci(
        pack_at_tier(RiskTier::R4, true, false),
        &[("ci-fast", CiConclusion::Success)],
    );
    let b = bundle_requiring(&["ci-fast"]);
    assert_eq!(
        judge_pack_under_bundle(b, &pack),
        GateDecision::AllowMerge,
        "pre-bell, green CI lets full-auto AllowMerge at R4"
    );

    let ledger: Arc<dyn VerdictLedger> = Arc::new(MemoryLedger::new());
    let bell = KillBell::new(ledger);
    let key = EdSigningKey::generate("operator.alice");
    let now = Utc::now();
    bell.pause("incident", "alice", 3600, &key, now)
        .await
        .unwrap();
    let (decision, _why) = bell
        .downgrade_if_paused(GateDecision::AllowMerge, now)
        .await
        .unwrap();
    assert_eq!(decision, GateDecision::RequireHuman);
}
