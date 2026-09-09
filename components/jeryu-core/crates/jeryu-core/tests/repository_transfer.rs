//! UUID-preserving repository-transfer and persistence coverage.

mod support;

use support::private_directory;

use jeryu_core::{
    CheckConclusion, CheckRunStatus, CommitStatusState, CreateCheckRunRequest,
    CreateCommentRequest, CreateCommitStatusRequest, CreateIssueRequest, CreateLabelRequest,
    CreatePullRequestRequest, CreateRepositoryRequest, CreateReviewRequest, CreateWebhookRequest,
    ForgeCore, ForgeError, PrepareRepositoryTransfer, RecordJankuraiScoreRequest, RepoAccessLevel,
    ReviewCommentInput, ReviewState, UserRole, WebhookConfig,
};
use rusqlite::Connection;
use serde_json::json;

const REPO_SCOPED_TABLES: [&str; 16] = [
    "repo_access_grants",
    "labels",
    "issues",
    "issue_comments",
    "pull_requests",
    "reviews",
    "review_comments",
    "branch_protection_rules",
    "codeowners",
    "repository_readmes",
    "commit_statuses",
    "check_runs",
    "webhooks",
    "webhook_deliveries",
    "repo_counters",
    "jankurai_scores",
];

fn seed_full_repo(core: &ForgeCore) -> uuid::Uuid {
    let repo = core
        .create_repository(
            "jeryu",
            CreateRepositoryRequest {
                name: "redline".to_string(),
                private: true,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    core.create_account("operator", "correct horse battery", UserRole::Admin)
        .unwrap();
    core.grant_repo_access(
        "operator",
        "operator",
        "jeryu",
        "redline",
        RepoAccessLevel::Admin,
    )
    .unwrap();
    core.create_webhook(
        "jeryu",
        "redline",
        CreateWebhookRequest {
            name: "events".to_string(),
            active: true,
            events: vec!["issues".to_string(), "pull_request".to_string()],
            config: WebhookConfig {
                url: "https://hooks.invalid/redline".to_string(),
                content_type: "json".to_string(),
                secret: Some("secret".to_string()),
            },
        },
    )
    .unwrap();
    core.create_label(
        "jeryu",
        "redline",
        CreateLabelRequest {
            name: "release".to_string(),
            color: "ff0000".to_string(),
            description: None,
        },
    )
    .unwrap();
    let issue = core
        .create_issue(
            "jeryu",
            "redline",
            "operator",
            CreateIssueRequest {
                title: "transfer".to_string(),
                body: None,
                labels: vec!["release".to_string()],
                assignees: Vec::new(),
                milestone: None,
            },
        )
        .unwrap();
    core.add_issue_comment(
        "jeryu",
        "redline",
        issue.number,
        "operator",
        CreateCommentRequest {
            body: "preserve me".to_string(),
        },
    )
    .unwrap();
    core.set_codeowners("jeryu", "redline", "*.rs @reviewer")
        .unwrap();
    core.set_repository_readme("jeryu", "redline", "# Redline\n".to_string())
        .unwrap();
    let pull = core
        .create_pull_request(
            "jeryu",
            "redline",
            "operator",
            CreatePullRequestRequest {
                title: "change".to_string(),
                body: None,
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some("abc123".to_string()),
                base_sha: Some("base123".to_string()),
                source_repository: Some("jeryu/redline".to_string()),
                draft: false,
                commits: Vec::new(),
                changed_files: vec!["src/lib.rs".to_string()],
            },
        )
        .unwrap();
    core.create_review(
        "jeryu",
        "redline",
        pull.number,
        "reviewer",
        CreateReviewRequest {
            body: None,
            event: ReviewState::Approved,
            comments: vec![ReviewCommentInput {
                path: "src/lib.rs".to_string(),
                line: Some(1),
                body: "preserve".to_string(),
            }],
            expected_head_sha: None,
        },
    )
    .unwrap();
    core.create_commit_status(
        "jeryu",
        "redline",
        "abc123",
        "operator",
        CreateCommitStatusRequest {
            state: CommitStatusState::Success,
            context: "redline/required".to_string(),
            description: None,
            target_url: None,
        },
    )
    .unwrap();
    core.create_check_run(
        "jeryu",
        "redline",
        CreateCheckRunRequest {
            name: "redline/required".to_string(),
            head_sha: "abc123".to_string(),
            status: Some(CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Success),
            details_url: None,
            output: None,
        },
    )
    .unwrap();
    core.record_jankurai_score(
        "jeryu",
        "redline",
        RecordJankuraiScoreRequest {
            branch: "main".to_string(),
            commit_sha: "abc123".to_string(),
            score: Some(90),
            hard_findings: Some(0),
            decision: "scored".to_string(),
            caps_applied: Vec::new(),
            report: None,
            tool_exit: None,
        },
    )
    .unwrap();
    repo.id
}

fn transfer(repository_id: uuid::Uuid, fingerprint: &str) -> PrepareRepositoryTransfer {
    PrepareRepositoryTransfer {
        repository_id,
        expected_source_owner: "jeryu".to_string(),
        expected_source_name: "redline".to_string(),
        destination_owner: "veox".to_string(),
        request_fingerprint: fingerprint.to_string(),
        idempotency_key: "redline-to-veox-1".to_string(),
    }
}

#[test]
fn transfer_preserves_uuid_scoped_state_alias_and_journal_across_reopen() {
    let temp = private_directory();
    let database = temp.path().join("forge.sqlite");
    let repository_id;
    let transaction_id;
    {
        let core = ForgeCore::open_sqlite(&database).unwrap();
        repository_id = seed_full_repo(&core);
        let prepared = core
            .prepare_repository_transfer(transfer(repository_id, "sha256:request"))
            .unwrap();
        transaction_id = prepared.transaction_id;
        assert_eq!(
            core.get_repository("jeryu", "redline").unwrap().id,
            repository_id
        );
    }
    {
        let core = ForgeCore::open_sqlite(&database).unwrap();
        let prepared = core
            .get_repository_transfer("redline-to-veox-1")
            .expect("prepared journal survives");
        assert_eq!(prepared.transaction_id, transaction_id);
        core.commit_repository_transfer(
            transaction_id,
            json!({"schema_version": "jeryu.repository-transfer/v1"}),
        )
        .unwrap();
    }

    let core = ForgeCore::open_sqlite(&database).unwrap();
    assert!(matches!(
        core.get_repository("jeryu", "redline"),
        Err(ForgeError::NotFound(_))
    ));
    assert_eq!(
        core.get_repository("veox", "redline").unwrap().id,
        repository_id
    );
    assert_eq!(
        core.get_repository_by_id(repository_id).unwrap().full_name,
        "veox/redline"
    );
    assert_eq!(core.get_issue("veox", "redline", 1).unwrap().owner, "veox");
    assert_eq!(
        core.get_pull_request("veox", "redline", 1).unwrap().owner,
        "veox"
    );
    assert_eq!(
        core.get_pull_request("veox", "redline", 1)
            .unwrap()
            .source_repository,
        "veox/redline"
    );
    assert_eq!(
        core.list_check_runs("veox", "redline", Some("abc123"))
            .unwrap()
            .total_count,
        1
    );
    assert_eq!(core.list_repo_access("veox", "redline").len(), 1);
    assert_eq!(
        core.list_jankurai_scores("veox", "redline", None, None)
            .unwrap()
            .len(),
        1
    );
    let connection = Connection::open(&database).unwrap();
    for table in REPO_SCOPED_TABLES {
        let count: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE repo_id = ?1"),
                rusqlite::params![repository_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert!(count > 0, "{table} lost repository-scoped rows");
    }
    let alias = core
        .get_repository_alias("jeryu", "redline")
        .expect("old slug is retained");
    assert_eq!(alias.repository_id, repository_id);
    assert_eq!(alias.canonical_owner, "veox");
    let committed = core
        .get_repository_transfer("redline-to-veox-1")
        .expect("journal survives commit");
    assert_eq!(
        committed.receipt.unwrap()["schema_version"],
        "jeryu.repository-transfer/v1"
    );

    let replay = core
        .prepare_repository_transfer(transfer(repository_id, "sha256:request"))
        .unwrap();
    assert_eq!(replay.transaction_id, transaction_id);
    assert!(matches!(
        core.prepare_repository_transfer(transfer(repository_id, "sha256:different")),
        Err(ForgeError::Conflict(_))
    ));
    let mut wrong_repository = transfer(uuid::Uuid::new_v4(), "sha256:request");
    wrong_repository.idempotency_key = "redline-to-veox-1".to_string();
    assert!(matches!(
        core.prepare_repository_transfer(wrong_repository),
        Err(ForgeError::Conflict(_))
    ));
}
