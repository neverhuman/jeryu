use super::*;

fn root() -> PathBuf {
    PathBuf::from("/workspace/core/web")
}

fn rebased_startup() -> StartupSync {
    StartupSync::Rebased {
        main_ref: "origin/main".into(),
        base_sha: "abc123".into(),
        head_sha: "def456".into(),
    }
}

fn claim_request(
    agent_id: &str,
    branch_budget: u32,
    runner_id: &str,
    runner_epoch: u64,
    git_status_summary: &str,
    ci_snapshot_age_ms: Option<u64>,
) -> WorkcellClaimRequest {
    WorkcellClaimRequest {
        agent_id: agent_id.into(),
        workspace_root: root(),
        repo_roots: vec![root()],
        branch_budget,
        runner_id: runner_id.into(),
        runner_epoch,
        git_status_summary: git_status_summary.into(),
        ci_snapshot_age_ms,
        startup: rebased_startup(),
    }
}

struct FailedTreeFixture<'a> {
    agent_id: &'a str,
    branch_budget: u32,
    runner_id: &'a str,
    runner_epoch: u64,
    git_status_summary: &'a str,
    ci_snapshot_age_ms: Option<u64>,
    ci_run_id: &'a str,
    failed_run_id: &'a str,
    failed_receipt_id: &'a str,
    failure_log_digest: &'a str,
}

fn held_failed_tree_request(fixture: FailedTreeFixture<'_>) -> HoldFailedTreeRequest {
    HoldFailedTreeRequest {
        claim: claim_request(
            fixture.agent_id,
            fixture.branch_budget,
            fixture.runner_id,
            fixture.runner_epoch,
            fixture.git_status_summary,
            fixture.ci_snapshot_age_ms,
        ),
        ci_run_id: fixture.ci_run_id.into(),
        failed_run_id: fixture.failed_run_id.into(),
        failed_receipt_id: fixture.failed_receipt_id.into(),
        failure_log_digest: fixture.failure_log_digest.into(),
    }
}

#[test]
fn claim_replaces_warm_cell_and_assigns_branch_budget() {
    let mut manager = WorkcellManager::with_warm_pool(1);
    assert_eq!(manager.ready_count(), 1);

    let lease = manager
        .claim(claim_request(
            "agent-wrath-17",
            1,
            "xbabe0",
            7,
            "clean",
            Some(0),
        ))
        .expect("claim succeeds");

    assert_eq!(lease.state, WorkcellState::Claimed);
    assert_eq!(lease.branch_policy.max_branches, 1);
    assert_eq!(
        manager.ready_count(),
        1,
        "a replacement warm cell is spawned"
    );
    assert_eq!(
        manager
            .workcell(&lease.workcell_id)
            .unwrap()
            .startup_main_ref
            .as_deref(),
        Some("origin/main")
    );
}

#[test]
fn startup_rebase_failure_blocks_the_cell() {
    let mut manager = WorkcellManager::with_warm_pool(1);
    let err = manager
        .claim(WorkcellClaimRequest {
            startup: StartupSync::Failed {
                main_ref: "origin/main".into(),
                base_sha: "abc123".into(),
                head_sha: "def456".into(),
                reason: "rebase conflict".into(),
            },
            ..claim_request("agent-storm-04", 5, "xbabe1", 8, "dirty", Some(42))
        })
        .expect_err("rebase failure must block");

    assert_eq!(err.reason, "workcell_startup_rebase_failed");
    assert!(err.repair_hint.contains("workcell"));
}

#[test]
fn heartbeat_fences_stale_epochs_and_release_marks_released() {
    let mut manager = WorkcellManager::with_warm_pool(1);
    let lease = manager
        .claim(claim_request(
            "agent-wrath-17",
            1,
            "xbabe0",
            7,
            "clean",
            None,
        ))
        .expect("claim succeeds");

    let fence = manager
        .heartbeat(&lease.workcell_id, lease.runner_epoch + 1, true)
        .expect_err("stale epoch must fence");
    assert_eq!(fence.reason, "workcell_epoch_fenced");

    manager
        .heartbeat(&lease.workcell_id, lease.runner_epoch, true)
        .expect("matching epoch heartbeat succeeds");
    manager
        .release(&lease.workcell_id, lease.runner_epoch)
        .expect("release succeeds");
    assert_eq!(
        manager.workcell(&lease.workcell_id).unwrap().state,
        WorkcellState::Released
    );
}

