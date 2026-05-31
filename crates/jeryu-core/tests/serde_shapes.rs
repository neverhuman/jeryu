//! Wire-format tests: the serialized JSON must match GitHub's vocabulary
//! (snake_case PR states, UPPERCASE review states, lowercase status states),
//! and request DTOs must accept GitHub-style field names and aliases.

use jeryu_core::{
    CheckConclusion, CheckRunStatus, CommitStatusState, CreatePullRequestRequest,
    CreateReviewRequest, IssueState, PullRequestState, ReviewState, UpdateIssueRequest,
};

fn to_json_str<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}

// ---------------------------------------------------------------------------
// PullRequestState: snake_case (GitHub uses these as derived/internal verbs)
// ---------------------------------------------------------------------------

#[test]
fn pull_request_state_is_snake_case() {
    assert_eq!(
        to_json_str(&PullRequestState::ReadyForReview),
        "\"ready_for_review\""
    );
    assert_eq!(
        to_json_str(&PullRequestState::BlockedByChecks),
        "\"blocked_by_checks\""
    );
    assert_eq!(to_json_str(&PullRequestState::Merged), "\"merged\"");
    assert_eq!(to_json_str(&PullRequestState::Open), "\"open\"");
    assert_eq!(to_json_str(&PullRequestState::Draft), "\"draft\"");
    assert_eq!(
        to_json_str(&PullRequestState::SpeculativeMergeTesting),
        "\"speculative_merge_testing\""
    );
}

#[test]
fn pull_request_state_default_is_open() {
    assert_eq!(PullRequestState::default(), PullRequestState::Open);
}

#[test]
fn pull_request_state_roundtrips() {
    for state in [
        PullRequestState::Draft,
        PullRequestState::Open,
        PullRequestState::ReadyForReview,
        PullRequestState::Approved,
        PullRequestState::Queued,
        PullRequestState::Mergeable,
        PullRequestState::Merged,
        PullRequestState::Closed,
    ] {
        let json = to_json_str(&state);
        let back: PullRequestState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
    }
}

// ---------------------------------------------------------------------------
// ReviewState: UPPERCASE wire form with GitHub event aliases
// ---------------------------------------------------------------------------

#[test]
fn review_state_serializes_uppercase() {
    assert_eq!(to_json_str(&ReviewState::Approved), "\"APPROVED\"");
    assert_eq!(
        to_json_str(&ReviewState::ChangesRequested),
        "\"CHANGES_REQUESTED\""
    );
    assert_eq!(to_json_str(&ReviewState::Commented), "\"COMMENTED\"");
    assert_eq!(to_json_str(&ReviewState::Dismissed), "\"DISMISSED\"");
}

#[test]
fn review_state_accepts_event_verb_aliases() {
    // GitHub's review-submission `event` verbs map onto stored states.
    let approve: ReviewState = serde_json::from_str("\"APPROVE\"").unwrap();
    assert_eq!(approve, ReviewState::Approved);
    let req: ReviewState = serde_json::from_str("\"REQUEST_CHANGES\"").unwrap();
    assert_eq!(req, ReviewState::ChangesRequested);
    let comment: ReviewState = serde_json::from_str("\"COMMENT\"").unwrap();
    assert_eq!(comment, ReviewState::Commented);
}

#[test]
fn create_review_request_parses_event_field() {
    let req: CreateReviewRequest =
        serde_json::from_str(r#"{"event":"APPROVE","body":"wire-format fixture"}"#).unwrap();
    assert_eq!(req.event, ReviewState::Approved);
    assert_eq!(req.body.as_deref(), Some("wire-format fixture"));
    assert!(req.comments.is_empty());
}

// ---------------------------------------------------------------------------
// Commit-status + check enums: lowercase / snake_case
// ---------------------------------------------------------------------------

#[test]
fn commit_status_state_is_lowercase() {
    assert_eq!(to_json_str(&CommitStatusState::Success), "\"success\"");
    assert_eq!(to_json_str(&CommitStatusState::Pending), "\"pending\"");
    assert_eq!(to_json_str(&CommitStatusState::Failure), "\"failure\"");
    assert_eq!(to_json_str(&CommitStatusState::Error), "\"error\"");
}

#[test]
fn check_run_status_is_snake_case() {
    assert_eq!(to_json_str(&CheckRunStatus::Queued), "\"queued\"");
    assert_eq!(to_json_str(&CheckRunStatus::InProgress), "\"in_progress\"");
    assert_eq!(to_json_str(&CheckRunStatus::Completed), "\"completed\"");
}

#[test]
fn check_conclusion_is_snake_case() {
    assert_eq!(to_json_str(&CheckConclusion::Success), "\"success\"");
    assert_eq!(
        to_json_str(&CheckConclusion::ActionRequired),
        "\"action_required\""
    );
    assert_eq!(to_json_str(&CheckConclusion::TimedOut), "\"timed_out\"");
    assert_eq!(to_json_str(&CheckConclusion::Neutral), "\"neutral\"");
    // GitHub-only conclusion: the Rust variant is named `Superseded`, but its
    // wire value must remain GitHub's documented `"stale"` string. Assert both
    // serialize and deserialize directions so the serde rename never drifts.
    assert_eq!(to_json_str(&CheckConclusion::Superseded), "\"stale\"");
    assert_eq!(
        serde_json::from_str::<CheckConclusion>("\"stale\"").unwrap(),
        CheckConclusion::Superseded
    );
}

#[test]
fn issue_state_is_lowercase_and_defaults_open() {
    assert_eq!(to_json_str(&IssueState::Open), "\"open\"");
    assert_eq!(to_json_str(&IssueState::Closed), "\"closed\"");
    assert_eq!(IssueState::default(), IssueState::Open);
}

// ---------------------------------------------------------------------------
// Request DTO parsing tolerance (serde defaults)
// ---------------------------------------------------------------------------

#[test]
fn create_pull_request_request_minimal_json() {
    let req: CreatePullRequestRequest =
        serde_json::from_str(r#"{"title":"t","head":"feature","base":"main"}"#).unwrap();
    assert_eq!(req.title, "t");
    assert_eq!(req.head, "feature");
    assert_eq!(req.base, "main");
    assert!(!req.draft);
    assert!(req.body.is_none());
    assert!(req.head_sha.is_none());
}

#[test]
fn update_issue_request_all_fields_optional() {
    let req: UpdateIssueRequest = serde_json::from_str("{}").unwrap();
    assert!(req.title.is_none());
    assert!(req.state.is_none());
    assert!(req.labels.is_none());
    assert!(req.assignees.is_none());
}
