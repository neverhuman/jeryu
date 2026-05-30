//! Branch-protection enforcement tests.
//!
//! These exercise `ForgeCore::evaluate_pull_request`, `merge_pull_request`, and
//! `evaluate_ref_operation` against required status checks, required approving
//! reviews, the jankurai-proof gate, the SHA-mismatch (optimistic concurrency)
//! guard, and the GitHub-correctness rules added in this fix:
//! `required_linear_history`, `require_signed_commits`, `allow_force_pushes`,
//! `allow_deletions`, `enforce_admins`, and CODEOWNERS. Each rule has a matching
//! positive merge-blocking test asserting the correct `MergeBlocker`.

use jeryu_core::{
    CheckConclusion, CheckRunStatus, CommitStatusState, CreateCheckRunRequest,
    CreateCommitStatusRequest, CreatePullRequestRequest, CreateRepositoryRequest,
    CreateReviewRequest, CreateUserRequest, ForgeCore, ForgeError, MergeBlocker,
    MergePullRequestRequest, PullRequestCommit, RefOperation, RefOperationBlocker, ReviewState,
    SetBranchProtectionRequest,
};

fn core_with_repo() -> ForgeCore {
    let core = ForgeCore::new();
    core.create_user(CreateUserRequest {
        login: "alice".to_string(),
        ..Default::default()
    })
    .unwrap();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            default_branch: Some("main".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    core
}

fn open_pr(core: &ForgeCore, head_sha: &str) -> u64 {
    core.create_pull_request(
        "alice",
        "jeryu",
        "alice",
        CreatePullRequestRequest {
            title: "change".to_string(),
            head: "feature".to_string(),
            base: "main".to_string(),
            head_sha: Some(head_sha.to_string()),
            ..Default::default()
        },
    )
    .unwrap()
    .number
}

fn eval(core: &ForgeCore, number: u64) -> jeryu_core::BranchProtectionEvaluation {
    core.evaluate_pull_request("alice", "jeryu", number, None)
        .unwrap()
}

/// Opens a PR carrying a specific commit list and changed-file set, for the
/// history / signature / CODEOWNERS gates.
fn open_pr_with(
    core: &ForgeCore,
    head_sha: &str,
    commits: Vec<PullRequestCommit>,
    changed_files: Vec<String>,
) -> u64 {
    core.create_pull_request(
        "alice",
        "jeryu",
        "alice",
        CreatePullRequestRequest {
            title: "change".to_string(),
            head: "feature".to_string(),
            base: "main".to_string(),
            head_sha: Some(head_sha.to_string()),
            commits,
            changed_files,
            ..Default::default()
        },
    )
    .unwrap()
    .number
}

fn signed(sha: &str) -> PullRequestCommit {
    PullRequestCommit {
        sha: sha.to_string(),
        verified: true,
        parents: 1,
    }
}

fn unsigned(sha: &str) -> PullRequestCommit {
    PullRequestCommit {
        sha: sha.to_string(),
        verified: false,
        parents: 1,
    }
}

fn merge_commit(sha: &str) -> PullRequestCommit {
    PullRequestCommit {
        sha: sha.to_string(),
        verified: true,
        parents: 2,
    }
}

fn approve(core: &ForgeCore, number: u64, who: &str) {
    core.create_review(
        "alice",
        "jeryu",
        number,
        who,
        CreateReviewRequest {
            body: None,
            event: ReviewState::Approved,
            comments: vec![],
        },
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Rule round-trip + persistence
// ---------------------------------------------------------------------------

#[test]
fn set_and_get_branch_protection_roundtrips() {
    let core = core_with_repo();
    let rule = core
        .set_branch_protection(
            "alice",
            "jeryu",
            "main",
            SetBranchProtectionRequest {
                required_status_checks: vec!["ci/fast".to_string(), "ci/slow".to_string()],
                required_approving_review_count: 2,
                enforce_admins: true,
                required_linear_history: true,
                allow_force_pushes: false,
                allow_deletions: false,
                require_signed_commits: true,
                require_jankurai_proof: true,
            },
        )
        .unwrap();
    assert_eq!(rule.branch, "main");
    assert_eq!(rule.required_approving_review_count, 2);
    assert!(rule.enforce_admins);
    assert!(rule.required_linear_history);
    assert!(rule.require_signed_commits);
    assert!(!rule.allow_force_pushes);
    assert!(!rule.allow_deletions);

    let fetched = core
        .get_branch_protection("alice", "jeryu", "main")
        .unwrap();
    assert_eq!(fetched, rule);
}

#[test]
fn get_branch_protection_missing_is_not_found() {
    let core = core_with_repo();
    let err = core
        .get_branch_protection("alice", "jeryu", "main")
        .unwrap_err();
    assert!(matches!(err, ForgeError::NotFound(_)));
}

#[test]
fn set_branch_protection_on_missing_repo_is_not_found() {
    let core = ForgeCore::new();
    let err = core
        .set_branch_protection(
            "nobody",
            "void",
            "main",
            SetBranchProtectionRequest::default(),
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::NotFound(_)));
}

#[test]
fn empty_protected_branch_name_is_rejected() {
    let core = core_with_repo();
    let err = core
        .set_branch_protection(
            "alice",
            "jeryu",
            "  ",
            SetBranchProtectionRequest::default(),
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::Validation(_)));
}

// ---------------------------------------------------------------------------
// No protection -> clean
// ---------------------------------------------------------------------------

#[test]
fn unprotected_branch_evaluates_clean() {
    let core = core_with_repo();
    let number = open_pr(&core, "abc");
    let e = eval(&core, number);
    assert!(e.mergeable);
    assert_eq!(e.state, "clean");
    assert!(e.blockers.is_empty());
}

// ---------------------------------------------------------------------------
// Required reviews (negative)
// ---------------------------------------------------------------------------

#[test]
fn missing_required_review_produces_missing_review_blocker() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            required_approving_review_count: 2,
            ..Default::default()
        },
    )
    .unwrap();
    let number = open_pr(&core, "abc");

    let e = eval(&core, number);
    assert!(!e.mergeable);
    assert_eq!(e.state, "blocked");
    assert!(e.blockers.iter().any(|b| matches!(
        b,
        MergeBlocker::MissingReview {
            required: 2,
            approved: 0
        }
    )));
}