#[test]
fn frozen_ci_snapshot_is_immutable_and_repair_uses_it() {
    let mut manager = WorkcellManager::with_warm_pool(1);
    let lease = manager
        .claim(claim_request(
            "agent-wrath-17",
            1,
            "xbabe0",
            7,
            "clean",
            Some(100),
        ))
        .expect("claim succeeds");

    let frozen = manager
        .freeze_failed_ci_run(
            &lease.workcell_id,
            lease.runner_epoch,
            FreezeFailedCiRunRequest {
                ci_run_id: "ci-17".into(),
                failed_run_id: "run-17".into(),
                failed_receipt_id: "receipt-17".into(),
                failure_log_digest: "sha256:deadbeef".into(),
                snapshot_age_ms: 1_200,
            },
        )
        .expect("freeze succeeds");
    let frozen_before = frozen.clone();
    let repair = manager
        .repair_from_snapshot(
            &frozen,
            StartupSync::Rebased {
                main_ref: "origin/main".into(),
                base_sha: "def456".into(),
                head_sha: "fedcba".into(),
            },
        )
        .expect("repair claim succeeds");

    assert_eq!(frozen, frozen_before, "frozen snapshot must stay immutable");
    assert_eq!(repair.state, WorkcellState::Held);
    assert!(repair.frozen_snapshot.is_some());
    assert_eq!(repair.frozen_snapshot.as_ref().unwrap().ci_run_id, "ci-17");

    let repairing = manager
        .begin_live_repair(&repair.workcell_id, repair.runner_epoch)
        .expect("repair may start after hold");
    assert_eq!(repairing.state, WorkcellState::Repairing);
}

#[test]
fn hold_failed_tree_preserves_distinct_ci_run_identity() {
    let mut manager = WorkcellManager::with_warm_pool(1);
    let held = manager
        .hold_failed_tree(held_failed_tree_request(FailedTreeFixture {
            agent_id: "agent-wrath-17",
            branch_budget: 1,
            runner_id: "xbabe0",
            runner_epoch: 7,
            git_status_summary: "failed run tree",
            ci_snapshot_age_ms: Some(100),
            ci_run_id: "ci-parent-17",
            failed_run_id: "run-attempt-17",
            failed_receipt_id: "receipt-17",
            failure_log_digest: "sha256:deadbeef",
        }))
        .expect("hold failed tree succeeds");

    let snapshot = held.frozen_snapshot.as_ref().expect("snapshot stored");
    assert_eq!(snapshot.ci_run_id, "ci-parent-17");
    assert_eq!(snapshot.failed_run_id, "run-attempt-17");
    assert_ne!(snapshot.ci_run_id, snapshot.failed_run_id);
}

#[test]
fn branch_budget_defaults_to_one_and_caps_at_five() {
    let mut one = BranchPolicy::new("agent-a", "wc-1", 0);
    assert_eq!(one.max_branches, 1);
    assert!(one.open_branch("fix-1").is_ok());
    assert!(one.open_branch("fix-2").is_err());

    let mut five = BranchPolicy::new("agent-a", "wc-2", 9);
    assert_eq!(five.max_branches, 5);
    for idx in 0..5 {
        assert!(
            five.open_branch(format!("branch-{idx}")).is_ok(),
            "branch budget should allow branch {idx}"
        );
    }
    assert!(five.open_branch("branch-6").is_err());
}

#[test]
fn merge_and_delete_are_denied() {
    let policy = BranchPolicy::new("agent-a", "wc-3", 1);
    assert_eq!(
        policy.allow_merge().unwrap_err().reason,
        "workcell_merge_denied"
    );
    assert_eq!(
        policy.allow_delete().unwrap_err().reason,
        "workcell_delete_denied"
    );
}

#[test]
fn tar_helpers_reject_traversal_symlink_and_special_files() {
    let allowed_roots = vec![PathBuf::from("/workspace/core/web")];
    let destination = PathBuf::from("/workspace/core/web");

    assert!(
        validate_import_archive(
            &[ArchiveEntry::new("src/lib.rs", ArchiveEntryKind::File)],
            &destination,
            &allowed_roots,
        )
        .is_ok()
    );

    for entry in [
        ArchiveEntry::new("../escape", ArchiveEntryKind::File),
        ArchiveEntry::new("/abs/path", ArchiveEntryKind::File),
        ArchiveEntry::new("src/link", ArchiveEntryKind::Symlink),
        ArchiveEntry::new("src/hard", ArchiveEntryKind::Hardlink),
        ArchiveEntry::new("dev/tty", ArchiveEntryKind::CharacterDevice),
        ArchiveEntry::new("dev/sda", ArchiveEntryKind::BlockDevice),
        ArchiveEntry::new("tmp/fifo", ArchiveEntryKind::Fifo),
        ArchiveEntry::new("tmp/socket", ArchiveEntryKind::Socket),
    ] {
        let err = validate_import_archive(&[entry], &destination, &allowed_roots)
            .expect_err("unsafe archive entry must be denied");
        assert_eq!(err.reason, "workcell_tar_path_denied");
    }

    assert!(
        validate_export_paths(
            &[PathBuf::from("/workspace/core/web/src/lib.rs")],
            &allowed_roots
        )
        .is_ok()
    );

    let err = validate_export_paths(
        &[PathBuf::from("/workspace/core/api/src/lib.rs")],
        &allowed_roots,
    )
    .expect_err("export outside repo roots must be denied");
    assert_eq!(err.reason, "workcell_tar_path_denied");
}

