//! Review verdicts, targeted dismissal and their persistence boundary.

#[path = "support/policy.rs"]
mod policy;
mod support;

use support::private_directory;

use jeryu_core::{
    CreatePullRequestRequest, CreateRepositoryRequest, CreateReviewRequest, CreateWebhookRequest,
    DismissReviewRequest, ForgeCore, ForgeError, MergeBlocker, PrepareRepositoryTransfer, Review,
    ReviewCommentInput, ReviewState, SetBranchProtectionRequest, UserRole, WebhookConfig,
    effective_reviews_for_head, effective_reviews_for_pull_request,
};
use rusqlite::Connection;
use serde_json::{Value, json};

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const NEXT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn seed(core: &ForgeCore) -> u64 {
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "demo".to_string(),
            default_branch: Some("main".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    core.set_branch_protection(
        "alice",
        "demo",
        "main",
        SetBranchProtectionRequest {
            required_approving_review_count: 1,
            ..Default::default()
        },
    )
    .unwrap();
    core.create_webhook(
        "alice",
        "demo",
        CreateWebhookRequest {
            name: "review-events".to_string(),
            active: true,
            events: vec!["pull_request_review".to_string()],
            config: WebhookConfig {
                url: "https://hooks.invalid/reviews".to_string(),
                content_type: "json".to_string(),
                secret: None,
            },
        },
    )
    .unwrap();
    open_pr(core, "feature", "alice")
}

fn open_pr(core: &ForgeCore, branch: &str, author: &str) -> u64 {
    core.create_pull_request(
        "alice",
        "demo",
        author,
        CreatePullRequestRequest {
            title: "reviewed change".to_string(),
            head: branch.to_string(),
            base: "main".to_string(),
            head_sha: Some(HEAD.to_string()),
            changed_files: vec!["src/lib.rs".to_string()],
            ..Default::default()
        },
    )
    .unwrap()
    .number
}

fn request(event: ReviewState) -> CreateReviewRequest {
    CreateReviewRequest {
        body: Some("review evidence".to_string()),
        event,
        comments: vec![ReviewCommentInput {
            path: "src/lib.rs".to_string(),
            line: Some(1),
            body: "inline evidence".to_string(),
        }],
        expected_head_sha: Some(HEAD.to_string()),
    }
}

fn submit(core: &ForgeCore, number: u64, actor: &str, event: ReviewState) -> Review {
    core.create_review("alice", "demo", number, actor, request(event))
        .unwrap()
}

fn dismissal(target: &Review) -> DismissReviewRequest {
    DismissReviewRequest {
        review_id: target.id,
        expected_head_sha: HEAD.to_string(),
        reason: "withdraw my current verdict".to_string(),
    }
}

fn snapshot(core: &ForgeCore, number: u64) -> Value {
    json!({
        "pull": core.get_pull_request("alice", "demo", number).unwrap(),
        "reviews": core.list_reviews("alice", "demo", number).unwrap(),
        "comments": core.list_review_comments("alice", "demo", number).unwrap(),
        "deliveries": core.list_webhook_deliveries("alice", "demo").unwrap(),
    })
}

#[test]
fn author_self_approval_is_denied_inside_core_without_side_effects() {
    let core = ForgeCore::new();
    let number = seed(&core);
    core.create_account("alice", "correct horse battery", UserRole::Admin)
        .unwrap();
    for actor in ["alice", "ALICE"] {
        let before = snapshot(&core, number);
        let error = core
            .create_review(
                "alice",
                "demo",
                number,
                actor,
                request(ReviewState::Approved),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            ForgeError::Forbidden(_) | ForgeError::Validation(_)
        ));
        assert_eq!(snapshot(&core, number), before);
    }
    assert!(core.get_user("ALICE").is_err());
    let legacy_author = open_pr(&core, "legacy-author", "ALICE");
    assert!(matches!(
        core.create_review(
            "alice",
            "demo",
            legacy_author,
            "alice",
            request(ReviewState::Approved)
        ),
        Err(ForgeError::Forbidden(_))
    ));
    let comment = submit(&core, number, "alice", ReviewState::Commented);
    let reviews = core.list_reviews("alice", "demo", number).unwrap();
    assert_eq!(reviews, vec![comment]);
    assert!(effective_reviews_for_head(&reviews, HEAD).is_empty());
}