#[test]
fn partial_approvals_reported_in_blocker_counts() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            required_approving_review_count: 3,
            ..Default::default()
        },
    )
    .unwrap();
    let number = open_pr(&core, "abc");
    for who in ["a", "b"] {
        core.create_review(
            "alice",
            "jeryu",
            number,
            who,
            CreateReviewRequest {
                body: None,
                event: ReviewState::Approved,
                comments: vec![],
            },
        )
        .unwrap();
    }
    let e = eval(&core, number);
    assert!(e.blockers.iter().any(|b| matches!(
        b,
        MergeBlocker::MissingReview {
            required: 3,
            approved: 2
        }
    )));
}

// ---------------------------------------------------------------------------
// Required status checks (negative + positive)
// ---------------------------------------------------------------------------

#[test]
fn missing_status_check_produces_missing_blocker() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            required_status_checks: vec!["ci/fast".to_string()],
            ..Default::default()
        },
    )
    .unwrap();
    let number = open_pr(&core, "abc");
    let e = eval(&core, number);
    assert!(e.blockers.iter().any(|b| matches!(
        b,
        MergeBlocker::MissingStatusCheck { context } if context == "ci/fast"
    )));
}

#[test]
fn failing_status_check_produces_failed_blocker() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            required_status_checks: vec!["ci/fast".to_string()],
            ..Default::default()
        },
    )
    .unwrap();
    let number = open_pr(&core, "abc");
    core.create_commit_status(
        "alice",
        "jeryu",
        "abc",
        "ci-bot",
        CreateCommitStatusRequest {
            state: CommitStatusState::Failure,
            context: "ci/fast".to_string(),
            description: None,
            target_url: None,
        },
    )
    .unwrap();
    let e = eval(&core, number);
    assert!(e.blockers.iter().any(|b| matches!(
        b,
        MergeBlocker::FailedStatusCheck { context } if context == "ci/fast"
    )));
}

#[test]
fn latest_status_for_a_context_wins() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            required_status_checks: vec!["ci/fast".to_string()],
            ..Default::default()
        },
    )
    .unwrap();
    let number = open_pr(&core, "abc");
    // First a failure, then a later success for the same context.
    core.create_commit_status(
        "alice",
        "jeryu",
        "abc",
        "ci-bot",
        CreateCommitStatusRequest {
            state: CommitStatusState::Failure,
            context: "ci/fast".to_string(),
            description: None,
            target_url: None,
        },
    )
    .unwrap();
    core.create_commit_status(
        "alice",
        "jeryu",
        "abc",
        "ci-bot",
        CreateCommitStatusRequest {
            state: CommitStatusState::Success,
            context: "ci/fast".to_string(),
            description: None,
            target_url: None,
        },
    )
    .unwrap();
    let e = eval(&core, number);
    assert!(e.mergeable, "most recent successful status should win");
}

