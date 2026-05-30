//! PullRequest lifecycle + state-machine behavioral tests.
//!
//! The forge recomputes a PR's `state`/`mergeable` on every read from the
//! current set of reviews, statuses, checks, and branch protection. These tests
//! drive the machine through draft -> open -> ready-for-review ->
//! blocked/approved -> mergeable -> merged/closed and assert that illegal
//! operations (e.g. merging a closed PR, merging while blocked) are rejected.

use jeryu_core::{
    CheckConclusion, CheckRunStatus, CommitStatusState, CreateCheckRunRequest,
    CreateCommitStatusRequest, CreatePullRequestRequest, CreateRepositoryRequest,
    CreateReviewRequest, CreateUserRequest, ForgeCore, ForgeError, MergePullRequestRequest,
    PullRequestState, ReviewState, SetBranchProtectionRequest, UpdatePullRequestRequest,
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

fn open_pr(core: &ForgeCore, head_sha: &str, draft: bool) -> u64 {
    core.create_pull_request(
        "alice",
        "jeryu",
        "alice",
        CreatePullRequestRequest {
            title: "change".to_string(),
            body: Some("desc".to_string()),
            head: "feature".to_string(),
            base: "main".to_string(),
            head_sha: Some(head_sha.to_string()),
            base_sha: None,
            draft,
            ..Default::default()
        },
    )
    .unwrap()
    .number
}

fn protect_main(core: &ForgeCore, reviews: u64, checks: &[&str]) {
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            required_status_checks: checks.iter().map(|c| c.to_string()).collect(),
            required_approving_review_count: reviews,
            ..Default::default()
        },
    )
    .unwrap();
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