#[test]
fn comments_preserve_each_explicit_verdict_and_its_qualification() {
    for verdict in [ReviewState::Approved, ReviewState::ChangesRequested] {
        let core = ForgeCore::new();
        let number = seed(&core);
        let explicit = submit(&core, number, "reviewer", verdict.clone());
        let before = core
            .evaluate_pull_request("alice", "demo", number, Some(HEAD))
            .unwrap();
        let policy_before =
            policy::evaluate_advisory(&core, "alice", "demo", number, Some(HEAD)).unwrap();
        match verdict {
            ReviewState::Approved => assert!(policy_before.mergeable),
            ReviewState::ChangesRequested => assert!(policy_before.blockers.iter().any(|blocker| {
                matches!(blocker, MergeBlocker::ChangesRequested { reviewers } if reviewers == &["reviewer"])
            })),
            _ => unreachable!("the fixture supplies only explicit verdicts"),
        }
        assert!(!before.mergeable);
        submit(&core, number, "reviewer", ReviewState::Commented);
        let reviews = core.list_reviews("alice", "demo", number).unwrap();
        assert_eq!(effective_reviews_for_head(&reviews, HEAD), vec![&explicit]);
        assert_eq!(
            core.evaluate_pull_request("alice", "demo", number, Some(HEAD))
                .unwrap(),
            before
        );
        assert_eq!(
            policy::evaluate_advisory(&core, "alice", "demo", number, Some(HEAD)).unwrap(),
            policy_before
        );
        let qualification = core.review_qualification("alice", "demo", number).unwrap();
        assert!(qualification.effective_reviews.is_empty());
        assert_eq!(qualification.advisory_reviews, reviews);
        assert_eq!(reviews.len(), 2);
        assert_eq!(
            core.list_review_comments("alice", "demo", number)
                .unwrap()
                .len(),
            2
        );
    }
}

#[test]
fn targeted_dismissal_does_not_resurrect_an_earlier_verdict() {
    for verdicts in [
        [ReviewState::Approved, ReviewState::ChangesRequested],
        [ReviewState::ChangesRequested, ReviewState::Approved],
    ] {
        let core = ForgeCore::new();
        let number = seed(&core);
        let [earlier, current] = verdicts;
        submit(&core, number, "reviewer", earlier);
        let target = submit(&core, number, "reviewer", current);
        submit(&core, number, "reviewer", ReviewState::Commented);
        let history = core.list_reviews("alice", "demo", number).unwrap();
        let removed = core
            .dismiss_review("alice", "demo", number, "reviewer", dismissal(&target))
            .unwrap();
        assert_eq!(removed.state, ReviewState::Dismissed);
        assert_eq!(removed.dismissed_review_id, Some(target.id));
        assert_eq!(removed.head_sha.as_deref(), Some(HEAD));
        assert_eq!(removed.body.as_deref(), Some("withdraw my current verdict"));
        assert_ne!(removed.id, target.id);
        let reviews = core.list_reviews("alice", "demo", number).unwrap();
        assert_eq!(&reviews[..history.len()], history.as_slice());
        assert!(effective_reviews_for_head(&reviews, HEAD).is_empty());
        let evaluation =
            policy::evaluate_advisory(&core, "alice", "demo", number, Some(HEAD)).unwrap();
        assert!(
            evaluation
                .blockers
                .iter()
                .any(|blocker| matches!(blocker, MergeBlocker::MissingReview { approved: 0, .. }))
        );
        assert!(
            !evaluation
                .blockers
                .iter()
                .any(|blocker| matches!(blocker, MergeBlocker::ChangesRequested { .. }))
        );
        let accepted = submit(&core, number, "reviewer", ReviewState::Approved);
        let reviews = core.list_reviews("alice", "demo", number).unwrap();
        assert_eq!(effective_reviews_for_head(&reviews, HEAD), vec![&accepted]);
        assert!(
            policy::evaluate_advisory(&core, "alice", "demo", number, Some(HEAD))
                .unwrap()
                .mergeable
        );
        assert!(
            !core
                .get_pull_request("alice", "demo", number)
                .unwrap()
                .mergeable
        );
        let qualification = core.review_qualification("alice", "demo", number).unwrap();
        assert!(qualification.effective_reviews.is_empty());
        assert_eq!(qualification.advisory_reviews, reviews);
    }
}