#[test]
fn status_on_other_sha_does_not_satisfy_check() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            required_status_checks: vec!["ci/fast".to_string()],
            ..Default::default()
        },
    )
    .unwrap();
    let number = open_pr(&core, "abc");
    // Status posted against a DIFFERENT sha must not satisfy this PR's gate.
    core.create_commit_status(
        "alice",
        "jeryu",
        "different-sha",
        "ci-bot",
        CreateCommitStatusRequest {
            state: CommitStatusState::Success,
            context: "ci/fast".to_string(),
            description: None,
            target_url: None,
        },
    )
    .unwrap();
    let e = eval(&core, number);
    assert!(!e.mergeable);
    assert!(e.blockers.iter().any(|b| matches!(
        b,
        MergeBlocker::MissingStatusCheck { context } if context == "ci/fast"
    )));
}

// ---------------------------------------------------------------------------
// jankurai-proof gate (the forge's own required-proof rule)
// ---------------------------------------------------------------------------

#[test]
fn jankurai_proof_required_blocks_until_proof_succeeds() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            require_jankurai_proof: true,
            ..Default::default()
        },
    )
    .unwrap();
    let number = open_pr(&core, "abc");

    let e = eval(&core, number);
    assert!(!e.mergeable);
    assert!(
        e.blockers
            .iter()
            .any(|b| matches!(b, MergeBlocker::JankuraiProofRequired))
    );

    // A check run named "jankurai/proof" succeeding clears the gate.
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "jankurai/proof".to_string(),
            head_sha: "abc".to_string(),
            status: Some(CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Success),
            details_url: None,
            output: None,
        },
    )
    .unwrap();
    let e = eval(&core, number);
    assert!(e.mergeable);
}

// ---------------------------------------------------------------------------
// SHA mismatch guard
// ---------------------------------------------------------------------------

#[test]
fn evaluation_with_stale_sha_reports_mismatch() {
    let core = core_with_repo();
    let number = open_pr(&core, "fresh");
    let e = core
        .evaluate_pull_request("alice", "jeryu", number, Some("stale"))
        .unwrap();
    assert!(!e.mergeable);
    assert!(e.blockers.iter().any(|b| matches!(
        b,
        MergeBlocker::ShaMismatch { expected, actual }
            if expected == "fresh" && actual == "stale"
    )));
}

#[test]
fn evaluation_with_matching_sha_has_no_mismatch_blocker() {
    let core = core_with_repo();
    let number = open_pr(&core, "fresh");
    let e = core
        .evaluate_pull_request("alice", "jeryu", number, Some("fresh"))
        .unwrap();
    assert!(e.mergeable);
    assert!(
        !e.blockers
            .iter()
            .any(|b| matches!(b, MergeBlocker::ShaMismatch { .. }))
    );
}

// ---------------------------------------------------------------------------
// Combined rules accumulate multiple blockers
// ---------------------------------------------------------------------------

#[test]
fn multiple_unmet_rules_accumulate_distinct_blockers() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            required_status_checks: vec!["ci/fast".to_string(), "ci/slow".to_string()],
            required_approving_review_count: 1,
            require_jankurai_proof: true,
            ..Default::default()
        },
    )
    .unwrap();
    let number = open_pr(&core, "abc");
    let e = eval(&core, number);
    // 1 missing review + 2 missing checks + 1 jankurai-proof = 4 blockers.
    assert_eq!(e.blockers.len(), 4);
    assert!(!e.mergeable);
}

#[test]
fn evaluate_unknown_pr_is_not_found() {
    let core = core_with_repo();
    let err = core
        .evaluate_pull_request("alice", "jeryu", 9999, None)
        .unwrap_err();
    assert!(matches!(err, ForgeError::NotFound(_)));
}

// ---------------------------------------------------------------------------
// Merge respects every rule
// ---------------------------------------------------------------------------

