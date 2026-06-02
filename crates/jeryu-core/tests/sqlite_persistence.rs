use std::fs;
use std::os::unix::fs::PermissionsExt;

use jeryu_core::{
    CheckConclusion, CheckRunStatus, CommitStatusState, CreateCheckRunRequest,
    CreateCommentRequest, CreateCommitStatusRequest, CreateIssueRequest, CreateLabelRequest,
    CreateOrganizationRequest, CreatePullRequestRequest, CreateRepositoryRequest,
    CreateReviewRequest, CreateTeamRequest, CreateUserRequest, CreateWebhookRequest, ForgeCore,
    ForgeError, PullRequestCommit, ReviewCommentInput, ReviewState, SetBranchProtectionRequest,
    WebhookConfig,
};
use rusqlite::Connection;

#[test]
fn sqlite_store_round_trips_core_forge_resources() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("forge.sqlite");

    {
        let core = ForgeCore::open_sqlite(&db).unwrap();
        core.create_user(CreateUserRequest {
            login: "alice".to_string(),
            name: Some("Alice".to_string()),
            email: Some("alice@example.invalid".to_string()),
        })
        .unwrap();
        core.create_organization(CreateOrganizationRequest {
            login: "neverhuman".to_string(),
            display_name: Some("Neverhuman".to_string()),
        })
        .unwrap();
        core.create_team(
            "neverhuman",
            CreateTeamRequest {
                name: "Core Team".to_string(),
                slug: Some("core".to_string()),
                members: vec!["alice".to_string()],
            },
        )
        .unwrap();
        core.create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: true,
                description: Some("local forge".to_string()),
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
        core.create_label(
            "alice",
            "jeryu",
            CreateLabelRequest {
                name: "bug".to_string(),
                color: "ff0000".to_string(),
                description: Some("defect".to_string()),
            },
        )
        .unwrap();
        core.create_webhook(
            "alice",
            "jeryu",
            CreateWebhookRequest {
                name: "events".to_string(),
                active: true,
                events: vec![
                    "issues".to_string(),
                    "issue_comment".to_string(),
                    "pull_request".to_string(),
                    "pull_request_review".to_string(),
                    "status".to_string(),
                    "check_run".to_string(),
                ],
                config: WebhookConfig {
                    url: "https://hooks.invalid/jeryu".to_string(),
                    content_type: "json".to_string(),
                    secret: Some("secret".to_string()),
                },
            },
        )
        .unwrap();
        let issue = core
            .create_issue(
                "alice",
                "jeryu",
                "alice",
                CreateIssueRequest {
                    title: "persist issue".to_string(),
                    body: Some("body".to_string()),
                    labels: vec!["bug".to_string()],
                    assignees: vec!["alice".to_string()],
                    milestone: Some("v1".to_string()),
                },
            )
            .unwrap();
        core.add_issue_comment(
            "alice",
            "jeryu",
            issue.number,
            "alice",
            CreateCommentRequest {
                body: "confirmed".to_string(),
            },
        )
        .unwrap();
        core.set_branch_protection(
            "alice",
            "jeryu",
            "main",
            SetBranchProtectionRequest {
                required_status_checks: vec!["ci/fast".to_string()],
                required_approving_review_count: 1,
                enforce_admins: true,
                required_linear_history: true,
                allow_force_pushes: false,
                allow_deletions: false,
                require_signed_commits: true,
                require_jankurai_proof: true,
            },
        )
        .unwrap();
        core.set_codeowners("alice", "jeryu", "*.rs @alice")
            .unwrap();
        let pr = core
            .create_pull_request(
                "alice",
                "jeryu",
                "alice",
                CreatePullRequestRequest {
                    title: "persist pr".to_string(),
                    body: Some("change".to_string()),
                    head: "feature".to_string(),
                    base: "main".to_string(),
                    head_sha: Some("abc123".to_string()),
                    base_sha: Some("base123".to_string()),
                    draft: false,
                    commits: vec![PullRequestCommit {
                        sha: "abc123".to_string(),
                        verified: true,
                        parents: 1,
                    }],
                    changed_files: vec!["src/lib.rs".to_string()],
                },
            )
            .unwrap();
        core.create_review(
            "alice",
            "jeryu",
            pr.number,
            "alice",
            CreateReviewRequest {
                body: Some("approval receipt recorded".to_string()),
                event: ReviewState::Approved,
                comments: vec![ReviewCommentInput {
                    path: "src/lib.rs".to_string(),
                    line: Some(7),
                    body: "nice".to_string(),
                }],
            },
        )
        .unwrap();
        core.create_commit_status(
            "alice",
            "jeryu",
            "abc123",
            "alice",
            CreateCommitStatusRequest {
                state: CommitStatusState::Success,
                context: "ci/fast".to_string(),
                description: Some("green".to_string()),
                target_url: Some("https://ci.invalid/build/1".to_string()),
            },
        )
        .unwrap();
        core.create_check_run(
            "alice",
            "jeryu",
            CreateCheckRunRequest {
                name: "ci/fast".to_string(),
                head_sha: "abc123".to_string(),
                status: Some(CheckRunStatus::Completed),
                conclusion: Some(CheckConclusion::Success),
                details_url: Some("https://ci.invalid/check/1".to_string()),
                output: None,
            },
        )
        .unwrap();
    }

    let reopened = ForgeCore::open_sqlite(&db).unwrap();
    assert_eq!(
        reopened.get_user("alice").unwrap().name.as_deref(),
        Some("Alice")
    );
    assert_eq!(reopened.list_teams("neverhuman").unwrap().len(), 1);
    assert_eq!(reopened.list_repositories(None).len(), 1);
    assert_eq!(
        reopened.list_labels("alice", "jeryu").unwrap()[0].name,
        "bug"
    );
    assert_eq!(
        reopened.list_issues("alice", "jeryu", None).unwrap().len(),
        2
    );
    assert_eq!(
        reopened.list_issue_comments("alice", "jeryu", 1).unwrap()[0].body,
        "confirmed"
    );
    assert_eq!(
        reopened
            .list_pull_requests("alice", "jeryu", None)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(reopened.list_reviews("alice", "jeryu", 1).unwrap().len(), 1);
    assert_eq!(
        reopened.list_review_comments("alice", "jeryu", 1).unwrap()[0].path,
        "src/lib.rs"
    );
    assert_eq!(
        reopened
            .get_branch_protection("alice", "jeryu", "main")
            .unwrap()
            .required_status_checks,
        vec!["ci/fast".to_string()]
    );
    assert_eq!(
        reopened.get_codeowners("alice", "jeryu").unwrap(),
        "*.rs @alice"
    );
    assert_eq!(
        reopened
            .combined_status("alice", "jeryu", "abc123")
            .unwrap()
            .state,
        CommitStatusState::Success
    );
    assert_eq!(
        reopened
            .list_check_runs("alice", "jeryu", Some("abc123"))
            .unwrap()
            .total_count,
        1
    );
    assert_eq!(
        reopened.list_webhooks("alice", "jeryu").unwrap()[0].name,
        "events"
    );
    assert!(
        reopened
            .list_webhook_deliveries("alice", "jeryu")
            .unwrap()
            .iter()
            .any(|delivery| delivery.event == "issues")
    );

    let next_issue = reopened
        .create_issue(
            "alice",
            "jeryu",
            "alice",
            CreateIssueRequest {
                title: "next".to_string(),
                body: None,
                labels: Vec::new(),
                assignees: Vec::new(),
                milestone: None,
            },
        )
        .unwrap();
    assert_eq!(next_issue.number, 3);
    let next_pr = reopened
        .create_pull_request(
            "alice",
            "jeryu",
            "alice",
            CreatePullRequestRequest {
                title: "next pr".to_string(),
                body: None,
                head: "feature-2".to_string(),
                base: "main".to_string(),
                head_sha: Some("def456".to_string()),
                base_sha: None,
                draft: true,
                commits: Vec::new(),
                changed_files: Vec::new(),
            },
        )
        .unwrap();
    assert_eq!(next_pr.number, 2);
}