#[test]
fn dismissal_preserves_another_reviewers_rejection() {
    let core = ForgeCore::new();
    let number = seed(&core);
    let target = submit(&core, number, "reviewer", ReviewState::ChangesRequested);
    submit(
        &core,
        number,
        "second-reviewer",
        ReviewState::ChangesRequested,
    );
    core.dismiss_review("alice", "demo", number, "reviewer", dismissal(&target))
        .unwrap();
    let evaluation = policy::evaluate_advisory(&core, "alice", "demo", number, Some(HEAD)).unwrap();
    assert!(evaluation.blockers.iter().any(|blocker| matches!(
        blocker, MergeBlocker::ChangesRequested { reviewers } if reviewers == &["second-reviewer"]
    )));
    let qualification = core.review_qualification("alice", "demo", number).unwrap();
    assert!(qualification.effective_reviews.is_empty());
    assert_eq!(
        qualification.advisory_reviews,
        core.list_reviews("alice", "demo", number).unwrap()
    );
    assert!(
        !core
            .get_pull_request("alice", "demo", number)
            .unwrap()
            .mergeable
    );
}

#[test]
fn dismissal_refusals_leave_every_review_side_effect_unchanged() {
    let core = ForgeCore::new();
    let number = seed(&core);
    let target = submit(&core, number, "reviewer", ReviewState::Approved);
    let comment = submit(&core, number, "reviewer", ReviewState::Commented);
    let other_number = open_pr(&core, "another-pr", "alice");
    core.create_account("administrator", "correct horse battery", UserRole::Admin)
        .unwrap();
    let before = snapshot(&core, number);
    let other_before = snapshot(&core, other_number);
    for actor in ["intruder", "administrator"] {
        assert!(matches!(
            core.dismiss_review("alice", "demo", number, actor, dismissal(&target)),
            Err(ForgeError::Forbidden(_))
        ));
        assert_eq!(snapshot(&core, number), before);
    }
    assert!(core.get_user("intruder").is_err());
    for reason in ["", " \n\t"] {
        let mut invalid = dismissal(&target);
        invalid.reason = reason.to_string();
        assert!(matches!(
            core.dismiss_review("alice", "demo", number, "reviewer", invalid),
            Err(ForgeError::Validation(_))
        ));
    }
    for head in [
        "",
        "abc",
        "head-1",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        let mut invalid = dismissal(&target);
        invalid.expected_head_sha = head.to_string();
        assert!(matches!(
            core.dismiss_review("alice", "demo", number, "reviewer", invalid),
            Err(ForgeError::Validation(_))
        ));
    }
    assert!(matches!(
        core.dismiss_review("alice", "demo", number, "reviewer", dismissal(&comment)),
        Err(ForgeError::Validation(_))
    ));
    let mut wrong_head = dismissal(&target);
    wrong_head.expected_head_sha = NEXT.to_string();
    assert!(matches!(
        core.dismiss_review("alice", "demo", number, "reviewer", wrong_head),
        Err(ForgeError::Conflict(_))
    ));
    assert!(matches!(
        core.dismiss_review(
            "alice",
            "demo",
            other_number,
            "reviewer",
            dismissal(&target)
        ),
        Err(ForgeError::NotFound(_))
    ));
    assert!(matches!(
        core.dismiss_review("alice", "missing", number, "reviewer", dismissal(&target)),
        Err(ForgeError::NotFound(_))
    ));
    assert!(matches!(
        core.create_review(
            "alice",
            "demo",
            number,
            "new-profile",
            request(ReviewState::Dismissed)
        ),
        Err(ForgeError::Validation(_))
    ));
    assert!(core.get_user("new-profile").is_err());
    assert_eq!(snapshot(&core, number), before);
    assert_eq!(snapshot(&core, other_number), other_before);
}

