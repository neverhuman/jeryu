//! Integration tests for `ProofPlan` derivation from a `ChangeSet`.
//!
//! Verifies which proof lanes and owners are required for a given set of
//! changed paths, that owners/lanes are deduplicated and ordered, that
//! sensitive paths inject the `policy` lane, and that the "no proof, no merge"
//! gating fires for ownerless paths and unmapped proof lanes.

use jeryu_proof::{
    ChangeSet, OwnerRule, PathPattern, ProofBlocker, ProofEngine, ProofLane, TestRule,
    default_phase7_engine,
};

use jeryu_core::phase7::{ChangedPath, PullRequestId, RepoId};

/// Builds a non-agent change set for the canonical Phase 7 repo fixture.
fn change_set(paths: Vec<ChangedPath>) -> ChangeSet {
    ChangeSet {
        repo: RepoId::new("repo_phase7"),
        pr: PullRequestId::new("pr_1"),
        head_sha: "head001".to_string(),
        paths,
        agent_authored: false,
    }
}

#[test]
fn plan_for_core_change_requires_unit_and_policy_lanes() {
    let engine = default_phase7_engine();
    let plan = engine
        .plan(&change_set(vec![ChangedPath::new(
            "crates/jeryu_core/src/model.rs",
        )]))
        .expect("core path is owned and mapped");

    assert_eq!(plan.repo, RepoId::new("repo_phase7"));
    assert_eq!(plan.pr, PullRequestId::new("pr_1"));
    assert_eq!(plan.head_sha, "head001");
    assert_eq!(plan.paths, vec!["crates/jeryu_core/src/model.rs"]);
    assert_eq!(plan.owners, vec!["@jeryu_core".to_string()]);

    let lane_names: Vec<&str> = plan.lanes.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(lane_names, vec!["policy", "unit"]);
}

#[test]
fn plan_lane_set_is_sorted_and_deduplicated_across_paths() {
    let engine = default_phase7_engine();
    // Two paths sharing the `unit` lane plus distinct lanes; the resulting
    // lane set must be deduplicated and sorted (BTreeSet ordering).
    let plan = engine
        .plan(&change_set(vec![
            ChangedPath::new("crates/jeryu_core/src/a.rs"),
            ChangedPath::new("crates/jeryu_proof/src/b.rs"),
        ]))
        .expect("both paths owned and mapped");

    let lane_names: Vec<&str> = plan.lanes.iter().map(|l| l.name.as_str()).collect();
    // unit appears once despite two contributing paths; sorted alphabetically.
    assert_eq!(lane_names, vec!["jankurai-proof", "policy", "unit"]);
}

#[test]
fn plan_owner_set_is_sorted_and_deduplicated() {
    let engine = default_phase7_engine();
    // docs/** maps to two owners (@docs and @phase7); Cargo.toml maps to
    // @phase7 again, which must be deduplicated.
    let plan = engine
        .plan(&change_set(vec![
            ChangedPath::new("docs/guide.md"),
            ChangedPath::new("Cargo.toml"),
        ]))
        .expect("docs and Cargo.toml owned and mapped");

    assert_eq!(plan.owners, vec!["@docs".to_string(), "@phase7".to_string()]);
}

#[test]
fn plan_preserves_changed_path_order() {
    let engine = default_phase7_engine();
    let plan = engine
        .plan(&change_set(vec![
            ChangedPath::new("crates/jeryu_proof/src/z.rs"),
            ChangedPath::new("crates/jeryu_core/src/a.rs"),
        ]))
        .expect("paths owned and mapped");

    // Paths are recorded in the order they appear in the change set, not sorted.
    assert_eq!(
        plan.paths,
        vec![
            "crates/jeryu_proof/src/z.rs".to_string(),
            "crates/jeryu_core/src/a.rs".to_string(),
        ]
    );
}

#[test]
fn sensitive_path_injects_the_policy_lane() {
    let engine = default_phase7_engine();
    // ops/ci/** maps only to `local-ci-parity`; marking it sensitive must add
    // the `policy` lane on top.
    let plan = engine
        .plan(&change_set(vec![
            ChangedPath::new("ops/ci/pipeline.yml").sensitive(),
        ]))
        .expect("sensitive ops path owned and mapped");

    let lane_names: Vec<&str> = plan.lanes.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(lane_names, vec!["local-ci-parity", "policy"]);
}

#[test]
fn non_sensitive_path_does_not_inject_policy_lane() {
    let engine = default_phase7_engine();
    let plan = engine
        .plan(&change_set(vec![ChangedPath::new("ops/ci/pipeline.yml")]))
        .expect("ops path owned and mapped");

    let lane_names: Vec<&str> = plan.lanes.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(lane_names, vec!["local-ci-parity"]);
}

#[test]
fn ownerless_path_blocks_merge() {
    let engine = default_phase7_engine();
    let err = engine
        .plan(&change_set(vec![ChangedPath::new("random/unowned.rs")]))
        .expect_err("paths with no owner rule must block merge");
    assert_eq!(
        err,
        ProofBlocker::OwnerlessPath("random/unowned.rs".to_string())
    );
}

