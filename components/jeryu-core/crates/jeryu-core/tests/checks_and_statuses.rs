//! Commit-status, check-run, check-suite, and workflow-run aggregation tests
//! that mirror the GitHub Statuses and Checks APIs.

use jeryu_core::{
    CheckConclusion, CheckRunStatus, CombinedStatus, CommitStatusState, CreateCheckRunRequest,
    CreateCommitStatusRequest, CreateRepositoryRequest, CreateUserRequest, ForgeCore, ForgeError,
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
            ..Default::default()
        },
    )
    .unwrap();
    core
}

fn status(core: &ForgeCore, sha: &str, context: &str, state: CommitStatusState) {
    core.create_commit_status(
        "alice",
        "jeryu",
        sha,
        "ci-bot",
        CreateCommitStatusRequest {
            state,
            context: context.to_string(),
            description: None,
            target_url: None,
        },
    )
    .unwrap();
}

fn check(core: &ForgeCore, sha: &str, name: &str, conclusion: Option<CheckConclusion>) {
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: name.to_string(),
            head_sha: sha.to_string(),
            status: conclusion.as_ref().map(|_| CheckRunStatus::Completed),
            conclusion,
            details_url: None,
            output: None,
        },
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Commit statuses + combined status rollup
// ---------------------------------------------------------------------------

#[test]
fn commit_status_records_fields() {
    let core = core_with_repo();
    let s = core
        .create_commit_status(
            "alice",
            "jeryu",
            "deadbeef",
            "ci-bot",
            CreateCommitStatusRequest {
                state: CommitStatusState::Success,
                context: "continuous-integration/jeryu".to_string(),
                description: Some("All good".to_string()),
                target_url: Some("https://ci.example.test/1".to_string()),
            },
        )
        .unwrap();
    assert_eq!(s.sha, "deadbeef");
    assert_eq!(s.state, CommitStatusState::Success);
    assert_eq!(s.context, "continuous-integration/jeryu");
    assert_eq!(s.creator, "ci-bot");
}

#[test]
fn empty_sha_for_status_is_rejected() {
    let core = core_with_repo();
    let err = core
        .create_commit_status(
            "alice",
            "jeryu",
            "  ",
            "ci-bot",
            CreateCommitStatusRequest {
                state: CommitStatusState::Success,
                context: "ci".to_string(),
                description: None,
                target_url: None,
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::Validation(_)));
}

#[test]
fn combined_status_with_no_statuses_is_pending() {
    let core = core_with_repo();
    let combined: CombinedStatus = core.combined_status("alice", "jeryu", "nostatus").unwrap();
    assert_eq!(combined.state, CommitStatusState::Pending);
    assert_eq!(combined.total_count, 0);
    assert!(combined.statuses.is_empty());
}

#[test]
fn combined_status_all_success_is_success() {
    let core = core_with_repo();
    status(&core, "abc", "ci/a", CommitStatusState::Success);
    status(&core, "abc", "ci/b", CommitStatusState::Success);
    let combined = core.combined_status("alice", "jeryu", "abc").unwrap();
    assert_eq!(combined.state, CommitStatusState::Success);
    assert_eq!(combined.total_count, 2);
}

#[test]
fn combined_status_any_pending_is_pending() {
    let core = core_with_repo();
    status(&core, "abc", "ci/a", CommitStatusState::Success);
    status(&core, "abc", "ci/b", CommitStatusState::Pending);
    let combined = core.combined_status("alice", "jeryu", "abc").unwrap();
    assert_eq!(combined.state, CommitStatusState::Pending);
}

#[test]
fn combined_status_any_failure_is_failure() {
    let core = core_with_repo();
    status(&core, "abc", "ci/a", CommitStatusState::Success);
    status(&core, "abc", "ci/b", CommitStatusState::Failure);
    let combined = core.combined_status("alice", "jeryu", "abc").unwrap();
    assert_eq!(combined.state, CommitStatusState::Failure);
}

#[test]
fn combined_status_error_dominates_failure() {
    let core = core_with_repo();
    status(&core, "abc", "ci/a", CommitStatusState::Failure);
    status(&core, "abc", "ci/b", CommitStatusState::Error);
    let combined = core.combined_status("alice", "jeryu", "abc").unwrap();
    // GitHub rollup precedence: error > failure > pending > success.
    assert_eq!(combined.state, CommitStatusState::Error);
}

// ---------------------------------------------------------------------------
// Check runs: status inference + listing + filtering
// ---------------------------------------------------------------------------

#[test]
fn check_run_with_conclusion_is_marked_completed() {
    let core = core_with_repo();
    let run = core
        .create_check_run(
            "alice",
            "jeryu",
            CreateCheckRunRequest {
                name: "build".to_string(),
                head_sha: "abc".to_string(),
                status: None,
                conclusion: Some(CheckConclusion::Success),
                details_url: None,
                output: None,
            },
        )
        .unwrap();
    // A conclusion implies completion even without an explicit status.
    assert_eq!(run.status, CheckRunStatus::Completed);
    assert!(run.completed_at.is_some());
    assert_eq!(run.conclusion, Some(CheckConclusion::Success));
}