#[test]
fn merge_blocked_message_names_the_blockers() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            required_approving_review_count: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let number = open_pr(&core, "abc");
    let err = core
        .merge_pull_request("alice", "jeryu", number, MergePullRequestRequest::default())
        .unwrap_err();
    match err {
        ForgeError::BranchProtection(msg) => assert!(msg.contains("MissingReview")),
        other => panic!("expected BranchProtection, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// required_linear_history (positive merge-block + clearing case)
// ---------------------------------------------------------------------------

#[test]
fn required_linear_history_blocks_merge_commit() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            required_linear_history: true,
            ..Default::default()
        },
    )
    .unwrap();
    // A PR whose history contains a merge commit (parents > 1) is non-linear.
    let number = open_pr_with(&core, "abc", vec![signed("c1"), merge_commit("m1")], vec![]);
    let e = eval(&core, number);
    assert!(!e.mergeable);
    assert!(e.blockers.iter().any(|b| matches!(
        b,
        MergeBlocker::NonLinearHistory { sha } if sha == "m1"
    )));
    // The merge itself is rejected with a branch-protection error.
    let err = core
        .merge_pull_request("alice", "jeryu", number, MergePullRequestRequest::default())
        .unwrap_err();
    assert!(matches!(err, ForgeError::BranchProtection(_)));
}

#[test]
fn required_linear_history_allows_linear_commits() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            required_linear_history: true,
            ..Default::default()
        },
    )
    .unwrap();
    let number = open_pr_with(&core, "abc", vec![signed("c1"), signed("c2")], vec![]);
    let e = eval(&core, number);
    assert!(e.mergeable, "all single-parent commits are linear");
    assert!(
        !e.blockers
            .iter()
            .any(|b| matches!(b, MergeBlocker::NonLinearHistory { .. }))
    );
}

// ---------------------------------------------------------------------------
// require_signed_commits (positive merge-block + clearing case)
// ---------------------------------------------------------------------------

#[test]
fn require_signed_commits_blocks_unsigned_commit() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            require_signed_commits: true,
            ..Default::default()
        },
    )
    .unwrap();
    let number = open_pr_with(
        &core,
        "abc",
        vec![signed("c1"), unsigned("c2"), unsigned("c3")],
        vec![],
    );
    let e = eval(&core, number);
    assert!(!e.mergeable);
    assert!(e.blockers.iter().any(|b| matches!(
        b,
        MergeBlocker::UnsignedCommits { unsigned } if unsigned == &["c2".to_string(), "c3".to_string()]
    )));
    let err = core
        .merge_pull_request("alice", "jeryu", number, MergePullRequestRequest::default())
        .unwrap_err();
    assert!(matches!(err, ForgeError::BranchProtection(_)));
}

#[test]
fn require_signed_commits_allows_all_verified() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            require_signed_commits: true,
            ..Default::default()
        },
    )
    .unwrap();
    let number = open_pr_with(&core, "abc", vec![signed("c1"), signed("c2")], vec![]);
    let e = eval(&core, number);
    assert!(e.mergeable);
    assert!(
        !e.blockers
            .iter()
            .any(|b| matches!(b, MergeBlocker::UnsignedCommits { .. }))
    );
}

// ---------------------------------------------------------------------------
// allow_force_pushes / allow_deletions (ref-operation gating)
// ---------------------------------------------------------------------------

#[test]
fn force_push_blocked_when_not_allowed() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            allow_force_pushes: false,
            ..Default::default()
        },
    )
    .unwrap();
    let e = core
        .evaluate_ref_operation("alice", "jeryu", "main", RefOperation::ForcePush, false)
        .unwrap();
    assert!(!e.allowed);
    assert!(
        e.blockers
            .contains(&RefOperationBlocker::ForcePushNotAllowed)
    );
    let err = core
        .force_push("alice", "jeryu", "main", false)
        .unwrap_err();
    assert!(matches!(err, ForgeError::BranchProtection(_)));
}

#[test]
fn force_push_allowed_when_enabled() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            allow_force_pushes: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(core.force_push("alice", "jeryu", "main", false).is_ok());
}

#[test]
fn deletion_blocked_when_not_allowed() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            allow_deletions: false,
            ..Default::default()
        },
    )
    .unwrap();
    let e = core
        .evaluate_ref_operation("alice", "jeryu", "main", RefOperation::Delete, false)
        .unwrap();
    assert!(!e.allowed);
    assert!(
        e.blockers
            .contains(&RefOperationBlocker::DeletionNotAllowed)
    );
    let err = core
        .delete_ref("alice", "jeryu", "main", false)
        .unwrap_err();
    assert!(matches!(err, ForgeError::BranchProtection(_)));
}

#[test]
fn deletion_allowed_when_enabled() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            allow_deletions: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(core.delete_ref("alice", "jeryu", "main", false).is_ok());
}