#[test]
fn superseded_repeated_and_previous_head_targets_are_refused() {
    let core = ForgeCore::new();
    let number = seed(&core);
    let earlier = submit(&core, number, "reviewer", ReviewState::Approved);
    let target = submit(&core, number, "reviewer", ReviewState::ChangesRequested);
    let before = snapshot(&core, number);
    assert!(matches!(
        core.dismiss_review("alice", "demo", number, "reviewer", dismissal(&earlier)),
        Err(ForgeError::Conflict(_))
    ));
    assert_eq!(snapshot(&core, number), before);
    core.dismiss_review("alice", "demo", number, "reviewer", dismissal(&target))
        .unwrap();
    let before = snapshot(&core, number);
    assert!(matches!(
        core.dismiss_review("alice", "demo", number, "reviewer", dismissal(&target)),
        Err(ForgeError::Conflict(_))
    ));
    assert_eq!(snapshot(&core, number), before);
    let target = submit(&core, number, "reviewer", ReviewState::Approved);
    core.refresh_pull_request_heads_for_ref("alice", "demo", "feature", NEXT)
        .unwrap();
    let mut moved = dismissal(&target);
    moved.expected_head_sha = NEXT.to_string();
    let before = snapshot(&core, number);
    assert!(matches!(
        core.dismiss_review("alice", "demo", number, "reviewer", moved),
        Err(ForgeError::Conflict(_))
    ));
    assert_eq!(snapshot(&core, number), before);
}

#[test]
fn rejected_creation_does_not_create_a_profile() {
    let core = ForgeCore::new();
    let number = seed(&core);
    let mut moved = request(ReviewState::Approved);
    moved.expected_head_sha = Some(NEXT.to_string());
    let before = snapshot(&core, number);
    assert!(matches!(
        core.create_review("alice", "demo", number, "new-reviewer", moved),
        Err(ForgeError::Conflict(_))
    ));
    assert!(core.get_user("new-reviewer").is_err());
    assert_eq!(snapshot(&core, number), before);
    assert!(matches!(
        core.create_review(
            "alice",
            "demo",
            999,
            "absent-reviewer",
            request(ReviewState::Approved)
        ),
        Err(ForgeError::NotFound(_))
    ));
    assert!(core.get_user("absent-reviewer").is_err());
}

#[test]
fn inherited_self_approval_never_satisfies_count_or_codeowners() {
    for historical_actor in ["alice", "ALICE"] {
        let temp = private_directory();
        let db = temp.path().join("forge.sqlite");
        let core = ForgeCore::open_sqlite(&db).unwrap();
        let number = seed(&core);
        core.set_codeowners("alice", "demo", "*.rs @alice").unwrap();
        submit(&core, number, "reviewer", ReviewState::Approved);
        drop(core);
        let conn = Connection::open(&db).unwrap();
        conn.execute("UPDATE reviews SET author = ?1", [historical_actor])
            .unwrap();
        drop(conn);
        let core = ForgeCore::open_sqlite(&db).unwrap();
        let pr = core.get_pull_request("alice", "demo", number).unwrap();
        let reviews = core.list_reviews("alice", "demo", number).unwrap();
        assert_eq!(reviews.len(), 1);
        assert!(effective_reviews_for_pull_request(&reviews, &pr).is_empty());
        let evaluation = core
            .evaluate_pull_request("alice", "demo", number, Some(HEAD))
            .unwrap();
        assert!(
            evaluation
                .blockers
                .iter()
                .any(|blocker| matches!(blocker, MergeBlocker::MissingReview { approved: 0, .. }))
        );
        assert!(
            evaluation
                .blockers
                .iter()
                .any(|blocker| matches!(blocker, MergeBlocker::MissingCodeOwnerReview { .. }))
        );
    }
}