fn pass_status(core: &ForgeCore, sha: &str, context: &str) {
    core.create_commit_status(
        "alice",
        "jeryu",
        sha,
        "ci-bot",
        CreateCommitStatusRequest {
            state: CommitStatusState::Success,
            context: context.to_string(),
            description: None,
            target_url: None,
        },
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Shape / field-name assertions (GitHub semantics, not legacy)
// ---------------------------------------------------------------------------

#[test]
fn pull_request_uses_github_field_names() {
    let core = core_with_repo();
    let number = open_pr(&core, "abc", false);
    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();

    // GitHub model: `number`, paired `issue_number`, head/base refs with `sha`.
    assert_eq!(pr.number, 1);
    assert_eq!(pr.issue_number, 1);
    assert_eq!(pr.head.ref_name, "feature");
    assert_eq!(pr.head.sha, "abc");
    assert_eq!(pr.base.ref_name, "main");
    assert!(!pr.merged);
    assert!(pr.merged_at.is_none());
    assert!(pr.merge_commit_sha.is_none());

    // Serialized JSON must carry GitHub-shaped key names: number, head, base,
    // and merge_commit_sha.
    let json = serde_json::to_value(&pr).unwrap();
    assert!(json.get("number").is_some());
    assert!(json.get("issue_number").is_some());
    assert!(json.get("merge_commit_sha").is_some());
    // The branch ref serializes its name under the GitHub key "ref".
    assert_eq!(json["head"]["ref"], "feature");
}

#[test]
fn pr_and_backing_issue_share_numbering_sequence() {
    let core = core_with_repo();
    // An issue first, then a PR. Both draw from the same per-repo counter so the
    // PR's backing issue gets number 2 while the PR itself is pull #1.
    core.create_pull_request(
        "alice",
        "jeryu",
        "alice",
        CreatePullRequestRequest {
            title: "first pr".to_string(),
            head: "f1".to_string(),
            base: "main".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    let pr = core.get_pull_request("alice", "jeryu", 1).unwrap();
    assert_eq!(pr.number, 1);
    // The backing issue exists and is flagged as a pull request.
    let issue = core.get_issue("alice", "jeryu", pr.issue_number).unwrap();
    assert!(issue.pull_request.is_some());
    let marker = issue.pull_request.unwrap();
    assert!(marker.url.contains("/pulls/1"));
    assert!(marker.html_url.contains("/pull/1"));
}

// ---------------------------------------------------------------------------
// Draft state
// ---------------------------------------------------------------------------

#[test]
fn draft_pr_starts_in_draft_and_is_not_mergeable() {
    let core = core_with_repo();
    let number = open_pr(&core, "abc", true);
    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert_eq!(pr.state, PullRequestState::Draft);
    assert!(pr.draft);
    assert!(!pr.mergeable);
}

#[test]
fn draft_pr_cannot_be_merged_even_without_protection() {
    let core = core_with_repo();
    let number = open_pr(&core, "abc", true);
    let err = core
        .merge_pull_request("alice", "jeryu", number, MergePullRequestRequest::default())
        .unwrap_err();
    assert!(matches!(err, ForgeError::BranchProtection(_)));
}

#[test]
fn marking_ready_for_review_clears_draft() {
    let core = core_with_repo();
    let number = open_pr(&core, "abc", true);
    // draft -> ready_for_review: toggling draft off.
    let updated = core
        .update_pull_request(
            "alice",
            "jeryu",
            number,
            UpdatePullRequestRequest {
                draft: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!updated.draft);
    // With no protection rule, an undrafted PR is immediately mergeable.
    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert_eq!(pr.state, PullRequestState::Mergeable);
    assert!(pr.mergeable);
}

#[test]
fn converting_open_pr_back_to_draft_blocks_merge_again() {
    let core = core_with_repo();
    let number = open_pr(&core, "abc", false);
    // Now flip it to draft.
    core.update_pull_request(
        "alice",
        "jeryu",
        number,
        UpdatePullRequestRequest {
            draft: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert_eq!(pr.state, PullRequestState::Draft);
    assert!(!pr.mergeable);
}

// ---------------------------------------------------------------------------
// open -> blocked -> approved/mergeable transitions under protection
// ---------------------------------------------------------------------------

#[test]
fn open_pr_with_no_protection_is_immediately_mergeable() {
    let core = core_with_repo();
    let number = open_pr(&core, "abc", false);
    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert_eq!(pr.state, PullRequestState::Mergeable);
    assert!(pr.mergeable);
}

#[test]
fn protection_blocks_then_review_plus_status_unblocks() {
    let core = core_with_repo();
    protect_main(&core, 1, &["ci/fast"]);
    let number = open_pr(&core, "abc", false);

    // Initially blocked: missing review and missing status.
    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert_eq!(pr.state, PullRequestState::BlockedByChecks);
    assert!(!pr.mergeable);

    // Add the approving review -- still blocked on the status check.
    approve(&core, number, "alice");
    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert!(!pr.mergeable);

    // Now satisfy the required status -> becomes mergeable.
    pass_status(&core, "abc", "ci/fast");
    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert_eq!(pr.state, PullRequestState::Mergeable);
    assert!(pr.mergeable);
}

#[test]
fn changes_requested_review_does_not_count_as_approval() {
    let core = core_with_repo();
    protect_main(&core, 1, &[]);
    let number = open_pr(&core, "abc", false);

    core.create_review(
        "alice",
        "jeryu",
        number,
        "reviewer",
        CreateReviewRequest {
            body: Some("please fix".to_string()),
            event: ReviewState::ChangesRequested,
            comments: vec![],
        },
    )
    .unwrap();

    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert!(
        !pr.mergeable,
        "changes-requested must not satisfy review gate"
    );
}

// ---------------------------------------------------------------------------
// Merge transition (-> merged) and illegal transitions
// ---------------------------------------------------------------------------

#[test]
fn full_lifecycle_to_merged_closes_backing_issue() {
    let core = core_with_repo();
    protect_main(&core, 1, &["ci/fast"]);
    let number = open_pr(&core, "abc", false);
    approve(&core, number, "alice");
    pass_status(&core, "abc", "ci/fast");

    let result = core
        .merge_pull_request("alice", "jeryu", number, MergePullRequestRequest::default())
        .unwrap();
    assert!(result.merged);
    assert!(result.sha.starts_with("merge-"));

    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert_eq!(pr.state, PullRequestState::Merged);
    assert!(pr.merged);
    assert!(pr.merged_at.is_some());
    assert!(pr.merge_commit_sha.is_some());
    // Merge state is sticky -- re-reading keeps it merged, not "mergeable".
    assert_eq!(pr.mergeable_state, "merged");
    assert!(!pr.mergeable);

    // Backing issue auto-closed on merge.
    let issue = core.get_issue("alice", "jeryu", pr.issue_number).unwrap();
    assert_eq!(issue.state, jeryu_core::IssueState::Closed);
    assert!(issue.closed_at.is_some());
}

#[test]
fn merging_an_already_merged_pr_is_idempotent() {
    let core = core_with_repo();
    let number = open_pr(&core, "abc", false);
    let first = core
        .merge_pull_request("alice", "jeryu", number, MergePullRequestRequest::default())
        .unwrap();
    let second = core
        .merge_pull_request("alice", "jeryu", number, MergePullRequestRequest::default())
        .unwrap();
    assert!(second.merged);
    // Returns the original merge sha rather than minting a new one.
    assert_eq!(first.sha, second.sha);
    assert!(second.message.to_lowercase().contains("already"));
}

#[test]
fn merging_blocked_pr_is_rejected_with_branch_protection_error() {
    let core = core_with_repo();
    protect_main(&core, 1, &["ci/fast"]);
    let number = open_pr(&core, "abc", false);
    // No review, no status -> illegal to merge.
    let err = core
        .merge_pull_request("alice", "jeryu", number, MergePullRequestRequest::default())
        .unwrap_err();
    assert!(matches!(err, ForgeError::BranchProtection(_)));
    // PR must remain unmerged after a rejected merge.
    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert!(!pr.merged);
}

#[test]
fn closed_pr_cannot_be_merged() {
    let core = core_with_repo();
    // Closed is now a sticky terminal state regardless of mergeability (see
    // `closing_a_mergeable_pr_sticks`), so we can close a plain open PR and the
    // merge guard fires on the preserved Closed state.
    let number = open_pr(&core, "abc", false);
    core.update_pull_request(
        "alice",
        "jeryu",
        number,
        UpdatePullRequestRequest {
            state: Some(PullRequestState::Closed),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        core.get_pull_request("alice", "jeryu", number)
            .unwrap()
            .state,
        PullRequestState::Closed,
        "a closed PR keeps its terminal Closed state"
    );

    let err = core
        .merge_pull_request("alice", "jeryu", number, MergePullRequestRequest::default())
        .unwrap_err();
    // Closed PRs are an illegal merge target -> Validation error, not protection.
    assert!(matches!(err, ForgeError::Validation(_)));
}

#[test]
fn closing_a_mergeable_pr_sticks() {
    // DELIBERATE CORRECTNESS CHANGE: previously `apply_evaluation` recomputed the
    // PR state on every read and reverted an undrafted, otherwise-mergeable PR
    // from a just-set `Closed` back to `Mergeable` (the old, buggy behavior pinned
    // by `closing_a_mergeable_pr_does_not_stick`). GitHub treats `Closed` (like
    // `Merged`) as a sticky terminal state that is never recomputed away until the
    // PR is explicitly reopened. This test pins the corrected behavior.
    let core = core_with_repo();
    let number = open_pr(&core, "abc", false);
    core.update_pull_request(
        "alice",
        "jeryu",
        number,
        UpdatePullRequestRequest {
            state: Some(PullRequestState::Closed),
            ..Default::default()
        },
    )
    .unwrap();
    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert_eq!(pr.state, PullRequestState::Closed);
    assert_eq!(pr.mergeable_state, "closed");
    assert!(!pr.mergeable);
    assert!(!pr.merged);

    // And it stays Closed across repeated reads (true stickiness, not a one-shot).
    let pr_again = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert_eq!(pr_again.state, PullRequestState::Closed);
}

#[test]
fn merge_with_mismatched_sha_is_rejected() {
    let core = core_with_repo();
    let number = open_pr(&core, "realhead", false);
    let err = core
        .merge_pull_request(
            "alice",
            "jeryu",
            number,
            MergePullRequestRequest {
                sha: Some("stalehead".to_string()),
                ..Default::default()
            },
        )
        .unwrap_err();
    // SHA mismatch is a merge blocker (optimistic-concurrency guard).
    assert!(matches!(err, ForgeError::BranchProtection(_)));
}

#[test]
fn merge_with_matching_sha_succeeds() {
    let core = core_with_repo();
    let number = open_pr(&core, "realhead", false);
    let result = core
        .merge_pull_request(
            "alice",
            "jeryu",
            number,
            MergePullRequestRequest {
                sha: Some("realhead".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(result.merged);
}

#[test]
fn merge_unknown_pr_is_not_found() {
    let core = core_with_repo();
    let err = core
        .merge_pull_request("alice", "jeryu", 777, MergePullRequestRequest::default())
        .unwrap_err();
    assert!(matches!(err, ForgeError::NotFound(_)));
}

// ---------------------------------------------------------------------------
// Check runs as merge gate (GitHub Checks API path)
// ---------------------------------------------------------------------------

#[test]
fn required_check_run_success_satisfies_protection() {
    let core = core_with_repo();
    protect_main(&core, 0, &["ci/build"]);
    let number = open_pr(&core, "abc", false);
    assert!(
        !core
            .get_pull_request("alice", "jeryu", number)
            .unwrap()
            .mergeable
    );

    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "ci/build".to_string(),
            head_sha: "abc".to_string(),
            status: Some(CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Success),
            details_url: None,
            output: None,
        },
    )
    .unwrap();

    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert!(pr.mergeable);
}

#[test]
fn failed_required_check_run_keeps_pr_blocked() {
    let core = core_with_repo();
    protect_main(&core, 0, &["ci/build"]);
    let number = open_pr(&core, "abc", false);

    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "ci/build".to_string(),
            head_sha: "abc".to_string(),
            status: Some(CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Failure),
            details_url: None,
            output: None,
        },
    )
    .unwrap();

    let pr = core.get_pull_request("alice", "jeryu", number).unwrap();
    assert!(!pr.mergeable);
}

// ---------------------------------------------------------------------------
// Reviews + review comments association
// ---------------------------------------------------------------------------

#[test]
fn review_with_inline_comments_is_recorded_and_associated() {
    let core = core_with_repo();
    let number = open_pr(&core, "abc", false);

    let review = core
        .create_review(
            "alice",
            "jeryu",
            number,
            "reviewer",
            CreateReviewRequest {
                body: Some(
                    "Approved with receipt jeryu-review:sha256:0123456789abcdef".to_string(),
                ),
                event: ReviewState::Approved,
                comments: vec![
                    jeryu_core::ReviewCommentInput {
                        path: "src/lib.rs".to_string(),
                        line: Some(10),
                        body: "nit: rename".to_string(),
                    },
                    jeryu_core::ReviewCommentInput {
                        path: "README.md".to_string(),
                        line: None,
                        body: "thanks".to_string(),
                    },
                ],
            },
        )
        .unwrap();
    assert_eq!(review.state, ReviewState::Approved);
    assert_eq!(review.pull_number, number);

    let reviews = core.list_reviews("alice", "jeryu", number).unwrap();
    assert_eq!(reviews.len(), 1);

    let comments = core.list_review_comments("alice", "jeryu", number).unwrap();
    assert_eq!(comments.len(), 2);
    // All review comments link back to the parent review id.
    assert!(comments.iter().all(|c| c.review_id == review.id));
    assert!(comments.iter().all(|c| c.pull_number == number));
    assert_eq!(comments[0].path, "src/lib.rs");
    assert_eq!(comments[0].line, Some(10));
    assert_eq!(comments[1].line, None);
}

#[test]
fn review_on_unknown_pr_is_not_found() {
    let core = core_with_repo();
    let err = core
        .create_review(
            "alice",
            "jeryu",
            123,
            "alice",
            CreateReviewRequest {
                body: None,
                event: ReviewState::Approved,
                comments: vec![],
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::NotFound(_)));
}

#[test]
fn multiple_approvals_count_toward_threshold() {
    let core = core_with_repo();
    protect_main(&core, 2, &[]);
    let number = open_pr(&core, "abc", false);

    approve(&core, number, "alice");
    assert!(
        !core
            .get_pull_request("alice", "jeryu", number)
            .unwrap()
            .mergeable,
        "one approval should not satisfy a two-review requirement"
    );

    approve(&core, number, "bob");
    assert!(
        core.get_pull_request("alice", "jeryu", number)
            .unwrap()
            .mergeable,
        "two approvals satisfy the gate"
    );
}

#[test]
fn list_pull_requests_filters_by_state() {
    let core = core_with_repo();
    // PR #1 stays open/mergeable, PR #2 gets merged.
    open_pr(&core, "h1", false);
    let n2 = open_pr(&core, "h2", false);
    core.merge_pull_request("alice", "jeryu", n2, MergePullRequestRequest::default())
        .unwrap();

    let merged = core
        .list_pull_requests("alice", "jeryu", Some(PullRequestState::Merged))
        .unwrap();
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].number, n2);

    let all = core.list_pull_requests("alice", "jeryu", None).unwrap();
    assert_eq!(all.len(), 2);
    // Sorted by number ascending.
    assert_eq!(all[0].number, 1);
    assert_eq!(all[1].number, 2);
}
