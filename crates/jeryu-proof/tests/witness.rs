//! Integration tests for `ProofWitness` verification — the "no proof, no merge"
//! enforcement on the evidence side.
//!
//! A witness is minted only when every REQUIRED lane in the plan has matching
//! evidence and that evidence succeeded. Missing evidence for a required lane,
//! or failing evidence, blocks the merge. Non-required lanes are not gated.

use jeryu_proof::{
    ChangeSet, OwnerRule, PathPattern, ProofBlocker, ProofEngine, ProofEvidence, ProofLane,
    ProofPlan, TestRule, default_phase7_engine,
};

use jeryu_core::phase7::{ChangedPath, PullRequestId, ReceiptKind, RepoId};

fn change_set(paths: Vec<ChangedPath>) -> ChangeSet {
    ChangeSet {
        repo: RepoId::new("repo_phase7"),
        pr: PullRequestId::new("pr_1"),
        head_sha: "head001".to_string(),
        paths,
        agent_authored: false,
    }
}

/// Produces a passing evidence entry for every lane in a plan.
fn passing_evidence(plan: &ProofPlan) -> Vec<ProofEvidence> {
    plan.lanes
        .iter()
        .map(|lane| ProofEvidence {
            lane: lane.name.clone(),
            commands: lane.commands.clone(),
            success: true,
            log_digest: format!("digest:{}", lane.name),
        })
        .collect()
}

#[test]
fn witness_is_minted_when_all_required_lanes_pass() {
    let engine = default_phase7_engine();
    let plan = engine
        .plan(&change_set(vec![ChangedPath::new(
            "crates/jeryu_proof/src/lib.rs",
        )]))
        .expect("proof path is owned and mapped");
    let witness = engine
        .verify(&plan, &passing_evidence(&plan))
        .expect("passing required lanes mint a witness");

    assert_eq!(witness.repo, RepoId::new("repo_phase7"));
    assert_eq!(witness.pr, PullRequestId::new("pr_1"));
    assert_eq!(witness.head_sha, "head001");
    assert_eq!(witness.changed_paths, vec!["crates/jeryu_proof/src/lib.rs"]);
    // jeryu_proof/** maps to unit + jankurai-proof.
    assert!(witness.lanes.contains(&"unit".to_string()));
    assert!(witness.lanes.contains(&"jankurai-proof".to_string()));
    assert_eq!(witness.owners, vec!["@jeryu_proof".to_string()]);
}

#[test]
fn witness_receipt_is_a_proof_witness_bound_to_pr_and_sha() {
    let engine = default_phase7_engine();
    let plan = engine
        .plan(&change_set(vec![ChangedPath::new(
            "crates/jeryu_core/src/lib.rs",
        )]))
        .expect("core path owned and mapped");
    let witness = engine
        .verify(&plan, &passing_evidence(&plan))
        .expect("witness minted");

    let receipt = &witness.receipt;
    assert_eq!(receipt.kind, ReceiptKind::ProofWitness);
    assert_eq!(receipt.repo, RepoId::new("repo_phase7"));
    assert_eq!(receipt.subject, "pr_1");
    assert_eq!(receipt.sha, "head001");
    assert_eq!(receipt.residual_risk, "none: all required lanes passed");
    // The witness id equals its receipt id.
    assert_eq!(witness.id, receipt.id);
    // Each lane shows up as a proof-lane command in the receipt.
    assert!(
        receipt
            .commands
            .iter()
            .any(|c| c == "proof-lane:unit")
    );
}

#[test]
fn missing_required_lane_evidence_blocks_merge() {
    let engine = default_phase7_engine();
    let plan = engine
        .plan(&change_set(vec![ChangedPath::new(
            "crates/jeryu_proof/src/lib.rs",
        )]))
        .expect("proof path owned and mapped");
    // Provide evidence for every lane EXCEPT jankurai-proof.
    let evidence: Vec<ProofEvidence> = plan
        .lanes
        .iter()
        .filter(|lane| lane.name != "jankurai-proof")
        .map(|lane| ProofEvidence {
            lane: lane.name.clone(),
            commands: lane.commands.clone(),
            success: true,
            log_digest: format!("digest:{}", lane.name),
        })
        .collect();
    let err = engine
        .verify(&plan, &evidence)
        .expect_err("missing evidence for a required lane must block");
    assert_eq!(
        err,
        ProofBlocker::MissingEvidence("jankurai-proof".to_string())
    );
}

#[test]
fn failed_required_lane_blocks_merge() {
    let engine = default_phase7_engine();
    let plan = engine
        .plan(&change_set(vec![ChangedPath::new(
            "crates/jeryu_core/src/lib.rs",
        )]))
        .expect("core path owned and mapped");
    // All lanes present, but `unit` failed.
    let evidence: Vec<ProofEvidence> = plan
        .lanes
        .iter()
        .map(|lane| ProofEvidence {
            lane: lane.name.clone(),
            commands: lane.commands.clone(),
            success: lane.name != "unit",
            log_digest: format!("digest:{}", lane.name),
        })
        .collect();
    let err = engine
        .verify(&plan, &evidence)
        .expect_err("a failed required lane must block");
    assert_eq!(err, ProofBlocker::FailedLane("unit".to_string()));
}