#[test]
fn branch_budget_exhaustion_is_denied_through_export() {
    let mut manager = WorkcellManager::with_warm_pool(1);
    let held = manager
        .hold_failed_tree(held_failed_tree_request(FailedTreeFixture {
            agent_id: "agent-wrath-17",
            branch_budget: 2,
            runner_id: "xbabe0",
            runner_epoch: 7,
            git_status_summary: "failed tree",
            ci_snapshot_age_ms: Some(0),
            ci_run_id: "ci-1",
            failed_run_id: "run-1",
            failed_receipt_id: "receipt-1",
            failure_log_digest: "sha256:dead",
        }))
        .expect("hold succeeds");
    let id = held.workcell_id.clone();
    let epoch = held.runner_epoch;

    manager
        .export_repair_branch(&id, epoch, "fix-1")
        .expect("first branch within budget");
    manager
        .export_repair_branch(&id, epoch, "fix-2")
        .expect("second branch within budget");
    let err = manager
        .export_repair_branch(&id, epoch, "fix-3")
        .expect_err("third branch exhausts the budget");
    assert_eq!(err.reason, "workcell_branch_budget_denied");
}

#[test]
fn two_claims_get_distinct_cells_and_stale_epoch_loser_is_fenced() {
    let mut manager = WorkcellManager::with_warm_pool(1);
    let base = |agent: &str, epoch: u64| claim_request(agent, 1, "xbabe0", epoch, "clean", Some(0));

    let first = manager.claim(base("agent-a", 7)).expect("first claim");
    let second = manager.claim(base("agent-b", 9)).expect("second claim");
    assert_ne!(
        first.workcell_id, second.workcell_id,
        "two claims must not collide on a workcell id"
    );

    let fenced = manager
        .heartbeat(&first.workcell_id, first.runner_epoch + 1, true)
        .expect_err("a stale epoch loses the race");
    assert_eq!(fenced.reason, "workcell_epoch_fenced");
    manager
        .heartbeat(&first.workcell_id, first.runner_epoch, true)
        .expect("the live epoch still wins");
}

#[test]
fn release_with_stale_epoch_is_fenced() {
    let mut manager = WorkcellManager::with_warm_pool(1);
    let lease = manager
        .claim(claim_request(
            "agent-wrath-17",
            1,
            "xbabe0",
            7,
            "clean",
            Some(0),
        ))
        .expect("claim succeeds");

    let err = manager
        .release(&lease.workcell_id, lease.runner_epoch + 1)
        .expect_err("stale-epoch release must be fenced");
    assert_eq!(err.reason, "workcell_epoch_fenced");
    assert_ne!(
        manager.workcell(&lease.workcell_id).unwrap().state,
        WorkcellState::Released,
        "a fenced release must NOT transition the cell"
    );

    manager
        .release(&lease.workcell_id, lease.runner_epoch)
        .expect("the live epoch releases for real");
    assert_eq!(
        manager.workcell(&lease.workcell_id).unwrap().state,
        WorkcellState::Released
    );
}

#[test]
fn tar_import_rejects_hardlink_fifo_and_socket_entries() {
    let allowed_roots = vec![root()];
    let destination = root();
    for (label, entry) in [
        (
            "hardlink",
            ArchiveEntry::new("src/hard", ArchiveEntryKind::Hardlink),
        ),
        (
            "fifo",
            ArchiveEntry::new("tmp/fifo", ArchiveEntryKind::Fifo),
        ),
        (
            "socket",
            ArchiveEntry::new("tmp/socket", ArchiveEntryKind::Socket),
        ),
    ] {
        let err = validate_import_archive(&[entry], &destination, &allowed_roots).unwrap_err();
        assert_eq!(
            err.reason, "workcell_tar_path_denied",
            "{label} entry must be denied by kind"
        );
    }
}