#[test]
fn ref_ops_on_unprotected_branch_are_allowed() {
    let core = core_with_repo();
    assert!(core.force_push("alice", "jeryu", "main", false).is_ok());
    assert!(core.delete_ref("alice", "jeryu", "main", false).is_ok());
}

// ---------------------------------------------------------------------------
// enforce_admins (the "Include administrators" toggle)
// ---------------------------------------------------------------------------

#[test]
fn enforce_admins_off_lets_admin_bypass_ref_ops() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            enforce_admins: false,
            allow_force_pushes: false,
            allow_deletions: false,
            ..Default::default()
        },
    )
    .unwrap();
    // Non-admin is blocked; admin bypasses because enforce_admins is off.
    assert!(core.force_push("alice", "jeryu", "main", false).is_err());
    assert!(core.force_push("alice", "jeryu", "main", true).is_ok());
    assert!(core.delete_ref("alice", "jeryu", "main", false).is_err());
    assert!(core.delete_ref("alice", "jeryu", "main", true).is_ok());
}

#[test]
fn enforce_admins_on_binds_admin_to_ref_ops() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            enforce_admins: true,
            allow_force_pushes: false,
            allow_deletions: false,
            ..Default::default()
        },
    )
    .unwrap();
    // With enforce_admins on, even an admin is blocked.
    assert!(core.force_push("alice", "jeryu", "main", true).is_err());
    assert!(core.delete_ref("alice", "jeryu", "main", true).is_err());
}

// ---------------------------------------------------------------------------
// CODEOWNERS owner-approval gate
// ---------------------------------------------------------------------------

#[test]
fn codeowners_blocks_until_owner_of_changed_path_approves() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest::default(),
    )
    .unwrap();
    core.set_codeowners("alice", "jeryu", "*.rs @rustlead\ndocs/ @docsowner\n")
        .unwrap();
    // Changes an owned path (`*.rs`) -> requires @rustlead's approval.
    let number = open_pr_with(
        &core,
        "abc",
        vec![signed("c1")],
        vec!["src/lib.rs".to_string()],
    );

    let e = eval(&core, number);
    assert!(!e.mergeable);
    assert!(e.blockers.iter().any(|b| matches!(
        b,
        MergeBlocker::MissingCodeOwnerReview { paths, owners }
            if paths == &["src/lib.rs".to_string()] && owners == &["rustlead".to_string()]
    )));
    let err = core
        .merge_pull_request("alice", "jeryu", number, MergePullRequestRequest::default())
        .unwrap_err();
    assert!(matches!(err, ForgeError::BranchProtection(_)));

    // The code owner approving clears the gate; a non-owner approval does not.
    approve(&core, number, "bystander");
    assert!(
        !eval(&core, number).mergeable,
        "non-owner approval is insufficient"
    );
    approve(&core, number, "rustlead");
    let e = eval(&core, number);
    assert!(e.mergeable);
    assert!(
        !e.blockers
            .iter()
            .any(|b| matches!(b, MergeBlocker::MissingCodeOwnerReview { .. }))
    );
}

#[test]
fn codeowners_only_gates_when_an_owned_path_changes() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest::default(),
    )
    .unwrap();
    core.set_codeowners("alice", "jeryu", "docs/ @docsowner\n")
        .unwrap();
    // Touches only an unowned path -> no code-owner requirement.
    let number = open_pr_with(
        &core,
        "abc",
        vec![signed("c1")],
        vec!["src/main.rs".to_string()],
    );
    let e = eval(&core, number);
    assert!(e.mergeable);
    assert!(
        !e.blockers
            .iter()
            .any(|b| matches!(b, MergeBlocker::MissingCodeOwnerReview { .. }))
    );
}

#[test]
fn codeowners_last_matching_rule_wins() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest::default(),
    )
    .unwrap();
    // `docs/secret.md` matches both rules; the later, more specific one wins.
    core.set_codeowners(
        "alice",
        "jeryu",
        "docs/ @docsowner\ndocs/secret.md @security\n",
    )
    .unwrap();
    let number = open_pr_with(
        &core,
        "abc",
        vec![signed("c1")],
        vec!["docs/secret.md".to_string()],
    );
    // @docsowner approval is not enough -- the last matching rule requires @security.
    approve(&core, number, "docsowner");
    let e = eval(&core, number);
    assert!(!e.mergeable);
    assert!(e.blockers.iter().any(|b| matches!(
        b,
        MergeBlocker::MissingCodeOwnerReview { owners, .. }
            if owners == &["security".to_string()]
    )));
    approve(&core, number, "security");
    assert!(eval(&core, number).mergeable);
}
