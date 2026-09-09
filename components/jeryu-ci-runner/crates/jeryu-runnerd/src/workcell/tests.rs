use super::*;

fn root() -> PathBuf {
    PathBuf::from("/workspace/core/web")
}

#[test]
fn claim_replaces_warm_cell_and_assigns_branch_budget() {
    let mut manager = WorkcellManager::with_warm_pool(1);
    assert_eq!(manager.ready_count(), 1);

    let lease = manager
        .claim(WorkcellClaimRequest {
            agent_id: "agent-wrath-17".into(),
            workspace_root: root(),
            repo_roots: vec![root()],
            branch_budget: 1,
            runner_id: "xbabe0".into(),
            runner_epoch: 7,
            git_status_summary: "clean".into(),
            ci_snapshot_age_ms: Some(0),
            startup: StartupSync::Rebased {
                main_ref: "origin/main".into(),
                base_sha: "abc123".into(),
                head_sha: "def456".into(),
            },
        })
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
            agent_id: "agent-storm-04".into(),
            workspace_root: root(),
            repo_roots: vec![root()],
            branch_budget: 5,
            runner_id: "xbabe1".into(),
            runner_epoch: 8,
            git_status_summary: "dirty".into(),
            ci_snapshot_age_ms: Some(42),
            startup: StartupSync::Failed {
                main_ref: "origin/main".into(),
                base_sha: "abc123".into(),
                head_sha: "def456".into(),
                reason: "rebase conflict".into(),
            },
        })
        .expect_err("rebase failure must block");

    assert_eq!(err.reason, "workcell_startup_rebase_failed");
    assert!(err.repair_hint.contains("workcell"));
}

#[test]
fn heartbeat_fences_outdated_epochs_and_release_marks_released() {
    let mut manager = WorkcellManager::with_warm_pool(1);
    let lease = manager
        .claim(WorkcellClaimRequest {
            agent_id: "agent-wrath-17".into(),
            workspace_root: root(),
            repo_roots: vec![root()],
            branch_budget: 1,
            runner_id: "xbabe0".into(),
            runner_epoch: 7,
            git_status_summary: "clean".into(),
            ci_snapshot_age_ms: None,
            startup: StartupSync::Rebased {
                main_ref: "origin/main".into(),
                base_sha: "abc123".into(),
                head_sha: "def456".into(),
            },
        })
        .expect("claim succeeds");

    let fence = manager
        .heartbeat(&lease.workcell_id, lease.runner_epoch + 1, true)
        .expect_err("outdated epoch must fence");
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
        .claim(WorkcellClaimRequest {
            agent_id: "agent-wrath-17".into(),
            workspace_root: root(),
            repo_roots: vec![root()],
            branch_budget: 1,
            runner_id: "xbabe0".into(),
            runner_epoch: 7,
            git_status_summary: "clean".into(),
            ci_snapshot_age_ms: Some(100),
            startup: StartupSync::Rebased {
                main_ref: "origin/main".into(),
                base_sha: "abc123".into(),
                head_sha: "def456".into(),
            },
        })
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
        .hold_failed_tree(HoldFailedTreeRequest {
            claim: WorkcellClaimRequest {
                agent_id: "agent-wrath-17".into(),
                workspace_root: root(),
                repo_roots: vec![root()],
                branch_budget: 1,
                runner_id: "xbabe0".into(),
                runner_epoch: 7,
                git_status_summary: "failed run tree".into(),
                ci_snapshot_age_ms: Some(100),
                startup: StartupSync::Rebased {
                    main_ref: "origin/main".into(),
                    base_sha: "abc123".into(),
                    head_sha: "def456".into(),
                },
            },
            ci_run_id: "ci-parent-17".into(),
            failed_run_id: "run-attempt-17".into(),
            failed_receipt_id: "receipt-17".into(),
            failure_log_digest: "sha256:deadbeef".into(),
        })
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
            &allowed_roots,
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

// A held cell with branch_budget 2 accepts two repair branches, then fences
// the third: agents cannot mint unbounded branches from a single cell.
#[test]
fn branch_budget_exhaustion_is_denied_through_export() {
    let mut manager = WorkcellManager::with_warm_pool(1);
    let held = manager
        .hold_failed_tree(HoldFailedTreeRequest {
            claim: WorkcellClaimRequest {
                agent_id: "agent-wrath-17".into(),
                workspace_root: root(),
                repo_roots: vec![root()],
                branch_budget: 2,
                runner_id: "xbabe0".into(),
                runner_epoch: 7,
                git_status_summary: "failed tree".into(),
                ci_snapshot_age_ms: Some(0),
                startup: StartupSync::Rebased {
                    main_ref: "origin/main".into(),
                    base_sha: "abc123".into(),
                    head_sha: "def456".into(),
                },
            },
            ci_run_id: "ci-1".into(),
            failed_run_id: "run-1".into(),
            failed_receipt_id: "receipt-1".into(),
            failure_log_digest: "sha256:dead".into(),
        })
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

// WorkcellManager is a synchronous &mut-self struct, so two claims never
// collide on an id. The real "one writer wins" guarantee is epoch fencing:
// an outdated-epoch op against the live cell is rejected while the live epoch
// still works.
#[test]
fn two_claims_get_distinct_cells_and_outdated_epoch_loser_is_fenced() {
    let mut manager = WorkcellManager::with_warm_pool(1);
    let base = |agent: &str, epoch: u64| WorkcellClaimRequest {
        agent_id: agent.into(),
        workspace_root: root(),
        repo_roots: vec![root()],
        branch_budget: 1,
        runner_id: "xbabe0".into(),
        runner_epoch: epoch,
        git_status_summary: "clean".into(),
        ci_snapshot_age_ms: Some(0),
        startup: StartupSync::Rebased {
            main_ref: "origin/main".into(),
            base_sha: "abc123".into(),
            head_sha: "def456".into(),
        },
    };

    let first = manager.claim(base("agent-a", 7)).expect("first claim");
    let second = manager.claim(base("agent-b", 9)).expect("second claim");
    assert_ne!(
        first.workcell_id, second.workcell_id,
        "two claims must not collide on a workcell id"
    );

    let fenced = manager
        .heartbeat(&first.workcell_id, first.runner_epoch + 1, true)
        .expect_err("an outdated epoch loses the race");
    assert_eq!(fenced.reason, "workcell_epoch_fenced");
    manager
        .heartbeat(&first.workcell_id, first.runner_epoch, true)
        .expect("the live epoch still wins");
}

// A release carrying the wrong runner epoch is fenced AND leaves the cell
// un-transitioned; the live epoch then releases for real.
#[test]
fn release_with_outdated_epoch_is_fenced() {
    let mut manager = WorkcellManager::with_warm_pool(1);
    let lease = manager
        .claim(WorkcellClaimRequest {
            agent_id: "agent-wrath-17".into(),
            workspace_root: root(),
            repo_roots: vec![root()],
            branch_budget: 1,
            runner_id: "xbabe0".into(),
            runner_epoch: 7,
            git_status_summary: "clean".into(),
            ci_snapshot_age_ms: Some(0),
            startup: StartupSync::Rebased {
                main_ref: "origin/main".into(),
                base_sha: "abc123".into(),
                head_sha: "def456".into(),
            },
        })
        .expect("claim succeeds");

    let err = manager
        .release(&lease.workcell_id, lease.runner_epoch + 1)
        .expect_err("outdated-epoch release must be fenced");
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

// Special tar entry kinds (hardlink/fifo/socket) are rejected by KIND on
// import, independent of path — they can never smuggle past the validator.
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