#[test]
fn failed_sqlite_write_rolls_back_memory_state() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("forge.sqlite");
    let core = ForgeCore::open_sqlite(&db).unwrap();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "stable".to_string(),
            private: false,
            description: None,
            default_branch: None,
        },
    )
    .unwrap();

    fs::set_permissions(&db, fs::Permissions::from_mode(0o400)).unwrap();
    let err = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "rolled-back".to_string(),
                private: false,
                description: None,
                default_branch: None,
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::Storage(_)));
    assert!(matches!(
        core.get_repository("alice", "rolled-back").unwrap_err(),
        ForgeError::NotFound(_)
    ));
    fs::set_permissions(&db, fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn sqlite_open_backfills_missing_default_branch_protection() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("forge.sqlite");

    {
        let core = ForgeCore::open_sqlite(&db).unwrap();
        core.create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "trunky".to_string(),
                private: false,
                description: None,
                default_branch: Some("trunk".to_string()),
            },
        )
        .unwrap();
    }

    let raw = Connection::open(&db).unwrap();
    raw.execute("DELETE FROM branch_protection_rules", [])
        .unwrap();
    drop(raw);

    let reopened = ForgeCore::open_sqlite(&db).unwrap();
    let rule = reopened
        .get_branch_protection("alice", "trunky", "trunk")
        .unwrap();
    assert!(rule.required_linear_history);
    assert_eq!(rule.branch, "trunk");

    let raw = Connection::open(&db).unwrap();
    let persisted: i64 = raw
        .query_row("SELECT COUNT(*) FROM branch_protection_rules", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(persisted, 1);
}
