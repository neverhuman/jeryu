//! Custody is enforced by public Core writers and survives complete reopening.

mod support;

use jeryu_core::*;
use serde_json::json;
use support::private_directory;
use uuid::Uuid;

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn repository(core: &ForgeCore, owner: &str, name: &str) -> Repository {
    core.create_repository(
        owner,
        CreateRepositoryRequest {
            name: name.into(),
            ..Default::default()
        },
    )
    .unwrap()
}

fn pull(core: &ForgeCore, owner: &str, source: Option<String>) -> Result<PullRequest> {
    core.create_pull_request(
        owner,
        "demo",
        "author",
        CreatePullRequestRequest {
            title: "change".into(),
            head: "topic".into(),
            base: "main".into(),
            head_sha: Some(HEAD.into()),
            source_repository: source,
            ..Default::default()
        },
    )
}

fn review_request() -> CreateReviewRequest {
    CreateReviewRequest {
        event: ReviewState::Approved,
        expected_head_sha: Some(HEAD.into()),
        body: None,
        comments: Vec::new(),
    }
}

fn read_only() -> RepositoryMutationBlock {
    RepositoryMutationBlock::ReadOnly {
        reason: "reviewed canonical inclusion".into(),
        evidence: "sha256:preserved-disposition-receipt".into(),
    }
}

fn assert_denied<T: std::fmt::Debug>(result: Result<T>) {
    assert!(
        matches!(result, Err(ForgeError::Forbidden(_))),
        "expected custody denial, got {result:?}"
    );
}