#[test]
fn ownerless_path_is_reported_before_lane_checks() {
    // An engine with an owner rule but no test rule for the path: ownerless
    // check happens before lane mapping, so an unowned path yields OwnerlessPath
    // even though it also lacks a lane.
    let engine = ProofEngine::new(
        vec![OwnerRule {
            pattern: PathPattern::new("src/**"),
            owners: vec!["@team".to_string()],
        }],
        vec![TestRule {
            pattern: PathPattern::new("src/**"),
            lanes: vec!["unit".to_string()],
        }],
        Vec::new(),
        vec![ProofLane {
            name: "unit".to_string(),
            commands: vec!["cargo test".to_string()],
            required: true,
        }],
    );
    let err = engine
        .plan(&change_set(vec![ChangedPath::new("docs/readme.md")]))
        .expect_err("unowned path blocks first");
    assert_eq!(
        err,
        ProofBlocker::OwnerlessPath("docs/readme.md".to_string())
    );
}

#[test]
fn owned_but_unmapped_path_blocks_with_unmapped_proof_lane() {
    // Path has an owner but no test rule -> UnmappedProofLane(path).
    let engine = ProofEngine::new(
        vec![OwnerRule {
            pattern: PathPattern::new("src/**"),
            owners: vec!["@team".to_string()],
        }],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let err = engine
        .plan(&change_set(vec![ChangedPath::new("src/lib.rs")]))
        .expect_err("owned but unmapped path must block merge");
    assert_eq!(
        err,
        ProofBlocker::UnmappedProofLane("src/lib.rs".to_string())
    );
}

#[test]
fn lane_referenced_by_test_rule_but_undefined_blocks_with_unmapped_lane() {
    // A test rule names a lane that is not defined in the engine's lane map.
    // The blocker reports the missing LANE NAME (not the path).
    let engine = ProofEngine::new(
        vec![OwnerRule {
            pattern: PathPattern::new("src/**"),
            owners: vec!["@team".to_string()],
        }],
        vec![TestRule {
            pattern: PathPattern::new("src/**"),
            lanes: vec!["ghost-lane".to_string()],
        }],
        Vec::new(),
        // No "ghost-lane" defined.
        Vec::new(),
    );
    let err = engine
        .plan(&change_set(vec![ChangedPath::new("src/lib.rs")]))
        .expect_err("undefined lane must block merge");
    assert_eq!(
        err,
        ProofBlocker::UnmappedProofLane("ghost-lane".to_string())
    );
}

#[test]
fn first_blocking_path_short_circuits_the_plan() {
    let engine = default_phase7_engine();
    // First path is owned+mapped, second is ownerless. The ownerless path must
    // be the reported blocker (loop returns on first failure).
    let err = engine
        .plan(&change_set(vec![
            ChangedPath::new("crates/jeryu_core/src/a.rs"),
            ChangedPath::new("nowhere/x.rs"),
        ]))
        .expect_err("a single ownerless path blocks the whole plan");
    assert_eq!(err, ProofBlocker::OwnerlessPath("nowhere/x.rs".to_string()));
}

#[test]
fn empty_change_set_yields_an_empty_plan() {
    let engine = default_phase7_engine();
    let plan = engine
        .plan(&change_set(Vec::new()))
        .expect("empty change sets plan trivially");
    assert!(plan.paths.is_empty());
    assert!(plan.owners.is_empty());
    assert!(plan.lanes.is_empty());
}

#[test]
fn multiple_owner_rules_matching_one_path_union_their_owners() {
    // Two overlapping owner rules both match the same path; their owners union.
    let engine = ProofEngine::new(
        vec![
            OwnerRule {
                pattern: PathPattern::new("src/**"),
                owners: vec!["@core".to_string()],
            },
            OwnerRule {
                pattern: PathPattern::new("src/api/**"),
                owners: vec!["@api".to_string()],
            },
        ],
        vec![TestRule {
            pattern: PathPattern::new("src/**"),
            lanes: vec!["unit".to_string()],
        }],
        Vec::new(),
        vec![ProofLane {
            name: "unit".to_string(),
            commands: vec!["cargo test".to_string()],
            required: true,
        }],
    );
    let plan = engine
        .plan(&change_set(vec![ChangedPath::new("src/api/routes.rs")]))
        .expect("path owned by two overlapping rules");
    assert_eq!(plan.owners, vec!["@api".to_string(), "@core".to_string()]);
}

#[test]
fn plan_includes_required_and_non_required_lanes() {
    let engine = default_phase7_engine();
    // docs/** maps to the `docs` lane, which is defined but NOT required.
    let plan = engine
        .plan(&change_set(vec![ChangedPath::new("docs/intro.md")]))
        .expect("docs path owned and mapped");
    let docs_lane = plan
        .lanes
        .iter()
        .find(|l| l.name == "docs")
        .expect("docs lane present in plan");
    assert!(
        !docs_lane.required,
        "docs lane is informational, not required"
    );
}