#[test]
fn non_required_lane_missing_evidence_does_not_block() {
    // Build a plan whose only lane is non-required (`docs`). Verify must mint a
    // witness even with NO evidence supplied, because no required lane gates it.
    let engine = ProofEngine::new(
        vec![OwnerRule {
            pattern: PathPattern::new("docs/**"),
            owners: vec!["@docs".to_string()],
        }],
        vec![TestRule {
            pattern: PathPattern::new("docs/**"),
            lanes: vec!["docs".to_string()],
        }],
        Vec::new(),
        vec![ProofLane {
            name: "docs".to_string(),
            commands: vec!["cargo doc".to_string()],
            required: false,
        }],
    );
    let plan = engine
        .plan(&change_set(vec![ChangedPath::new("docs/readme.md")]))
        .expect("docs path owned and mapped");
    let witness = engine
        .verify(&plan, &[])
        .expect("non-required lanes do not require evidence");
    assert_eq!(witness.lanes, vec!["docs".to_string()]);
}

#[test]
fn non_required_lane_failure_does_not_block() {
    let engine = ProofEngine::new(
        vec![OwnerRule {
            pattern: PathPattern::new("docs/**"),
            owners: vec!["@docs".to_string()],
        }],
        vec![TestRule {
            pattern: PathPattern::new("docs/**"),
            lanes: vec!["docs".to_string()],
        }],
        Vec::new(),
        vec![ProofLane {
            name: "docs".to_string(),
            commands: vec!["cargo doc".to_string()],
            required: false,
        }],
    );
    let plan = engine
        .plan(&change_set(vec![ChangedPath::new("docs/readme.md")]))
        .expect("docs path owned and mapped");
    // Even a FAILED non-required lane must not block.
    let witness = engine
        .verify(
            &plan,
            &[ProofEvidence {
                lane: "docs".to_string(),
                commands: vec!["cargo doc".to_string()],
                success: false,
                log_digest: "digest:docs".to_string(),
            }],
        )
        .expect("non-required lane failure is tolerated");
    assert_eq!(witness.lanes, vec!["docs".to_string()]);
}

#[test]
fn extra_unexpected_evidence_is_ignored() {
    // Evidence for a lane not in the plan must not cause failure; only required
    // plan lanes are checked.
    let engine = default_phase7_engine();
    let plan = engine
        .plan(&change_set(vec![ChangedPath::new(
            "crates/jeryu_core/src/lib.rs",
        )]))
        .expect("core path owned and mapped");
    let mut evidence = passing_evidence(&plan);
    evidence.push(ProofEvidence {
        lane: "totally-unrelated".to_string(),
        commands: vec!["noop".to_string()],
        success: false,
        log_digest: "digest:noop".to_string(),
    });
    let witness = engine
        .verify(&plan, &evidence)
        .expect("extra evidence is harmless");
    assert!(witness.lanes.contains(&"unit".to_string()));
}

#[test]
fn end_to_end_no_proof_no_merge_happy_path() {
    // Full happy path: plan a real multi-path change, run every lane, mint the
    // witness, and confirm the witness covers all paths and lanes.
    let engine = default_phase7_engine();
    let plan = engine
        .plan(&change_set(vec![
            ChangedPath::new("crates/jeryu_core/src/model.rs"),
            ChangedPath::new("crates/jeryu_proof/src/engine.rs"),
        ]))
        .expect("both paths owned and mapped");
    let witness = engine
        .verify(&plan, &passing_evidence(&plan))
        .expect("all required lanes pass -> merge allowed");

    assert_eq!(witness.changed_paths.len(), 2);
    // Combined lane set: unit (shared), policy (core), jankurai-proof (proof).
    assert!(witness.lanes.contains(&"unit".to_string()));
    assert!(witness.lanes.contains(&"policy".to_string()));
    assert!(witness.lanes.contains(&"jankurai-proof".to_string()));
    // Owners union both crates.
    assert!(witness.owners.contains(&"@jeryu_core".to_string()));
    assert!(witness.owners.contains(&"@jeryu_proof".to_string()));
}

#[test]
fn witness_lane_summary_counts_paths_and_lanes() {
    let engine = default_phase7_engine();
    let plan = engine
        .plan(&change_set(vec![ChangedPath::new(
            "crates/jeryu_proof/src/lib.rs",
        )]))
        .expect("proof path owned and mapped");
    let witness = engine
        .verify(&plan, &passing_evidence(&plan))
        .expect("witness minted");
    // Summary is a deterministic human string describing coverage.
    assert_eq!(
        witness.receipt.summary,
        format!(
            "proof witness covers {} path(s) and {} lane(s)",
            witness.changed_paths.len(),
            witness.lanes.len()
        )
    );
}