#[test]
fn every_repository_writer_denies_read_only_custody_including_administrator_calls() {
    let core = ForgeCore::new();
    let repo = repository(&core, "legacy", "demo");
    core.create_account("admin", "correct horse battery", UserRole::Admin)
        .unwrap();
    let pr = pull(&core, "legacy", None).unwrap();
    let review = core
        .create_review("legacy", "demo", pr.number, "reviewer", review_request())
        .unwrap();
    core.set_branch_protection(
        "legacy",
        "demo",
        "main",
        SetBranchProtectionRequest {
            allow_force_pushes: true,
            allow_deletions: true,
            ..Default::default()
        },
    )
    .unwrap();
    let transfer = core
        .prepare_repository_transfer(PrepareRepositoryTransfer {
            repository_id: repo.id,
            expected_source_owner: "legacy".into(),
            expected_source_name: "demo".into(),
            destination_owner: "canonical".into(),
            request_fingerprint: "exact-reviewed-transfer".into(),
            idempotency_key: "transfer-1".into(),
        })
        .unwrap();
    core.block_repository_mutations(repo.id, read_only())
        .unwrap();
    let before_pr = core.get_pull_request("legacy", "demo", pr.number).unwrap();
    let before_reviews = core.list_reviews("legacy", "demo", pr.number).unwrap();
    let before_issues = core.list_issues("legacy", "demo", None).unwrap();

    assert_denied(core.set_repository_visibility("legacy", "demo", true));
    assert_denied(core.create_repository_with_id(
        repo.id,
        "legacy",
        CreateRepositoryRequest {
            name: "demo".into(),
            ..Default::default()
        },
    ));
    assert_denied(core.set_repository_family("legacy", "demo", Some("retained".into())));
    assert_denied(core.set_repository_family_with_id(
        "legacy",
        "demo",
        repo.id,
        Some("retained".into()),
    ));
    assert_denied(core.delete_repository("legacy", "demo"));
    assert_denied(core.create_label(
        "legacy",
        "demo",
        CreateLabelRequest {
            name: "label".into(),
            color: "ffffff".into(),
            description: None,
        },
    ));
    assert_denied(core.create_issue(
        "legacy",
        "demo",
        "new-profile",
        CreateIssueRequest {
            title: "issue".into(),
            ..Default::default()
        },
    ));
    assert_denied(core.update_issue(
        "legacy",
        "demo",
        pr.issue_number,
        UpdateIssueRequest {
            title: Some("changed".into()),
            ..Default::default()
        },
    ));
    assert_denied(core.add_issue_comment(
        "legacy",
        "demo",
        pr.issue_number,
        "new-profile",
        CreateCommentRequest {
            body: "comment".into(),
        },
    ));
    assert_denied(pull(&core, "legacy", None));
    assert_denied(core.update_pull_request(
        "legacy",
        "demo",
        pr.number,
        UpdatePullRequestRequest {
            title: Some("changed".into()),
            ..Default::default()
        },
    ));
    assert_denied(core.refresh_pull_request_heads_for_ref("legacy", "demo", "topic", HEAD));
    assert_denied(core.finalize_merge("legacy", "demo", pr.number, HEAD.into(), Some(HEAD)));
    assert_denied(core.merge_pull_request(
        "legacy",
        "demo",
        pr.number,
        MergePullRequestRequest {
            sha: Some(HEAD.into()),
            ..Default::default()
        },
    ));
    assert_denied(core.create_review("legacy", "demo", pr.number, "admin", review_request()));
    assert_denied(core.dismiss_review(
        "legacy",
        "demo",
        pr.number,
        "reviewer",
        DismissReviewRequest {
            review_id: review.id,
            expected_head_sha: HEAD.into(),
            reason: "withdraw".into(),
        },
    ));
    assert_denied(core.create_check_run(
        "legacy",
        "demo",
        CreateCheckRunRequest {
            name: "demo/required".into(),
            head_sha: HEAD.into(),
            ..Default::default()
        },
    ));
    assert_denied(core.create_commit_status(
        "legacy",
        "demo",
        HEAD,
        "new-profile",
        CreateCommitStatusRequest {
            context: "demo/required".into(),
            state: CommitStatusState::Success,
            description: None,
            target_url: None,
        },
    ));
    assert_denied(core.record_jankurai_score(
        "legacy",
        "demo",
        RecordJankuraiScoreRequest {
            branch: "topic".into(),
            commit_sha: HEAD.into(),
            decision: "pass".into(),
            score: Some(100),
            ..Default::default()
        },
    ));
    assert_denied(core.set_branch_protection(
        "legacy",
        "demo",
        "main",
        SetBranchProtectionRequest::default(),
    ));
    assert_denied(core.set_codeowners("legacy", "demo", "* @admin"));
    assert_denied(core.force_push("legacy", "demo", "main", true));
    assert_denied(core.delete_ref("legacy", "demo", "main", true));
    assert_denied(core.set_repository_readme("legacy", "demo", "changed".into()));
    assert_denied(core.create_webhook(
        "legacy",
        "demo",
        CreateWebhookRequest {
            name: "hook".into(),
            active: true,
            events: vec!["push".into()],
            config: WebhookConfig {
                url: "https://hooks.invalid/event".into(),
                content_type: "json".into(),
                secret: None,
            },
        },
    ));
    assert_denied(core.grant_repo_access(
        "admin",
        "admin",
        "legacy",
        "demo",
        RepoAccessLevel::Admin,
    ));
    assert_denied(core.grant_repo_access_checked(
        "admin",
        "admin",
        "legacy",
        "demo",
        RepoAccessLevel::Admin,
    ));
    assert_denied(core.revoke_repo_access("admin", "legacy", "demo"));
    assert_denied(core.revoke_repo_access_checked("admin", "admin", "legacy", "demo"));
    assert_denied(core.prepare_repository_transfer(PrepareRepositoryTransfer {
        repository_id: repo.id,
        expected_source_owner: "legacy".into(),
        expected_source_name: "demo".into(),
        destination_owner: "elsewhere".into(),
        request_fingerprint: "another-transfer".into(),
        idempotency_key: "transfer-2".into(),
    }));
    assert_denied(
        core.commit_repository_transfer(transfer.transaction_id, json!({"storage": "moved"})),
    );
    assert_denied(core.fail_repository_transfer(transfer.transaction_id, "failure"));
    assert_denied(core.append_audit("repository.update", "legacy/demo", "requested", json!({})));

    assert_eq!(
        core.get_pull_request("legacy", "demo", pr.number).unwrap(),
        before_pr
    );
    assert_eq!(
        core.list_reviews("legacy", "demo", pr.number).unwrap(),
        before_reviews
    );
    assert_eq!(
        core.list_issues("legacy", "demo", None).unwrap(),
        before_issues
    );
    assert_eq!(core.get_repository_by_id(repo.id).unwrap(), repo);
    assert_eq!(
        core.get_repository_transfer("transfer-1").unwrap(),
        transfer
    );
    assert!(core.get_repository_transfer("transfer-2").is_none());
    assert!(core.get_user("new-profile").is_err());
    assert!(core.list_repo_access("legacy", "demo").is_empty());
    assert_eq!(
        core.list_check_runs("legacy", "demo", Some(HEAD))
            .unwrap()
            .total_count,
        0
    );
}

#[test]
fn custody_is_stable_across_unrelated_saves_and_complete_runtime_reopening() {
    let directory = private_directory();
    let database = directory.path().join("forge.sqlite");
    let root = directory.path().join("git");
    let core = ForgeCore::open_managed(&database, &root).unwrap();
    let repo = repository(&core, "legacy", "demo");
    let block = core
        .block_repository_mutations(repo.id, read_only())
        .unwrap();
    repository(&core, "active", "demo");
    core.ensure_user("unrelated").unwrap();
    drop(core);

    let core = ForgeCore::open_managed(&database, &root).unwrap();
    assert_eq!(
        core.repository_mutation_block(repo.id).unwrap(),
        Some(block.clone())
    );
    assert_denied(core.set_repository_readme("legacy", "demo", "changed".into()));
    core.set_repository_readme("active", "demo", "allowed".into())
        .unwrap();
    drop(core);
    let core = ForgeCore::open_managed(&database, &root).unwrap();
    assert_eq!(
        core.repository_mutation_block(repo.id).unwrap(),
        Some(block)
    );
    assert_eq!(
        core.get_repository_readme("active", "demo")
            .unwrap()
            .as_deref(),
        Some("allowed")
    );
}