#[test]
fn check_run_without_status_or_conclusion_is_queued() {
    let core = core_with_repo();
    let run = core
        .create_check_run(
            "alice",
            "jeryu",
            CreateCheckRunRequest {
                name: "lint".to_string(),
                head_sha: "abc".to_string(),
                status: None,
                conclusion: None,
                details_url: None,
                output: None,
            },
        )
        .unwrap();
    assert_eq!(run.status, CheckRunStatus::Queued);
    assert!(run.completed_at.is_none());
    assert!(run.conclusion.is_none());
}

#[test]
fn empty_check_run_name_is_rejected() {
    let core = core_with_repo();
    let err = core
        .create_check_run(
            "alice",
            "jeryu",
            CreateCheckRunRequest {
                name: " ".to_string(),
                head_sha: "abc".to_string(),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::Validation(_)));
}

#[test]
fn list_check_runs_filters_by_head_sha() {
    let core = core_with_repo();
    check(&core, "sha-1", "a", Some(CheckConclusion::Success));
    check(&core, "sha-1", "b", Some(CheckConclusion::Success));
    check(&core, "sha-2", "c", Some(CheckConclusion::Failure));

    let all = core.list_check_runs("alice", "jeryu", None).unwrap();
    assert_eq!(all.total_count, 3);
    assert_eq!(all.check_runs.len(), 3);

    let for_sha1 = core
        .list_check_runs("alice", "jeryu", Some("sha-1"))
        .unwrap();
    assert_eq!(for_sha1.total_count, 2);
    assert!(for_sha1.check_runs.iter().all(|r| r.head_sha == "sha-1"));
}

// ---------------------------------------------------------------------------
// Check suites: per-sha grouping + conclusion rollup
// ---------------------------------------------------------------------------

#[test]
fn check_suite_all_success_concludes_success() {
    let core = core_with_repo();
    check(&core, "abc", "a", Some(CheckConclusion::Success));
    check(&core, "abc", "b", Some(CheckConclusion::Success));

    let suites = core.check_suites("alice", "jeryu", Some("abc")).unwrap();
    assert_eq!(suites.len(), 1);
    assert_eq!(suites[0].head_sha, "abc");
    assert_eq!(suites[0].status, CheckRunStatus::Completed);
    assert_eq!(suites[0].conclusion, Some(CheckConclusion::Success));
    assert_eq!(suites[0].check_runs.len(), 2);
}

#[test]
fn check_suite_with_one_failure_concludes_failure() {
    let core = core_with_repo();
    check(&core, "abc", "a", Some(CheckConclusion::Success));
    check(&core, "abc", "b", Some(CheckConclusion::Failure));

    let suites = core.check_suites("alice", "jeryu", Some("abc")).unwrap();
    assert_eq!(suites[0].conclusion, Some(CheckConclusion::Failure));
}

#[test]
fn check_suite_with_in_progress_run_has_no_conclusion() {
    let core = core_with_repo();
    check(&core, "abc", "done", Some(CheckConclusion::Success));
    // An in-progress run (explicit status, no conclusion) keeps the suite open.
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "running".to_string(),
            head_sha: "abc".to_string(),
            status: Some(CheckRunStatus::InProgress),
            conclusion: None,
            details_url: None,
            output: None,
        },
    )
    .unwrap();

    let suites = core.check_suites("alice", "jeryu", Some("abc")).unwrap();
    assert_eq!(suites[0].status, CheckRunStatus::InProgress);
    assert!(suites[0].conclusion.is_none());
}

#[test]
fn check_suites_group_per_distinct_sha() {
    let core = core_with_repo();
    check(&core, "sha-1", "a", Some(CheckConclusion::Success));
    check(&core, "sha-2", "b", Some(CheckConclusion::Success));
    let suites = core.check_suites("alice", "jeryu", None).unwrap();
    assert_eq!(suites.len(), 2);
}

// ---------------------------------------------------------------------------
// Workflow runs derived from check suites
// ---------------------------------------------------------------------------

#[test]
fn workflow_runs_mirror_check_suites() {
    let core = core_with_repo();
    check(&core, "abc", "a", Some(CheckConclusion::Success));
    check(&core, "def", "b", Some(CheckConclusion::Failure));

    let runs = core.workflow_runs("alice", "jeryu").unwrap();
    assert_eq!(runs.total_count, 2);
    assert_eq!(runs.workflow_runs.len(), 2);
    // Each workflow run carries the per-sha status and conclusion.
    let abc = runs
        .workflow_runs
        .iter()
        .find(|r| r.head_sha == "abc")
        .unwrap();
    assert_eq!(abc.conclusion, Some(CheckConclusion::Success));
    let def = runs
        .workflow_runs
        .iter()
        .find(|r| r.head_sha == "def")
        .unwrap();
    assert_eq!(def.conclusion, Some(CheckConclusion::Failure));
}

#[test]
fn check_runs_on_missing_repo_is_not_found() {
    let core = ForgeCore::new();
    let err = core.list_check_runs("nobody", "void", None).unwrap_err();
    assert!(matches!(err, ForgeError::NotFound(_)));
}

#[test]
fn app_installations_is_empty_by_default() {
    let core = core_with_repo();
    let list = core.app_installations();
    assert_eq!(list.total_count, 0);
    assert!(list.installations.is_empty());
}
