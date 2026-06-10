//! Registry deletion + audit-trail durability against the SQLite store.
//!
//! Persistence is a full-state rewrite: every mutation runs `delete_all` and
//! reinserts the whole in-memory `State`. These tests pin the two invariants
//! that design forces on deletion: (1) removing a repo from every `State` map
//! and persisting once leaves zero rows for it in every table while another
//! repo's rows survive, and (2) the `forge_audit_log` table — appended outside
//! the rewrite — survives arbitrary later mutations.

use jeryu_core::{
    CheckConclusion, CheckRunStatus, CommitStatusState, CreateCheckRunRequest,
    CreateCommentRequest, CreateCommitStatusRequest, CreateIssueRequest, CreateLabelRequest,
    CreatePullRequestRequest, CreateRepositoryRequest, CreateReviewRequest, CreateWebhookRequest,
    ForgeCore, ForgeError, PullRequestCommit, ReviewCommentInput, ReviewState, WebhookConfig,
};
use rusqlite::Connection;

/// Tables keyed by `repo_id` whose rows must vanish with the repository.
const REPO_SCOPED_TABLES: [&str; 14] = [
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
];

/// Seed `owner/repo` with at least one of everything that is repo-scoped:
/// webhook (created FIRST so later events leave deliveries), label, issue +
/// comment, PR + review + review comment, branch protection (default rule from
/// create), codeowners, readme, commit status, check run. Returns the repo id.
fn seed_full_repo(core: &ForgeCore, owner: &str, repo: &str) -> String {
    let created = core
        .create_repository(
            owner,
            CreateRepositoryRequest {
                name: repo.to_string(),
                private: true,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    core.create_webhook(
        owner,
        repo,
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
    core.create_label(
        owner,
        repo,
        CreateLabelRequest {
            name: "bug".to_string(),
            color: "ff0000".to_string(),
            description: None,
        },
    )
    .unwrap();
    let issue = core
        .create_issue(
            owner,
            repo,
            owner,
            CreateIssueRequest {
                title: "doomed issue".to_string(),
                body: None,
                labels: vec!["bug".to_string()],
                assignees: Vec::new(),
                milestone: None,
            },
        )
        .unwrap();
    core.add_issue_comment(
        owner,
        repo,
        issue.number,
        owner,
        CreateCommentRequest {
            body: "confirmed".to_string(),
        },
    )
    .unwrap();
    core.set_codeowners(owner, repo, "*.rs @alice").unwrap();
    core.set_repository_readme(owner, repo, format!("# {owner}/{repo}\n"))
        .unwrap();
    let pr = core
        .create_pull_request(
            owner,
            repo,
            owner,
            CreatePullRequestRequest {
                title: "doomed pr".to_string(),
                body: None,
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some("abc123".to_string()),
                base_sha: Some("base123".to_string()),
                source_repository: None,
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
        owner,
        repo,
        pr.number,
        owner,
        CreateReviewRequest {
            body: Some("looks fine".to_string()),
            event: ReviewState::Approved,
            comments: vec![ReviewCommentInput {
                path: "src/lib.rs".to_string(),
                line: Some(1),
                body: "nice".to_string(),
            }],
        },
    )
    .unwrap();
    core.create_commit_status(
        owner,
        repo,
        "abc123",
        owner,
        CreateCommitStatusRequest {
            state: CommitStatusState::Success,
            context: "ci/fast".to_string(),
            description: None,
            target_url: None,
        },
    )
    .unwrap();
    core.create_check_run(
        owner,
        repo,
        CreateCheckRunRequest {
            name: "ci/fast".to_string(),
            head_sha: "abc123".to_string(),
            status: Some(CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Success),
            details_url: None,
            output: None,
        },
    )
    .unwrap();
    created.id.to_string()
}

fn repo_scoped_rows(conn: &Connection, table: &str, repo_id: &str) -> i64 {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE repo_id = ?1"),
        rusqlite::params![repo_id],
        |row| row.get(0),
    )
    .unwrap()
}

/// Delete a fully-populated repo: the reopened store has no trace of it (in
/// the typed API and raw SQL row counts alike) while a sibling repo's data
/// survives untouched.
#[test]
fn delete_repository_purges_every_table_and_spares_siblings() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("forge.sqlite");

    let (doomed_id, keeper_id, doomed_hook_id) = {
        let core = ForgeCore::open_sqlite(&db).unwrap();
        let doomed_id = seed_full_repo(&core, "alice", "doomed");
        let keeper_id = seed_full_repo(&core, "alice", "keeper");
        let doomed_hook_id = core.list_webhooks("alice", "doomed").unwrap()[0]
            .id
            .to_string();

        let deletion = core.delete_repository("alice", "doomed").unwrap();
        assert_eq!(deletion.repo.full_name, "alice/doomed");
        assert_eq!(deletion.labels, 1);
        // The PR creates a companion issue entry alongside the seeded issue.
        assert_eq!(deletion.issues, 2);
        assert_eq!(deletion.issue_comments, 1);
        assert_eq!(deletion.pulls, 1);
        assert_eq!(deletion.reviews, 1);
        assert_eq!(deletion.review_comments, 1);
        assert_eq!(deletion.branch_protections, 1);
        assert_eq!(deletion.codeowners, 1);
        assert_eq!(deletion.readmes, 1);
        assert_eq!(deletion.commit_statuses, 1);
        assert_eq!(deletion.check_runs, 1);
        assert_eq!(deletion.webhooks, 1);
        assert!(
            deletion.webhook_deliveries >= 1,
            "the seeded webhook must have produced deliveries to delete"
        );
        assert_eq!(deletion.counters, 1);

        assert!(matches!(
            core.delete_repository("alice", "doomed"),
            Err(ForgeError::NotFound(_))
        ));
        (doomed_id, keeper_id, doomed_hook_id)
    };

    // Reopen the store from disk: the deletion must have been persisted.
    let reopened = ForgeCore::open_sqlite(&db).unwrap();
    assert!(matches!(
        reopened.get_repository("alice", "doomed"),
        Err(ForgeError::NotFound(_))
    ));
    let keeper = reopened.get_repository("alice", "keeper").unwrap();
    assert_eq!(keeper.id.to_string(), keeper_id);
    assert_eq!(reopened.list_repositories(None).len(), 1);
    assert_eq!(reopened.list_labels("alice", "keeper").unwrap().len(), 1);
    assert_eq!(
        reopened
            .list_pull_requests("alice", "keeper", None)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(reopened.list_webhooks("alice", "keeper").unwrap().len(), 1);
    drop(reopened);

    // Raw SQL ground truth: zero rows anywhere for the doomed repo, live rows
    // for the keeper.
    let raw = Connection::open(&db).unwrap();
    let doomed_repo_rows: i64 = raw
        .query_row(
            "SELECT COUNT(*) FROM repositories WHERE id = ?1",
            rusqlite::params![doomed_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(doomed_repo_rows, 0);
    for table in REPO_SCOPED_TABLES {
        assert_eq!(
            repo_scoped_rows(&raw, table, &doomed_id),
            0,
            "table {table} still holds rows for the deleted repo"
        );
        assert!(
            repo_scoped_rows(&raw, table, &keeper_id) >= 1,
            "table {table} lost the surviving repo's rows"
        );
    }
    // webhook_metadata is keyed by hook id, not repo_id: the deleted repo's
    // hook must not have left a metadata row behind.
    let stale_metadata: i64 = raw
        .query_row(
            "SELECT COUNT(*) FROM webhook_metadata WHERE id = ?1",
            rusqlite::params![doomed_hook_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stale_metadata, 0);
}

/// The audit trail is OUTSIDE the full-state rewrite: entries appended before
/// and between mutations survive every persist (each of which runs
/// `delete_all` on the state tables), survive reopen, and survive the deletion
/// of the subject repository itself.
#[test]
fn forge_audit_log_survives_full_state_rewrites() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("forge.sqlite");

    let (requested_id, completed_id) = {
        let core = ForgeCore::open_sqlite(&db).unwrap();
        let requested_id = core
            .append_audit(
                "repository.delete",
                "alice/doomed",
                "requested",
                serde_json::json!({ "delete_storage": false }),
            )
            .unwrap();
        // Full-state rewrites between the two appends.
        seed_full_repo(&core, "alice", "doomed");
        core.delete_repository("alice", "doomed").unwrap();
        let completed_id = core
            .append_audit(
                "repository.delete",
                "alice/doomed",
                "completed",
                serde_json::json!({ "registry_deleted": true }),
            )
            .unwrap();

        // Invalid inputs are rejected before any write.
        assert!(matches!(
            core.append_audit("  ", "alice/doomed", "requested", serde_json::json!({})),
            Err(ForgeError::Validation(_))
        ));
        assert!(matches!(
            core.append_audit("x", "alice/doomed", "started", serde_json::json!({})),
            Err(ForgeError::Validation(_))
        ));
        (requested_id, completed_id)
    };

    let reopened = ForgeCore::open_sqlite(&db).unwrap();
    let trail = reopened.list_audit("alice/doomed").unwrap();
    assert_eq!(trail.len(), 2);
    assert_eq!(trail[0].id, requested_id);
    assert_eq!(trail[0].phase, "requested");
    assert_eq!(trail[0].actor, "local");
    assert_eq!(
        trail[0].detail,
        serde_json::json!({ "delete_storage": false })
    );
    assert_eq!(trail[1].id, completed_id);
    assert_eq!(trail[1].phase, "completed");
    assert!(reopened.list_audit("alice/other").unwrap().is_empty());
    drop(reopened);

    let raw = Connection::open(&db).unwrap();
    let rows: i64 = raw
        .query_row("SELECT COUNT(*) FROM forge_audit_log", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 2);

    // The in-memory core has no store: append is a validated no-op that still
    // returns an id, and the trail reads empty.
    let in_memory = ForgeCore::new();
    let id = in_memory
        .append_audit(
            "repository.delete",
            "a/b",
            "requested",
            serde_json::json!({}),
        )
        .unwrap();
    assert!(!id.is_empty());
    assert!(in_memory.list_audit("a/b").unwrap().is_empty());
}