#[test]
fn reconciliation_denial_survives_reopen_and_does_not_hide_reads() {
    let directory = private_directory();
    let database = directory.path().join("forge.sqlite");
    let core = ForgeCore::open_sqlite(&database).unwrap();
    let repo = repository(&core, "alice", "demo");
    let operation_id = Uuid::new_v4();
    let block = RepositoryMutationBlock::ReconciliationRequired {
        operation_id,
        reason: "Git result requires recorded reconciliation".into(),
    };
    core.block_repository_mutations(repo.id, block.clone())
        .unwrap();
    drop(core);
    let core = ForgeCore::open_sqlite(&database).unwrap();
    assert_eq!(
        core.repository_mutation_block(repo.id).unwrap(),
        Some(block.clone())
    );
    assert_eq!(core.get_repository("alice", "demo").unwrap().id, repo.id);
    let result = core.set_codeowners("alice", "demo", "* @reviewer");
    assert!(
        matches!(result, Err(ForgeError::WriterUnavailable(ref message)) if message.contains(&operation_id.to_string())),
        "{result:?}"
    );
    assert_eq!(
        core.block_repository_mutations(repo.id, block.clone())
            .unwrap(),
        block
    );
    assert!(matches!(
        core.block_repository_mutations(repo.id, read_only()),
        Err(ForgeError::Conflict(_))
    ));
}

#[test]
fn a_read_only_repository_can_supply_a_fork_without_being_mutated() {
    let core = ForgeCore::new();
    let source = repository(&core, "legacy", "demo");
    repository(&core, "canonical", "demo");
    core.block_repository_mutations(source.id, read_only())
        .unwrap();
    let pr = pull(&core, "canonical", Some("legacy/demo".into())).unwrap();
    assert_eq!(pr.source_repository, "legacy/demo");
    assert!(
        core.list_pull_requests("legacy", "demo", None)
            .unwrap()
            .is_empty()
    );
    assert_denied(pull(&core, "legacy", Some("canonical/demo".into())));
}

#[test]
fn a_reconciliation_blocked_source_cannot_supply_a_new_fork_operation() {
    let core = ForgeCore::new();
    let source = repository(&core, "source", "demo");
    repository(&core, "destination", "demo");
    core.block_repository_mutations(
        source.id,
        RepositoryMutationBlock::ReconciliationRequired {
            operation_id: Uuid::new_v4(),
            reason: "unexplained ref result".into(),
        },
    )
    .unwrap();
    assert!(matches!(
        pull(&core, "destination", Some("source/demo".into())),
        Err(ForgeError::WriterUnavailable(_))
    ));
    assert!(
        core.list_pull_requests("destination", "demo", None)
            .unwrap()
            .is_empty()
    );
    assert!(core.get_user("author").is_err());
}

#[test]
fn transferred_old_alias_cannot_bypass_custody_through_audit() {
    let core = ForgeCore::new();
    let repo = repository(&core, "old-owner", "demo");
    let transfer = core
        .prepare_repository_transfer(PrepareRepositoryTransfer {
            repository_id: repo.id,
            expected_source_owner: "old-owner".into(),
            expected_source_name: "demo".into(),
            destination_owner: "new-owner".into(),
            request_fingerprint: "transfer".into(),
            idempotency_key: "transfer".into(),
        })
        .unwrap();
    core.commit_repository_transfer(transfer.transaction_id, json!({"verified": true}))
        .unwrap();
    core.block_repository_mutations(repo.id, read_only())
        .unwrap();
    assert_denied(core.append_audit(
        "repository.update",
        "old-owner/demo",
        "requested",
        json!({}),
    ));
    assert_denied(core.append_audit(
        "repository.update",
        "new-owner/demo",
        "requested",
        json!({}),
    ));
    assert_eq!(
        core.get_repository_by_id(repo.id).unwrap().owner,
        "new-owner"
    );
}

#[test]
fn archived_or_disabled_historical_rows_deny_direct_core_writers() {
    for column in ["archived", "disabled"] {
        let directory = private_directory();
        let database = directory.path().join("forge.sqlite");
        let core = ForgeCore::open_sqlite(&database).unwrap();
        repository(&core, "legacy", "demo");
        drop(core);
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection
            .execute(&format!("UPDATE repositories SET {column} = 1"), [])
            .unwrap();
        drop(connection);
        let core = ForgeCore::open_sqlite(&database).unwrap();
        assert_denied(core.create_check_run(
            "legacy",
            "demo",
            CreateCheckRunRequest {
                name: "required".into(),
                head_sha: HEAD.into(),
                ..Default::default()
            },
        ));
        assert_denied(core.delete_repository("legacy", "demo"));
        assert!(core.get_repository("legacy", "demo").is_ok());
    }
}
