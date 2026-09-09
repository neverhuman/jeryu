use super::*;
use jeryu_runner_core::job::{NetworkPolicy, SecretPolicy, TokenPolicy};
use jeryu_runner_core::receipt::ReceiptStatus;
use jeryu_runner_core::trust::TrustTier;
use std::path::PathBuf;

fn job(id: usize) -> CoreJobRequest {
    CoreJobRequest {
        job_id: format!("job-{id}"),
        repo_id: "neverhuman/jeryu".to_string(),
        commit_sha: "abc123".to_string(),
        workspace: PathBuf::from("/tmp/jeryu-fleet-test"),
        command: "/bin/true".to_string(),
        args: Vec::new(),
        env: BTreeMap::new(),
        trust_tier: TrustTier::T2InternalBranch,
        requested_runner: Some(CoreRunnerClass::NativeRustClean),
        network_policy: NetworkPolicy::Deny,
        secret_policy: SecretPolicy::None,
        token_policy: TokenPolicy::ReadOnly,
        timeout_ms: 1_000,
        fork: false,
    }
}

#[test]
fn deterministic_fixture_registers_four_nodes_and_forty_slots() {
    let fleet = RunnerFleet::deterministic_fixture_with_mode(DispatchMode::Explain);
    let health = fleet.health();
    assert_eq!(health.len(), 4);
    assert_eq!(health.iter().map(|node| node.capacity).sum::<u32>(), 40);
    assert_eq!(
        health
            .iter()
            .map(|node| node.runner_id.as_str())
            .collect::<Vec<_>>(),
        vec!["xbabe0", "xbabe1", "xbabe2", "xbabe3"]
    );
}

#[test]
fn forty_job_fanout_uses_all_four_nodes_at_ten_slots_each() {
    let mut fleet = RunnerFleet::deterministic_fixture_with_mode(DispatchMode::Explain);
    let mut counts = BTreeMap::<String, usize>::new();
    for id in 0..40 {
        let reserved = fleet.reserve_job(job(id)).expect("reserve");
        *counts.entry(reserved.runner_id).or_default() += 1;
    }
    assert_eq!(counts["xbabe0"], 10);
    assert_eq!(counts["xbabe1"], 10);
    assert_eq!(counts["xbabe2"], 10);
    assert_eq!(counts["xbabe3"], 10);
}

#[test]
fn reaped_node_stale_completion_is_fenced_and_work_reassigns() {
    let mut fleet = RunnerFleet::deterministic_fixture_with_mode(DispatchMode::Explain);
    let stale = fleet.reserve_job(job(1)).expect("first reserve");
    assert_eq!(stale.runner_id, "xbabe0");
    for survivor in ["xbabe1", "xbabe2", "xbabe3"] {
        assert!(fleet.heartbeat(survivor, 20).still_owner);
    }
    let reaped = fleet.reap(20);
    assert_eq!(reaped.len(), 1);
    assert_eq!(reaped[0].node_id, "xbabe0");

    let err = fleet
        .run_reserved(stale.clone())
        .expect_err("stale completion must be fenced");
    assert!(matches!(err, FleetError::FencedOut { runner_id, .. } if runner_id == "xbabe0"));

    let replacement = fleet.reserve_job(stale.job).expect("reassign");
    assert_ne!(replacement.runner_id, "xbabe0");
    let completed = fleet.run_reserved(replacement).expect("run reassigned");
    assert_eq!(completed.receipt.status, ReceiptStatus::Planned);
}

#[test]
fn drain_blocks_new_assignments_but_inflight_work_finishes() {
    let mut fleet = RunnerFleet::deterministic_fixture_with_mode(DispatchMode::Explain);
    let inflight = fleet.reserve_job(job(1)).expect("reserve");
    assert_eq!(inflight.runner_id, "xbabe0");
    assert!(fleet.drain("xbabe0"));

    for id in 2..12 {
        let reserved = fleet.reserve_job(job(id)).expect("reserve after drain");
        assert_ne!(reserved.runner_id, "xbabe0");
    }

    let completed = fleet
        .run_reserved(inflight)
        .expect("draining node completes");
    assert_eq!(completed.receipt.status, ReceiptStatus::Planned);
}
