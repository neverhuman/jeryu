use super::*;

fn core_with_repo() -> ForgeCore {
    let core = ForgeCore::new();
    core.create_user(CreateUserRequest {
        login: "alice".to_string(),
        name: None,
        email: None,
    })
    .unwrap();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: true,
            description: Some("forge".to_string()),
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    core
}

#[test]
fn lifecycle_issue_and_comment() {
    let core = core_with_repo();
    let issue = core
        .create_issue(
            "alice",
            "jeryu",
            "alice",
            CreateIssueRequest {
                title: "bug".to_string(),
                body: Some("fix it".to_string()),
                labels: vec!["bug".to_string()],
                assignees: Vec::new(),
                milestone: None,
            },
        )
        .unwrap();
    assert_eq!(issue.number, 1);
    let comment = core
        .add_issue_comment(
            "alice",
            "jeryu",
            issue.number,
            "alice",
            CreateCommentRequest {
                body: "confirmed".to_string(),
            },
        )
        .unwrap();
    assert_eq!(comment.issue_number, issue.number);
    assert_eq!(
        core.get_issue("alice", "jeryu", issue.number)
            .unwrap()
            .comments,
        1
    );
}

#[test]
fn branch_protection_blocks_merge_until_review_and_status_pass() {
    let core = core_with_repo();
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
            require_signed_commits: false,
            require_jankurai_proof: false,
        },
    )
    .unwrap();
    let pr = core
        .create_pull_request(
            "alice",
            "jeryu",
            "alice",
            CreatePullRequestRequest {
                title: "change".to_string(),
                body: None,
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some("abc".to_string()),
                base_sha: None,
                draft: false,
                commits: Vec::new(),
                changed_files: Vec::new(),
            },
        )
        .unwrap();
    assert!(!pr.mergeable);
    assert!(
        core.merge_pull_request(
            "alice",
            "jeryu",
            pr.number,
            MergePullRequestRequest::default()
        )
        .is_err()
    );
    core.create_review(
        "alice",
        "jeryu",
        pr.number,
        "alice",
        CreateReviewRequest {
            body: None,
            event: ReviewState::Approved,
            comments: Vec::new(),
        },
    )
    .unwrap();
    core.create_commit_status(
        "alice",
        "jeryu",
        "abc",
        "alice",
        CreateCommitStatusRequest {
            state: CommitStatusState::Success,
            context: "ci/fast".to_string(),
            description: None,
            target_url: None,
        },
    )
    .unwrap();
    let merge = core
        .merge_pull_request(
            "alice",
            "jeryu",
            pr.number,
            MergePullRequestRequest::default(),
        )
        .unwrap();
    assert!(merge.merged);
}

#[test]
fn check_run_satisfies_required_context() {
    let core = core_with_repo();
    core.set_branch_protection(
        "alice",
        "jeryu",
        "main",
        SetBranchProtectionRequest {
            required_status_checks: vec!["ci/fast".to_string()],
            required_approving_review_count: 0,
            enforce_admins: false,
            required_linear_history: false,
            allow_force_pushes: false,
            allow_deletions: false,
            require_signed_commits: false,
            require_jankurai_proof: false,
        },
    )
    .unwrap();
    let pr = core
        .create_pull_request(
            "alice",
            "jeryu",
            "alice",
            CreatePullRequestRequest {
                title: "change".to_string(),
                body: None,
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some("abc".to_string()),
                base_sha: None,
                draft: false,
                commits: Vec::new(),
                changed_files: Vec::new(),
            },
        )
        .unwrap();
    assert!(!pr.mergeable);
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "ci/fast".to_string(),
            head_sha: "abc".to_string(),
            status: Some(CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Success),
            details_url: None,
            output: None,
        },
    )
    .unwrap();
    let pr = core.get_pull_request("alice", "jeryu", pr.number).unwrap();
    assert!(pr.mergeable);
}

#[test]
fn webhook_outbox_records_matching_events() {
    let core = core_with_repo();
    core.create_webhook(
        "alice",
        "jeryu",
        CreateWebhookRequest {
            name: "web".to_string(),
            active: true,
            events: vec!["issues".to_string()],
            config: WebhookConfig {
                url: "https://hooks.invalid/jeryu".to_string(),
                content_type: "json".to_string(),
                secret: Some("secret".to_string()),
            },
        },
    )
    .unwrap();
    core.create_issue(
        "alice",
        "jeryu",
        "alice",
        CreateIssueRequest {
            title: "bug".to_string(),
            body: None,
            labels: Vec::new(),
            assignees: Vec::new(),
            milestone: None,
        },
    )
    .unwrap();
    let deliveries = core.list_webhook_deliveries("alice", "jeryu").unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].event, "issues");
    assert!(deliveries[0].signature_256.is_some());
}
