//! Integration tests for generated-zone edit rules and blocker messages.
//!
//! Generated zones (e.g. build outputs) deny direct AGENT edits unless the zone
//! explicitly opts into agent edits. Human-authored ("agent_authored = false")
//! changes are never denied by zone rules. These tests pin that gating plus the
//! human-readable blocker messages used for merge-blocking surfaces.

use jeryu_proof::{
    ChangeSet, GeneratedZone, OwnerRule, PathPattern, ProofBlocker, ProofEngine, ProofLane,
    TestRule,
};

use jeryu_core::phase7::{ChangedPath, PullRequestId, RepoId};

/// Engine that owns and maps `src/**` and `target/**`, with `target/**` a
/// no-agent-edit generated zone.
fn zone_engine() -> ProofEngine {
    ProofEngine::new(
        vec![
            OwnerRule {
                pattern: PathPattern::new("src/**"),
                owners: vec!["@team".to_string()],
            },
            OwnerRule {
                pattern: PathPattern::new("target/**"),
                owners: vec!["@team".to_string()],
            },
        ],
        vec![
            TestRule {
                pattern: PathPattern::new("src/**"),
                lanes: vec!["unit".to_string()],
            },
            TestRule {
                pattern: PathPattern::new("target/**"),
                lanes: vec!["unit".to_string()],
            },
        ],
        vec![GeneratedZone {
            pattern: PathPattern::new("target/**"),
            allow_agent_edits: false,
            reason: "Build outputs are generated.".to_string(),
        }],
        vec![ProofLane {
            name: "unit".to_string(),
            commands: vec!["cargo test".to_string()],
            required: true,
        }],
    )
}

fn change(paths: Vec<ChangedPath>, agent_authored: bool) -> ChangeSet {
    ChangeSet {
        repo: RepoId::new("repo_phase7"),
        pr: PullRequestId::new("pr_1"),
        head_sha: "head001".to_string(),
        paths,
        agent_authored,
    }
}

#[test]
fn agent_edit_to_generated_zone_is_denied() {
    let engine = zone_engine();
    let err = engine
        .plan(&change(
            vec![ChangedPath::new("target/debug/app")],
            /* agent_authored = */ true,
        ))
        .expect_err("agent edits to generated zones must be denied");
    assert_eq!(
        err,
        ProofBlocker::GeneratedZoneEditDenied {
            path: "target/debug/app".to_string(),
            reason: "Build outputs are generated.".to_string(),
        }
    );
}

#[test]
fn human_edit_to_generated_zone_is_permitted() {
    let engine = zone_engine();
    // A non-agent author touching the same generated zone is NOT denied by the
    // zone rule; it proceeds through owner/lane resolution.
    let plan = engine
        .plan(&change(
            vec![ChangedPath::new("target/debug/app")],
            /* agent_authored = */ false,
        ))
        .expect("human edits to generated zones are allowed by zone rules");
    assert_eq!(plan.paths, vec!["target/debug/app"]);
    assert_eq!(plan.owners, vec!["@team".to_string()]);
}

#[test]
fn agent_edit_to_non_zone_path_is_permitted() {
    let engine = zone_engine();
    let plan = engine
        .plan(&change(
            vec![ChangedPath::new("src/lib.rs")],
            /* agent_authored = */ true,
        ))
        .expect("agent edits outside generated zones are allowed");
    assert_eq!(plan.paths, vec!["src/lib.rs"]);
}

#[test]
fn allow_agent_edits_zone_permits_agent_edits() {
    // A generated zone that opts into agent edits must not block agents.
    let engine = ProofEngine::new(
        vec![OwnerRule {
            pattern: PathPattern::new("gen/**"),
            owners: vec!["@team".to_string()],
        }],
        vec![TestRule {
            pattern: PathPattern::new("gen/**"),
            lanes: vec!["unit".to_string()],
        }],
        vec![GeneratedZone {
            pattern: PathPattern::new("gen/**"),
            allow_agent_edits: true,
            reason: "Regenerated but agent-curated.".to_string(),
        }],
        vec![ProofLane {
            name: "unit".to_string(),
            commands: vec!["cargo test".to_string()],
            required: true,
        }],
    );
    let plan = engine
        .plan(&change(
            vec![ChangedPath::new("gen/schema.rs")],
            /* agent_authored = */ true,
        ))
        .expect("agent-editable generated zones do not block agents");
    assert_eq!(plan.paths, vec!["gen/schema.rs"]);
}

#[test]
fn generated_zone_denial_precedes_owner_and_lane_checks() {
    // A generated-zone path that is BOTH ownerless and unmapped must still be
    // reported as a zone-edit denial when agent-authored: the zone check runs
    // first in the per-path loop.
    let engine = ProofEngine::new(
        Vec::new(), // no owners
        Vec::new(), // no lanes
        vec![GeneratedZone {
            pattern: PathPattern::new("target/**"),
            allow_agent_edits: false,
            reason: "generated".to_string(),
        }],
        Vec::new(),
    );
    let err = engine
        .plan(&change(
            vec![ChangedPath::new("target/x")],
            /* agent_authored = */ true,
        ))
        .expect_err("zone denial wins over ownerless/unmapped");
    assert_eq!(
        err,
        ProofBlocker::GeneratedZoneEditDenied {
            path: "target/x".to_string(),
            reason: "generated".to_string(),
        }
    );
}

#[test]
fn blocker_messages_are_human_readable() {
    assert_eq!(
        ProofBlocker::OwnerlessPath("a/b.rs".to_string()).message(),
        "ownerless path blocks merge: a/b.rs"
    );
    assert_eq!(
        ProofBlocker::UnmappedProofLane("a/b.rs".to_string()).message(),
        "unmapped proof lane blocks merge: a/b.rs"
    );
    assert_eq!(
        ProofBlocker::GeneratedZoneEditDenied {
            path: "target/x".to_string(),
            reason: "generated".to_string(),
        }
        .message(),
        "generated zone edit denied for target/x: generated"
    );
    assert_eq!(
        ProofBlocker::MissingEvidence("unit".to_string()).message(),
        "missing proof evidence for lane: unit"
    );
    assert_eq!(
        ProofBlocker::FailedLane("unit".to_string()).message(),
        "proof lane failed: unit"
    );
}