#[test]
fn historical_dismissals_do_not_infer_targets_or_cross_actor_and_head() {
    let core = ForgeCore::new();
    let number = seed(&core);
    for verdict in [ReviewState::Approved, ReviewState::ChangesRequested] {
        let explicit = submit(&core, number, "reviewer", verdict.clone());
        let mut unbound = explicit.clone();
        unbound.id = uuid::Uuid::new_v4();
        unbound.state = ReviewState::Dismissed;
        let history = [explicit.clone(), unbound.clone()];
        let effective = effective_reviews_for_head(&history, HEAD);
        if verdict == ReviewState::Approved {
            assert!(effective.is_empty());
        } else {
            assert_eq!(effective, vec![&history[0]]);
        }
        unbound.dismissed_review_id = Some(uuid::Uuid::new_v4());
        let history = [explicit.clone(), unbound.clone()];
        assert_eq!(
            effective_reviews_for_head(&history, HEAD),
            vec![&history[0]]
        );
        unbound.dismissed_review_id = Some(explicit.id);
        unbound.author = "another-reviewer".to_string();
        let history = [explicit.clone(), unbound.clone()];
        assert_eq!(
            effective_reviews_for_head(&history, HEAD),
            vec![&history[0]]
        );
        unbound.author = explicit.author.clone();
        unbound.head_sha = None;
        let history = [explicit, unbound];
        assert_eq!(
            effective_reviews_for_head(&history, HEAD),
            vec![&history[0]]
        );
    }
}

#[test]
fn inherited_self_approval_cannot_erase_an_earlier_rejection() {
    let core = ForgeCore::new();
    let number = seed(&core);
    let rejection = submit(&core, number, "alice", ReviewState::ChangesRequested);
    let mut invalid_approval = rejection.clone();
    invalid_approval.id = uuid::Uuid::new_v4();
    invalid_approval.state = ReviewState::Approved;
    let history = [rejection, invalid_approval];
    let pr = core.get_pull_request("alice", "demo", number).unwrap();
    assert_eq!(
        effective_reviews_for_pull_request(&history, &pr),
        vec![&history[0]]
    );
}

#[test]
fn review_dismissal_survives_sqlite_reopen_and_unrelated_write() {
    let temp = private_directory();
    let db = temp.path().join("forge.sqlite");
    let core = ForgeCore::open_sqlite(&db).unwrap();
    let number = seed(&core);
    submit(&core, number, "reviewer", ReviewState::Approved);
    let target = submit(&core, number, "reviewer", ReviewState::ChangesRequested);
    core.dismiss_review("alice", "demo", number, "reviewer", dismissal(&target))
        .unwrap();
    let expected = core.list_reviews("alice", "demo", number).unwrap();
    drop(core);
    let core = ForgeCore::open_sqlite(&db).unwrap();
    core.set_repository_readme("alice", "demo", "unrelated write".to_string())
        .unwrap();
    drop(core);
    let core = ForgeCore::open_sqlite(&db).unwrap();
    let actual = core.list_reviews("alice", "demo", number).unwrap();
    assert_eq!(actual, expected);
    assert!(effective_reviews_for_head(&actual, HEAD).is_empty());
    assert!(
        !core
            .get_pull_request("alice", "demo", number)
            .unwrap()
            .mergeable
    );
    let conn = Connection::open(&db).unwrap();
    let persisted: (String, String) = conn
        .query_row(
            "SELECT dismissed_review_id, body FROM reviews WHERE state = 'DISMISSED'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        persisted,
        (
            target.id.to_string(),
            "withdraw my current verdict".to_string()
        )
    );
}

#[test]
fn review_dismissal_target_and_order_survive_repository_transfer() {
    let temp = private_directory();
    let db = temp.path().join("forge.sqlite");
    let core = ForgeCore::open_sqlite(&db).unwrap();
    let number = seed(&core);
    let target = submit(&core, number, "reviewer", ReviewState::Approved);
    core.dismiss_review("alice", "demo", number, "reviewer", dismissal(&target))
        .unwrap();
    let mut expected = core.list_reviews("alice", "demo", number).unwrap();
    let repo = core.get_repository("alice", "demo").unwrap();
    let prepared = core
        .prepare_repository_transfer(PrepareRepositoryTransfer {
            repository_id: repo.id,
            expected_source_owner: "alice".to_string(),
            expected_source_name: "demo".to_string(),
            destination_owner: "destination".to_string(),
            request_fingerprint: "review-custody".to_string(),
            idempotency_key: "review-transfer-1".to_string(),
        })
        .unwrap();
    core.commit_repository_transfer(
        prepared.transaction_id,
        json!({"schema_version":"jeryu.repository-transfer/v1"}),
    )
    .unwrap();
    core.set_repository_readme("destination", "demo", "unrelated write".to_string())
        .unwrap();
    drop(core);
    for review in &mut expected {
        review.owner = "destination".to_string();
    }
    let core = ForgeCore::open_sqlite(&db).unwrap();
    let actual = core.list_reviews("destination", "demo", number).unwrap();
    assert_eq!(actual, expected);
    assert!(effective_reviews_for_head(&actual, HEAD).is_empty());
}

#[test]
fn failed_review_write_rolls_back_profile_and_review() {
    let temp = private_directory();
    let db = temp.path().join("forge.sqlite");
    let core = ForgeCore::open_sqlite(&db).unwrap();
    let number = seed(&core);
    let before = snapshot(&core, number);
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_review_insert BEFORE INSERT ON reviews BEGIN SELECT RAISE(ABORT, 'review persistence failure'); END;").unwrap();
    assert!(matches!(
        core.create_review(
            "alice",
            "demo",
            number,
            "new-reviewer",
            request(ReviewState::Approved)
        ),
        Err(ForgeError::Storage(_))
    ));
    assert!(core.get_user("new-reviewer").is_err());
    assert_eq!(snapshot(&core, number), before);
    conn.execute_batch("DROP TRIGGER reject_review_insert;")
        .unwrap();
    // Closing the last handle proves durable rollback, not shared live state.
    drop(core);
    let core = ForgeCore::open_sqlite(&db).unwrap();
    assert!(core.get_user("new-reviewer").is_err());
    assert_eq!(snapshot(&core, number), before);
    let target = submit(&core, number, "reviewer", ReviewState::Approved);
    let before = snapshot(&core, number);
    conn.execute_batch("CREATE TRIGGER reject_dismissal_insert BEFORE INSERT ON reviews WHEN NEW.state = 'DISMISSED' BEGIN SELECT RAISE(ABORT, 'dismissal persistence failure'); END;").unwrap();
    assert!(matches!(
        core.dismiss_review("alice", "demo", number, "reviewer", dismissal(&target)),
        Err(ForgeError::Storage(_))
    ));
    assert_eq!(snapshot(&core, number), before);
    conn.execute_batch("DROP TRIGGER reject_dismissal_insert;")
        .unwrap();
    drop(core);
    let reopened = ForgeCore::open_sqlite(&db).unwrap();
    assert_eq!(snapshot(&reopened, number), before);
}

#[test]
fn dismissal_wire_requires_target_head_and_reason_and_preserves_historical_shape() {
    let core = ForgeCore::new();
    let number = seed(&core);
    let review = submit(&core, number, "reviewer", ReviewState::Approved);
    let mut historical = serde_json::to_value(&review).unwrap();
    historical
        .as_object_mut()
        .unwrap()
        .remove("dismissed_review_id");
    let decoded: Review = serde_json::from_value(historical).unwrap();
    assert_eq!(decoded.dismissed_review_id, None);
    let shape = serde_json::to_value(dismissal(&review)).unwrap();
    for field in ["review_id", "expected_head_sha", "reason"] {
        let mut missing = shape.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<DismissReviewRequest>(missing).is_err());
    }
}
