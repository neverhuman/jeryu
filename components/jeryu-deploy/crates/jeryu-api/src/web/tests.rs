mod pull_review_routes;

use super::*;
use crate::Method;
use crate::web::markdown::render_markdown;
use crate::web::repositories::repo_list_response;
use crate::web::surface::serialize_payload;
use crate::web::surface::{bootstrap_payload, map_method};
use crate::web::ws::{hello_message, requested_scopes, snapshot_event, unsubscribe_scopes};
use axum::extract::{Extension, Query};
use jeryu_agentbridge::driver::{AgentDriver, CollectingSink, CommandSpec, stage_editbot};
use jeryu_codegraph::{
    CrateDepRow, GraphSnapshot, SymbolRefRow, SymbolRow, ToolBuildScanConfig,
    scan_tool_build_clusters,
};
use jeryu_core::CheckConclusion;
use jeryu_core::{
    AccountStatus, AccountSummary, CommitStatusState, CreateCheckRunRequest,
    CreateCommitStatusRequest, CreatePullRequestRequest, CreateRepositoryRequest,
    CreateReviewRequest, RepoAccessLevel, ReviewState, SetBranchProtectionRequest, UserRole,
};
use jeryu_readmodel::contracts::{RepositoryRole, ServerWsMessage};
use jeryu_readmodel::{HealthLevel, sample_read_model};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::tempdir;

fn write_file(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture parent");
    }
    std::fs::write(path, contents).expect("write fixture file");
}

fn authenticated_account(login: &str) -> Extension<AccountSummary> {
    Extension(AccountSummary {
        login: login.to_string(),
        display_name: login.to_string(),
        role: UserRole::User,
        status: AccountStatus::Active,
        auth_epoch: 0,
        must_change_password: false,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    })
}

fn authenticated_admin_account(login: &str) -> Extension<AccountSummary> {
    Extension(AccountSummary {
        login: login.to_string(),
        display_name: login.to_string(),
        role: UserRole::Admin,
        status: AccountStatus::Active,
        auth_epoch: 0,
        must_change_password: false,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    })
}

/// Seed a repo + open PR + one failing check, build `WebState`, and assert
/// the model served by `/api/v1/bootstrap.tui` (i.e. `state.tui`) reflects the
/// seeded load: a populated `RepoActivity` with `failed_jobs == 1`, a non-empty
/// pool fabric, and Healthy system components — NOT the empty fixture.
#[tokio::test]
async fn bootstrap_tui_reflects_seeded_repo_pr_and_failing_check() {
    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    // An open PR so the repo counts as active work.
    core.create_pull_request(
        "alice",
        "jeryu",
        "alice",
        CreatePullRequestRequest {
            title: "feature".to_string(),
            head: "feature".to_string(),
            base: "main".to_string(),
            head_sha: Some("deadbeef".to_string()),
            ..CreatePullRequestRequest::default()
        },
    )
    .unwrap();
    // A completed check-run that FAILED — must surface as one failed job.
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "ci".to_string(),
            head_sha: "deadbeef".to_string(),
            status: Some(jeryu_core::CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Failure),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();

    let state = Arc::new(WebState::new(core));

    // The pool activity is genuinely populated, not the empty fixture.
    let activity = &state.tui.pool_activity;
    assert_eq!(activity.repos.len(), 1, "the seeded repo must be present");
    let repo = &activity.repos[0];
    assert_eq!(repo.repo, "alice/jeryu");
    assert_eq!(repo.failed_jobs, 1, "the failing check is one failed job");
    assert!(!activity.pools.is_empty(), "a default pool must roll up");
    assert_eq!(activity.pools[0].pool, "default");
    assert_eq!(activity.pools[0].failed_jobs, 1);

    // System health is Healthy (core is open), never the Unknown fixture.
    assert!(matches!(state.tui.system.scm.status, HealthLevel::Healthy));

    // The actual `/api/v1/bootstrap.tui` handler serves exactly this model.
    let served = bootstrap_tui(State(state.clone())).await.0;
    assert_eq!(served.pool_activity, *activity);
    assert_eq!(served.pool_activity.repos[0].failed_jobs, 1);
    assert!(served.workcells.items.is_empty());
    // Sanity: this is NOT the empty default model.
    assert_ne!(
        served.pool_activity,
        TuiReadModel::default().pool_activity,
        "bootstrap.tui must not serve an empty pool activity"
    );
}

#[tokio::test]
async fn workcell_repair_flow_holds_exports_and_releases() {
    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    // The export slice gate runs a real `git diff base..head`, so back the API
    // with a real bare repo. The head commit changes one in-slice file; the
    // lease is a whole-repo lease (workspace_root == repo_roots[0]), so the
    // slice permits it.
    let storage = tempfile::tempdir().expect("git storage dir");
    let workspace_root = storage.path().join("workspace").join("core").join("web");
    let (base_sha, head_sha) = build_bare_repo_with_diff(
        storage.path(),
        "alice",
        "jeryu",
        &[(
            ".github/workflows/ci.yml",
            "name: ci\non: [push, pull_request]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo ci\n",
        )],
        &[("crates/jeryu-core/repaired.rs", "// repaired\n")],
    );
    let state = Arc::new(WebState::new_with_git_storage(
        core,
        storage.path().to_path_buf(),
    ));
    state
        .core
        .create_check_run(
            "alice",
            "jeryu",
            CreateCheckRunRequest {
                name: "ci/export-fixture".to_string(),
                head_sha: head_sha.clone(),
                status: Some(jeryu_core::CheckRunStatus::Completed),
                conclusion: Some(CheckConclusion::Success),
                ..CreateCheckRunRequest::default()
            },
        )
        .expect("seed exported head check-run");
    let repair_body = serde_json::json!({
        "agent_id": "agent-wrath-17",
        "workspace_root": workspace_root,
        "repo_roots": [workspace_root],
        "branch_budget": 2,
        "runner_id": "xbabe0",
        "runner_epoch": 7,
        "git_status_summary": "rebase failed",
        "ci_snapshot_age_ms": 1200,
        "startup": {
            "state": "rebased",
            "main_ref": "origin/main",
            "base_sha": base_sha,
            "head_sha": head_sha,
        },
        "ci_run_id": "ci-parent-17",
        "failed_run_id": "run-17",
        "failed_receipt_id": "receipt-17",
        "failure_log_digest": "sha256:deadbeef"
    });

    let response = response_json(
        super::workcells::repair_live(
            State(state.clone()),
            axum::body::Bytes::from(serde_json::to_vec(&repair_body).unwrap()),
        )
        .await,
    )
    .await;

    let workcell_id = response["held"]["workcell_id"]
        .as_str()
        .expect("held workcell id");
    assert_eq!(response["held"]["state"], "held");
    assert_eq!(response["repairing"]["state"], "repairing");
    assert_eq!(
        response["held"]["frozen_snapshot"]["ci_run_id"],
        "ci-parent-17"
    );
    assert_eq!(
        response["held"]["frozen_snapshot"]["failed_run_id"],
        "run-17"
    );

    let export_body = serde_json::json!({
        "workcell_id": workcell_id,
        "runner_epoch": 7,
        "branch_suffix": "repair-17",
        "changed_files": ["crates/jeryu-core/repaired.rs"],
        "owner": "alice",
        "repo": "jeryu",
        "author": "agent-wrath-17",
        "title": "Repair failed tree",
        "body": "Repaired from failed tree"
    });
    let export = response_json(
        super::workcells::export_pr(
            State(state.clone()),
            AxumPath(workcell_id.to_string()),
            axum::http::HeaderMap::new(),
            axum::body::Bytes::from(serde_json::to_vec(&export_body).unwrap()),
        )
        .await,
    )
    .await;
    assert!(
        export["branch"]
            .as_str()
            .expect("branch")
            .starts_with("agents/agent-wrath-17/workcells/")
    );
    assert!(export["pull_request_number"].as_u64().unwrap() > 0);
    assert_eq!(export["target_branch"], "main");

    let pr = state
        .core
        .get_pull_request(
            "alice",
            "jeryu",
            export["pull_request_number"].as_u64().unwrap(),
        )
        .expect("pull request exists");
    assert_eq!(pr.head.ref_name, export["branch"]);
    assert_eq!(pr.base.ref_name, "main");
    assert_eq!(pr.changed_files, vec!["crates/jeryu-core/repaired.rs"]);

    let check_runs = state
        .core
        .list_check_runs("alice", "jeryu", Some(&head_sha))
        .expect("list check-runs for exported head");
    assert!(
        check_runs.total_count >= 1,
        "the exported PR head should have CI check-runs, got {check_runs:?}"
    );
    assert!(
        check_runs
            .check_runs
            .iter()
            .any(|run| run.conclusion == Some(CheckConclusion::Success)),
        "the exported PR head CI set should include a green run"
    );

    let release = response_json(
        super::workcells::release(
            State(state.clone()),
            AxumPath(workcell_id.to_string()),
            axum::body::Bytes::from(
                serde_json::to_vec(&serde_json::json!({
                    "runner_epoch": 7
                }))
                .unwrap(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(release["state"], "released");
}

#[tokio::test]
async fn codegraph_query_route_returns_impact_pack() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    let state = Arc::new(WebState::new(core));
    let snapshot = GraphSnapshot {
        symbols: vec![SymbolRow {
            crate_name: "jeryu-codegraph".to_string(),
            file: "crates/jeryu-codegraph/src/lib.rs".to_string(),
            symbol: "CodeGraph".to_string(),
            kind: "public".to_string(),
            is_public: true,
            line: 7,
        }],
        crate_deps: vec![CrateDepRow {
            crate_name: "jeryu-mcp".to_string(),
            depends_on: "jeryu-codegraph".to_string(),
        }],
        symbol_refs: vec![SymbolRefRow {
            crate_name: "jeryu-codegraph".to_string(),
            file: "crates/jeryu-codegraph/src/lib.rs".to_string(),
            symbol: "CodeGraph".to_string(),
            ref_file: "crates/jeryu-mcp/src/backend/memory.rs".to_string(),
            ref_line: 12,
            ref_kind: "type".to_string(),
        }],
        ..Default::default()
    };
    state.codegraph_store.persist(&snapshot).unwrap();

    let response = super::codegraph::query(
        State(state),
        AxumPath(repo.id.to_string()),
        axum::body::Bytes::from(
            serde_json::json!({
                "changed_paths": ["crates/jeryu-codegraph/src/lib.rs"],
                "symbol": "CodeGraph",
                "crate_name": "jeryu-codegraph"
            })
            .to_string(),
        ),
    )
    .await;
    let pack = response_json(response).await;
    assert_eq!(pack["schema_version"], "codegraph.query/v1");
    assert_eq!(pack["provenance"]["storage_schema"], "4");
    assert_eq!(pack["definition"]["symbol"], "CodeGraph");
    assert_eq!(
        pack["references"][0]["ref_file"],
        "crates/jeryu-mcp/src/backend/memory.rs"
    );
    assert_eq!(pack["reverse_deps"], serde_json::json!(["jeryu-mcp"]));
    assert!(
        pack["proof_lanes"][0]
            .as_str()
            .unwrap()
            .contains("jeryu-codegraph")
    );
}

#[tokio::test]
async fn codegraph_query_route_errors_are_typed() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    let state = Arc::new(WebState::new(core));

    let missing = response_json(
        super::codegraph::query(
            State(state.clone()),
            AxumPath("repo-missing".to_string()),
            axum::body::Bytes::from("{}"),
        )
        .await,
    )
    .await;
    assert_eq!(missing["code"], "not_found");
    assert_eq!(missing["purpose"], "query repository codegraph");
    assert!(
        missing["common_fixes"]
            .as_array()
            .expect("common fixes")
            .len()
            >= 2
    );

    let invalid = response_json(
        super::codegraph::query(
            State(state),
            AxumPath(repo.id.to_string()),
            axum::body::Bytes::from("{"),
        )
        .await,
    )
    .await;
    assert_eq!(invalid["code"], "codegraph_invalid_request");
    assert_eq!(invalid["docs_url"], "docs/errors.md#not-found");
    assert!(
        invalid["repair_hint"]
            .as_str()
            .expect("repair hint")
            .contains("codegraph API proof lane")
    );
}

#[tokio::test]
async fn tool_build_routes_return_clusters_and_record_feedback() {
    let core = ForgeCore::new();
    let state = Arc::new(WebState::new(core));
    let root = tempdir().expect("tool-build fixture");
    let repeated = r#"
pub fn alpha(input: &str) -> Result<String, String> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        let response = call_remote(input);
        if response.is_ok() {
            return response;
        }
        if attempts > 3 {
            return Err("failed".to_string());
        }
    }
}
"#;
    write_file(root.path(), "crates/a/src/lib.rs", repeated);
    write_file(
        root.path(),
        "crates/b/src/lib.rs",
        &repeated.replace("alpha", "beta"),
    );
    let report = scan_tool_build_clusters(
        root.path(),
        "alice/jeryu",
        "commit-a",
        ToolBuildScanConfig {
            window_lines: 5,
            min_normalized_tokens: 12,
            min_occurrences: 2,
            max_file_bytes: 64 * 1024,
            max_clusters: 10,
            min_repo_count: 1,
        },
    )
    .unwrap();
    assert!(!report.clusters.is_empty());
    state
        .codegraph_store
        .persist_tool_build_report(&report)
        .unwrap();

    let status = response_json(
        super::tool_build::status(
            State(state.clone()),
            Query(super::tool_build::ToolBuildQuery {
                repo: Some("alice/jeryu".to_string()),
                limit: None,
                include_ignored: false,
            }),
        )
        .await,
    )
    .await;
    assert_eq!(status["schema_version"], "codegraph.tool_build/v1");
    assert!(status["cluster_count"].as_u64().unwrap() > 0);

    let clusters = response_json(
        super::tool_build::clusters(
            State(state.clone()),
            Query(super::tool_build::ToolBuildQuery {
                repo: Some("alice/jeryu".to_string()),
                limit: Some(10),
                include_ignored: false,
            }),
        )
        .await,
    )
    .await;
    let cluster_id = clusters["clusters"][0]["cluster_id"]
        .as_str()
        .expect("cluster id")
        .to_string();
    assert!(
        clusters["clusters"][0]["insight"]
            .as_str()
            .unwrap()
            .contains("normalized window")
    );

    let invalid = response_json(
        super::tool_build::feedback(
            State(state.clone()),
            AxumPath(cluster_id.clone()),
            axum::body::Bytes::from(r#"{"reason":""}"#),
        )
        .await,
    )
    .await;
    assert_eq!(invalid["code"], "tool_build_feedback_reason_required");
    assert_eq!(invalid["docs_url"], "docs/codegraph-tool-build.md");

    let feedback = response_json(
        super::tool_build::feedback(
            State(state.clone()),
            AxumPath(cluster_id.clone()),
            axum::body::Bytes::from(r#"{"reason":"fixture boilerplate","ignored_by":"test"}"#),
        )
        .await,
    )
    .await;
    assert_eq!(feedback["cluster_id"], cluster_id);
    assert_eq!(feedback["reason"], "fixture boilerplate");

    let suppressed = response_json(
        super::tool_build::clusters(
            State(state),
            Query(super::tool_build::ToolBuildQuery {
                repo: Some("alice/jeryu".to_string()),
                limit: Some(10),
                include_ignored: false,
            }),
        )
        .await,
    )
    .await;
    assert!(
        suppressed["clusters"]
            .as_array()
            .unwrap()
            .iter()
            .all(|cluster| cluster["cluster_id"] != cluster_id)
    );
}

#[tokio::test]
async fn pulls_routes_return_live_pr_detail_diff_checks_and_threads() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    let pr = core
        .create_pull_request(
            "alice",
            "jeryu",
            "alice",
            CreatePullRequestRequest {
                title: "feature".to_string(),
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some("head-a".to_string()),
                base_sha: Some("base-a".to_string()),
                changed_files: vec!["crates/jeryu-api/src/web.rs".to_string()],
                ..CreatePullRequestRequest::default()
            },
        )
        .unwrap();
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "ci/fast".to_string(),
            head_sha: "head-a".to_string(),
            status: Some(jeryu_core::CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Success),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    core.create_review(
        "alice",
        "jeryu",
        pr.number,
        "alice",
        jeryu_core::CreateReviewRequest {
            body: None,
            event: jeryu_core::ReviewState::Commented,
            comments: vec![jeryu_core::ReviewCommentInput {
                path: "crates/jeryu-api/src/web.rs".to_string(),
                line: Some(12),
                body: "check this".to_string(),
            }],
            expected_head_sha: Some("head-a".to_string()),
        },
    )
    .unwrap();
    let state = Arc::new(WebState::new(core));

    let list = response_json(
        super::pulls::list(
            State(state.clone()),
            AxumPath(repo.id.to_string()),
            Query(super::pulls::PullListQuery { state: None }),
        )
        .await,
    )
    .await;
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["title"], "feature");
    assert_eq!(list["items"][0]["repo"]["owner"], "alice");

    let detail = response_json(
        super::pulls::detail(
            State(state.clone()),
            authenticated_account("alice"),
            AxumPath((repo.id.to_string(), pr.number)),
        )
        .await,
    )
    .await;
    assert_eq!(detail["summary"]["head_sha"], "head-a");
    assert_eq!(detail["head_tree_sha"], serde_json::Value::Null);
    assert_eq!(detail["base_tree_sha"], serde_json::Value::Null);
    assert_eq!(detail["reviews"][0]["head_sha"], "head-a");
    assert_eq!(detail["reviews"][0]["effective"], false);
    assert_eq!(
        detail["reviews"][0]["dismissed_review_id"],
        serde_json::Value::Null
    );
    assert_eq!(detail["reviews"][0]["stale"], false);
    assert_eq!(
        detail["summary"]["review"]["user_review_state"],
        serde_json::Value::Null
    );
    assert!(
        detail["passport_hash"]
            .as_str()
            .unwrap()
            .starts_with("passport:")
    );

    let diff = response_json(
        super::pulls::diff(
            State(state.clone()),
            AxumPath((repo.id.to_string(), pr.number)),
        )
        .await,
    )
    .await;
    assert_eq!(diff["files"][0]["path"], "crates/jeryu-api/src/web.rs");
    assert_eq!(diff["files"][0]["hunks"].as_array().unwrap().len(), 0);
    assert_eq!(diff["truncated"], false);

    let checks = response_json(
        super::pulls::checks(
            State(state.clone()),
            AxumPath((repo.id.to_string(), pr.number)),
        )
        .await,
    )
    .await;
    assert_eq!(checks["passing"], 1);
    assert_eq!(checks["checks"][0]["status"], "success");

    let threads = response_json(
        super::pulls::threads(
            State(state.clone()),
            AxumPath((repo.id.to_string(), pr.number)),
        )
        .await,
    )
    .await;
    assert_eq!(
        threads["threads"][0]["file_path"],
        "crates/jeryu-api/src/web.rs"
    );
    assert_eq!(
        threads["threads"][0]["comments"][0]["body_markdown"],
        "check this"
    );
}

#[tokio::test]
async fn pull_detail_marks_review_history_stale_and_uses_latest_current_head_verdict() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "review-heads".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    core.set_branch_protection(
        "alice",
        "review-heads",
        "main",
        SetBranchProtectionRequest {
            required_approving_review_count: 1,
            ..SetBranchProtectionRequest::default()
        },
    )
    .unwrap();
    let head_a = "a".repeat(40);
    let head_b = "b".repeat(40);
    let pr = core
        .create_pull_request(
            "alice",
            "review-heads",
            "alice",
            CreatePullRequestRequest {
                title: "head-bound review readback".to_string(),
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some(head_a.clone()),
                base_sha: Some("c".repeat(40)),
                ..CreatePullRequestRequest::default()
            },
        )
        .unwrap();
    core.create_review(
        "alice",
        "review-heads",
        pr.number,
        "bob",
        CreateReviewRequest {
            body: Some("approved old head".to_string()),
            event: ReviewState::Approved,
            comments: Vec::new(),
            expected_head_sha: Some(head_a.clone()),
        },
    )
    .unwrap();
    core.refresh_pull_request_heads_for_ref("alice", "review-heads", "feature", &head_b)
        .unwrap();
    let state = Arc::new(WebState::new(core.clone()));
    let path = || AxumPath((repo.id.to_string(), pr.number));

    let moved = response_json(
        super::pulls::detail(State(state.clone()), authenticated_account("bob"), path()).await,
    )
    .await;
    assert_eq!(moved["summary"]["review"]["approvals"], 0);
    assert_eq!(
        moved["summary"]["review"]["user_review_state"],
        serde_json::Value::Null
    );
    assert_eq!(moved["reviews"][0]["head_sha"], head_a);
    assert_eq!(moved["reviews"][0]["effective"], false);
    assert_eq!(moved["reviews"][0]["stale"], true);

    core.create_review(
        "alice",
        "review-heads",
        pr.number,
        "bob",
        CreateReviewRequest {
            body: Some("fix the current head".to_string()),
            event: ReviewState::ChangesRequested,
            comments: Vec::new(),
            expected_head_sha: Some(head_b.clone()),
        },
    )
    .unwrap();
    let requested = response_json(
        super::pulls::detail(State(state.clone()), authenticated_account("bob"), path()).await,
    )
    .await;
    assert_eq!(requested["summary"]["review"]["changes_requested"], 1);
    assert_eq!(
        requested["summary"]["review"]["user_review_state"],
        "CHANGES_REQUESTED"
    );
    assert!(
        requested["merge_passport"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|blocker| blocker["code"] == "passport_blocked_changes_requested")
    );

    core.create_review(
        "alice",
        "review-heads",
        pr.number,
        "bob",
        CreateReviewRequest {
            body: Some("current head repaired".to_string()),
            event: ReviewState::Approved,
            comments: Vec::new(),
            expected_head_sha: Some(head_b.clone()),
        },
    )
    .unwrap();
    let approved = response_json(
        super::pulls::detail(State(state), authenticated_account("bob"), path()).await,
    )
    .await;
    assert_eq!(approved["summary"]["review"]["approvals"], 1);
    assert_eq!(approved["summary"]["review"]["changes_requested"], 0);
    assert_eq!(
        approved["summary"]["review"]["user_review_state"],
        "APPROVED"
    );
    assert_eq!(approved["reviews"].as_array().unwrap().len(), 3);
    assert_eq!(approved["reviews"][1]["state"], "CHANGES_REQUESTED");
    assert_eq!(approved["reviews"][1]["effective"], false);
    assert_eq!(approved["reviews"][1]["stale"], false);
    assert_eq!(approved["reviews"][2]["state"], "APPROVED");
    assert_eq!(approved["reviews"][2]["effective"], true);
    assert_eq!(approved["reviews"][2]["stale"], false);
    assert_eq!(approved["merge_passport"]["status"], "pass");
}

#[tokio::test]
async fn pulls_mutations_return_typed_repair_errors() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
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
            ..SetBranchProtectionRequest::default()
        },
    )
    .unwrap();
    let pr = core
        .create_pull_request(
            "alice",
            "jeryu",
            "alice",
            CreatePullRequestRequest {
                title: "blocked".to_string(),
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some("head-b".to_string()),
                ..CreatePullRequestRequest::default()
            },
        )
        .unwrap();
    let state = Arc::new(WebState::new(core));

    let missing_repo = super::pulls::list(
        State(state.clone()),
        AxumPath("repo-missing".to_string()),
        Query(super::pulls::PullListQuery { state: None }),
    )
    .await;
    assert_eq!(missing_repo.status(), StatusCode::NOT_FOUND);
    let missing_repo_body = response_json(missing_repo).await;
    assert_eq!(missing_repo_body["code"], "not_found");
    for key in [
        "purpose",
        "reason",
        "common_fixes",
        "docs_url",
        "repair_hint",
    ] {
        assert!(missing_repo_body.get(key).is_some(), "missing {key}");
    }

    let missing_pr = super::pulls::detail(
        State(state.clone()),
        authenticated_account("bob"),
        AxumPath((repo.id.to_string(), 404)),
    )
    .await;
    assert_eq!(missing_pr.status(), StatusCode::NOT_FOUND);
    assert_eq!(response_json(missing_pr).await["code"], "not_found");

    let invalid_review = super::pulls::review(
        State(state.clone()),
        authenticated_account("bob"),
        AxumPath((repo.id.to_string(), pr.number)),
        axum::body::Bytes::from("{"),
    )
    .await;
    assert_eq!(invalid_review.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response_json(invalid_review).await["code"],
        "pull_review_invalid_request"
    );

    let stale = super::pulls::approve(
        State(state.clone()),
        authenticated_account("bob"),
        AxumPath((repo.id.to_string(), pr.number)),
        axum::body::Bytes::from(r#"{"expected_head_sha":"old"}"#),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    let stale_body = response_json(stale).await;
    assert_eq!(stale_body["error"]["code"], "merge_sha_stale");
    assert_eq!(stale_body["error"]["details"]["current_head_sha"], "head-b");

    let detail = response_json(
        super::pulls::detail(
            State(state.clone()),
            authenticated_account("bob"),
            AxumPath((repo.id.to_string(), pr.number)),
        )
        .await,
    )
    .await;
    assert_eq!(
        detail["merge_passport"]["blockers"][0]["code"],
        "passport_blocked_checks_missing"
    );
    assert!(
        detail["merge_passport"]["blockers"][0]["details"]
            .as_str()
            .unwrap()
            .contains("ci/fast")
    );
    let merge = super::pulls::merge(
        State(state),
        AxumPath((repo.id.to_string(), pr.number)),
        axum::body::Bytes::from(
            serde_json::json!({
                "expected_head_sha": "head-b",
                "expected_passport_hash": detail["passport_hash"].as_str().unwrap(),
                "merge_method": "merge"
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(merge.status(), StatusCode::CONFLICT);
    let merge_body = response_json(merge).await;
    assert_eq!(merge_body["code"], "merge_blocked");
    assert_eq!(merge_body["purpose"], "merge pull request");
    assert_eq!(
        merge_body["error"]["details"]["passport_status"],
        serde_json::json!("blocked")
    );
}

async fn passport_for_required_check(
    status: Option<jeryu_core::CheckRunStatus>,
    conclusion: Option<CheckConclusion>,
) -> serde_json::Value {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "required-context".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    core.set_branch_protection(
        "alice",
        "required-context",
        "main",
        SetBranchProtectionRequest {
            required_status_checks: vec!["ci/required".to_string()],
            required_approving_review_count: 1,
            ..SetBranchProtectionRequest::default()
        },
    )
    .unwrap();
    let pr = core
        .create_pull_request(
            "alice",
            "required-context",
            "alice",
            CreatePullRequestRequest {
                title: "required context posture".to_string(),
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some("required-context-head".to_string()),
                ..CreatePullRequestRequest::default()
            },
        )
        .unwrap();
    core.create_review(
        "alice",
        "required-context",
        pr.number,
        "bob",
        CreateReviewRequest {
            body: None,
            event: ReviewState::Approved,
            comments: Vec::new(),
            expected_head_sha: Some("required-context-head".to_string()),
        },
    )
    .unwrap();
    if status.is_some() || conclusion.is_some() {
        core.create_check_run(
            "alice",
            "required-context",
            CreateCheckRunRequest {
                name: "ci/required".to_string(),
                head_sha: "required-context-head".to_string(),
                status,
                conclusion,
                details_url: Some("https://forge.invalid/checks/required".to_string()),
                ..CreateCheckRunRequest::default()
            },
        )
        .unwrap();
    }
    let state = Arc::new(WebState::new(core));
    response_json(
        super::pulls::detail(
            State(state),
            authenticated_account("bob"),
            AxumPath((repo.id.to_string(), pr.number)),
        )
        .await,
    )
    .await
}

#[tokio::test]
async fn pulls_passport_reports_missing_required_context() {
    let detail = passport_for_required_check(None, None).await;
    assert_eq!(detail["merge_passport"]["status"], "blocked");
    assert_eq!(
        detail["merge_passport"]["blockers"][0]["code"],
        "passport_blocked_checks_missing"
    );
    assert!(
        detail["merge_passport"]["blockers"][0]["details"]
            .as_str()
            .unwrap()
            .contains("ci/required")
    );
}

#[tokio::test]
async fn pulls_passport_reports_failing_required_context() {
    let detail = passport_for_required_check(
        Some(jeryu_core::CheckRunStatus::Completed),
        Some(CheckConclusion::Failure),
    )
    .await;
    assert_eq!(detail["merge_passport"]["status"], "blocked");
    assert_eq!(
        detail["merge_passport"]["blockers"][0]["code"],
        "passport_blocked_checks"
    );
    assert_eq!(
        detail["merge_passport"]["blockers"][0]["details"],
        "https://forge.invalid/checks/required"
    );
}

#[tokio::test]
async fn pulls_passport_reports_pending_required_context() {
    let detail =
        passport_for_required_check(Some(jeryu_core::CheckRunStatus::InProgress), None).await;
    assert_eq!(detail["merge_passport"]["status"], "blocked");
    assert_eq!(
        detail["merge_passport"]["blockers"][0]["code"],
        "passport_blocked_pending_checks"
    );
    assert!(
        detail["summary"]["mergeable"]["reason"]
            .as_str()
            .unwrap()
            .contains("ci/required")
    );
}

async fn passport_for_intrinsic_proof(
    status: Option<jeryu_core::CheckRunStatus>,
    conclusion: Option<CheckConclusion>,
) -> serde_json::Value {
    let core = ForgeCore::new();
    let _repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "intrinsic-proof".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    let pr = core
        .create_pull_request(
            "alice",
            "intrinsic-proof",
            "alice",
            CreatePullRequestRequest {
                title: "intrinsic proof posture".to_string(),
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some("intrinsic-proof-head".to_string()),
                ..CreatePullRequestRequest::default()
            },
        )
        .unwrap();
    if status.is_some() || conclusion.is_some() {
        core.create_check_run(
            "alice",
            "intrinsic-proof",
            CreateCheckRunRequest {
                name: "jankurai/proof".to_string(),
                head_sha: "intrinsic-proof-head".to_string(),
                status,
                conclusion,
                ..CreateCheckRunRequest::default()
            },
        )
        .unwrap();
    }
    let state = WebState::new(core);
    let current = state
        .github
        .core()
        .get_pull_request("alice", "intrinsic-proof", pr.number)
        .unwrap();
    let detail = super::pulls::detail_for_pr_with_audit_enforcement(&state, &current, true);
    serde_json::to_value(detail).unwrap()
}

#[test]
fn pulls_audit_enforcement_spellings_match_protected_core() {
    for value in ["1", "true", "yes", "on", " true "] {
        assert!(super::pulls::audit_merge_enforced_value(Some(value)));
    }
    for value in [None, Some(""), Some("0"), Some("false"), Some("TRUE")] {
        assert!(!super::pulls::audit_merge_enforced_value(value));
    }
}

#[tokio::test]
async fn pulls_intrinsic_proof_reports_missing_context() {
    let detail = passport_for_intrinsic_proof(None, None).await;
    assert_eq!(detail["merge_passport"]["status"], "blocked");
    assert_eq!(
        detail["merge_passport"]["blockers"][0]["code"],
        "passport_blocked_checks_missing"
    );
}

#[tokio::test]
async fn pulls_intrinsic_proof_reports_failing_context() {
    let detail = passport_for_intrinsic_proof(
        Some(jeryu_core::CheckRunStatus::Completed),
        Some(CheckConclusion::Failure),
    )
    .await;
    assert_eq!(detail["merge_passport"]["status"], "blocked");
    assert_eq!(
        detail["merge_passport"]["blockers"][0]["code"],
        "passport_blocked_checks"
    );
}

#[tokio::test]
async fn pulls_intrinsic_proof_accepts_passing_context() {
    let detail = passport_for_intrinsic_proof(
        Some(jeryu_core::CheckRunStatus::Completed),
        Some(CheckConclusion::Success),
    )
    .await;
    assert_eq!(detail["merge_passport"]["status"], "pass");
    assert_eq!(detail["merge_passport"]["blockers"], serde_json::json!([]));
}

#[tokio::test]
async fn pulls_mutations_submit_review_and_comment() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    let pr = core
        .create_pull_request(
            "alice",
            "jeryu",
            "alice",
            CreatePullRequestRequest {
                title: "ready".to_string(),
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some("head-ready".to_string()),
                base_sha: Some("base-ready".to_string()),
                changed_files: vec!["src/lib.rs".to_string()],
                ..CreatePullRequestRequest::default()
            },
        )
        .unwrap();
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "ci/fast".to_string(),
            head_sha: "head-ready".to_string(),
            status: Some(jeryu_core::CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Success),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    let state = Arc::new(WebState::new(core));
    let path = || AxumPath((repo.id.to_string(), pr.number));

    let review = response_json(
        super::pulls::review(
            State(state.clone()),
            authenticated_account("bob"),
            path(),
            axum::body::Bytes::from(
                serde_json::json!({
                    "verdict": "comment",
                    "expected_head_sha": "head-ready",
                    "body_markdown": "reviewed",
                    "thread_comments": [{
                        "thread_id": null,
                        "body_markdown": "nit",
                        "file_path": "src/lib.rs",
                        "line": 7,
                        "anchor_sha": "head-ready"
                    }],
                    "evidence": null
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(review["summary"]["review"]["unresolved_threads"], 1);

    let comment = response_json(
        super::pulls::comment(
            State(state.clone()),
            authenticated_account("carol"),
            path(),
            axum::body::Bytes::from(
                serde_json::json!({
                    "thread_id": null,
                    "body_markdown": "follow-up",
                    "file_path": "src/lib.rs",
                    "line": 8,
                    "anchor_sha": "head-ready"
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    assert!(
        comment["threads"]
            .as_array()
            .unwrap()
            .iter()
            .any(|thread| thread["comments"][0]["body_markdown"] == "follow-up")
    );
    let reviews = state
        .github
        .core()
        .list_reviews("alice", "jeryu", pr.number)
        .unwrap();
    assert!(
        reviews
            .iter()
            .any(|review| review.author == "bob" && review.state == ReviewState::Commented)
    );
    assert!(
        reviews
            .iter()
            .any(|review| review.author == "carol" && review.state == ReviewState::Commented)
    );

    let self_review = super::pulls::review(
        State(state.clone()),
        authenticated_account("alice"),
        path(),
        axum::body::Bytes::from(
            serde_json::json!({
                "verdict": "approve",
                "expected_head_sha": "head-ready",
                "body_markdown": "self approval",
                "thread_comments": [],
                "evidence": null
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(self_review.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response_json(self_review).await["code"],
        "pull_self_approval_forbidden"
    );

    let self_approve = super::pulls::approve(
        State(state.clone()),
        authenticated_account("alice"),
        path(),
        axum::body::Bytes::from(r#"{"expected_head_sha":"head-ready"}"#),
    )
    .await;
    assert_eq!(self_approve.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response_json(self_approve).await["code"],
        "pull_self_approval_forbidden"
    );
    assert_eq!(
        state
            .github
            .core()
            .list_reviews("alice", "jeryu", pr.number)
            .unwrap()
            .iter()
            .filter(|review| review.state == ReviewState::Approved)
            .count(),
        0
    );
}

#[tokio::test]
async fn pulls_mutations_allow_record_only_autonomy_advisory() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    let storage = tempfile::tempdir().expect("git storage dir");
    let (base_sha, head_sha) = build_bare_repo_with_main_and_feature(
        storage.path(),
        "alice",
        "jeryu",
        "src/merge.rs",
        "pub fn merge_ready() -> bool { true }\n",
    );
    let pr = core
        .create_pull_request(
            "alice",
            "jeryu",
            "alice",
            CreatePullRequestRequest {
                title: "merge-ready".to_string(),
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some(head_sha.clone()),
                base_sha: Some(base_sha),
                changed_files: vec!["src/merge.rs".to_string()],
                ..CreatePullRequestRequest::default()
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
            ..SetBranchProtectionRequest::default()
        },
    )
    .unwrap();
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "ci/fast".to_string(),
            head_sha: head_sha.clone(),
            status: Some(jeryu_core::CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Success),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "jeryu/autonomy".to_string(),
            head_sha: head_sha.clone(),
            status: Some(jeryu_core::CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::ActionRequired),
            details_url: Some("https://forge.invalid/advisory".to_string()),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    let state = Arc::new(WebState::new_with_git_storage(
        core,
        storage.path().to_path_buf(),
    ));
    let path = || AxumPath((repo.id.to_string(), pr.number));

    let approved = response_json(
        super::pulls::approve(
            State(state.clone()),
            authenticated_account("bob"),
            path(),
            axum::body::Bytes::from(
                serde_json::json!({
                    "expected_head_sha": head_sha.clone(),
                    "body_markdown": "ship it"
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(approved["summary"]["review"]["approvals"], 1);
    let reviews = state
        .github
        .core()
        .list_reviews("alice", "jeryu", pr.number)
        .unwrap();
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].author, "bob");

    let detail = response_json(
        super::pulls::detail(State(state.clone()), authenticated_account("bob"), path()).await,
    )
    .await;
    assert_eq!(detail["summary"]["checks"]["total"], 2);
    assert_eq!(detail["summary"]["checks"]["failing"], 1);
    assert_eq!(detail["merge_passport"]["status"], "pass");
    assert_eq!(detail["merge_passport"]["blockers"], serde_json::json!([]));
    let head_tree = detail["head_tree_sha"]
        .as_str()
        .expect("real head commit must expose its tree");
    let base_tree = detail["base_tree_sha"]
        .as_str()
        .expect("real base commit must expose its tree");
    assert_eq!(head_tree.len(), 40);
    assert_eq!(base_tree.len(), 40);
    assert_ne!(head_tree, base_tree);
    let passport_hash = detail["passport_hash"].as_str().unwrap();

    state
        .github
        .core()
        .create_check_run(
            "alice",
            "jeryu",
            CreateCheckRunRequest {
                name: "jeryu/autonomy".to_string(),
                head_sha: head_sha.clone(),
                status: Some(jeryu_core::CheckRunStatus::Completed),
                conclusion: Some(CheckConclusion::Neutral),
                details_url: Some("https://forge.invalid/advisory/newer".to_string()),
                ..CreateCheckRunRequest::default()
            },
        )
        .unwrap();
    let advisory_refresh = response_json(
        super::pulls::detail(State(state.clone()), authenticated_account("bob"), path()).await,
    )
    .await;
    assert_eq!(advisory_refresh["merge_passport"]["status"], "pass");
    assert_eq!(advisory_refresh["passport_hash"], passport_hash);

    state
        .github
        .core()
        .create_commit_status(
            "alice",
            "jeryu",
            &head_sha,
            "ci",
            CreateCommitStatusRequest {
                state: CommitStatusState::Failure,
                context: "ci/fast".to_string(),
                description: Some("required lane regressed".to_string()),
                target_url: None,
            },
        )
        .unwrap();
    let merge = super::pulls::merge(
        State(state),
        path(),
        axum::body::Bytes::from(
            serde_json::json!({
                "expected_head_sha": head_sha,
                "expected_passport_hash": passport_hash,
                "merge_method": "merge"
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(merge.status(), StatusCode::CONFLICT);
    assert_eq!(response_json(merge).await["code"], "merge_passport_stale");
}

#[tokio::test]
async fn web_pull_merge_advances_real_bare_main_ref() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    let storage = tempfile::tempdir().expect("git storage dir");
    let (base_sha, head_sha) = build_bare_repo_with_main_and_feature(
        storage.path(),
        "alice",
        "jeryu",
        "src/web_merge.rs",
        "pub fn merged() -> bool { true }\n",
    );
    let pr = core
        .create_pull_request(
            "alice",
            "jeryu",
            "alice",
            CreatePullRequestRequest {
                title: "real merge".to_string(),
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some(head_sha.clone()),
                base_sha: Some(base_sha.clone()),
                changed_files: vec!["src/web_merge.rs".to_string()],
                ..CreatePullRequestRequest::default()
            },
        )
        .unwrap();
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "ci/fast".to_string(),
            head_sha: head_sha.clone(),
            status: Some(jeryu_core::CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Success),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    core.create_review(
        "alice",
        "jeryu",
        pr.number,
        "bob",
        CreateReviewRequest {
            body: None,
            event: ReviewState::Approved,
            comments: Vec::new(),
            expected_head_sha: Some(head_sha.clone()),
        },
    )
    .unwrap();
    let state = Arc::new(WebState::new_with_git_storage(
        core,
        storage.path().to_path_buf(),
    ));
    let path = || AxumPath((repo.id.to_string(), pr.number));
    let detail = response_json(
        super::pulls::detail(State(state.clone()), authenticated_account("bob"), path()).await,
    )
    .await;
    assert_eq!(detail["merge_passport"]["status"], "pass");
    let passport_hash = detail["passport_hash"].as_str().unwrap();

    let response = super::pulls::merge(
        State(state),
        path(),
        axum::body::Bytes::from(
            serde_json::json!({
                "expected_head_sha": head_sha.clone(),
                "expected_passport_hash": passport_hash,
                "merge_method": "merge"
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let merged = response_json(response).await;

    assert_eq!(merged["summary"]["state"], "merged");
    assert_eq!(
        bare_ref(storage.path(), "alice", "jeryu", "refs/heads/main"),
        head_sha,
        "web merge route must move the real bare main ref"
    );
}

#[tokio::test]
async fn mounted_pulls_routes_are_reachable() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    core.create_pull_request(
        "alice",
        "jeryu",
        "alice",
        CreatePullRequestRequest {
            title: "mounted".to_string(),
            head: "feature".to_string(),
            base: "main".to_string(),
            head_sha: Some("head-c".to_string()),
            ..CreatePullRequestRequest::default()
        },
    )
    .unwrap();
    let response = app(
        WebState::new(core),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    )
    .oneshot(
        Request::builder()
            .uri(format!("/api/v1/repos/{}/pulls", repo.id))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["items"][0]["title"], "mounted");
}

#[tokio::test]
async fn web_pull_passport_uses_latest_run_per_check_name_within_current_head() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    let head_sha = "current-head".to_string();
    let pr = core
        .create_pull_request(
            "alice",
            "jeryu",
            "alice",
            CreatePullRequestRequest {
                title: "latest proof wins".to_string(),
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some(head_sha.clone()),
                ..CreatePullRequestRequest::default()
            },
        )
        .unwrap();
    for conclusion in [CheckConclusion::Failure, CheckConclusion::Success] {
        core.create_check_run(
            "alice",
            "jeryu",
            CreateCheckRunRequest {
                name: "jankurai/proof".to_string(),
                head_sha: head_sha.clone(),
                status: Some(jeryu_core::CheckRunStatus::Completed),
                conclusion: Some(conclusion),
                ..CreateCheckRunRequest::default()
            },
        )
        .unwrap();
    }
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "jankurai/proof".to_string(),
            head_sha: "different-head".to_string(),
            status: Some(jeryu_core::CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Failure),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    core.create_review(
        "alice",
        "jeryu",
        pr.number,
        "bob",
        CreateReviewRequest {
            body: None,
            event: ReviewState::Approved,
            comments: Vec::new(),
            expected_head_sha: Some(head_sha.clone()),
        },
    )
    .unwrap();
    assert_eq!(
        core.list_check_runs("alice", "jeryu", Some(&head_sha))
            .unwrap()
            .total_count,
        2,
        "historical check-runs remain in forge history"
    );
    assert_eq!(
        core.list_check_runs("alice", "jeryu", None)
            .unwrap()
            .total_count,
        3,
        "another head's newer failure remains historical but cannot affect this passport"
    );
    let state = Arc::new(WebState::new(core));
    let detail = response_json(
        super::pulls::detail(
            State(state),
            authenticated_account("bob"),
            AxumPath((repo.id.to_string(), pr.number)),
        )
        .await,
    )
    .await;

    assert_eq!(detail["summary"]["checks"]["total"], 1);
    assert_eq!(detail["summary"]["checks"]["passing"], 1);
    assert_eq!(detail["summary"]["checks"]["failing"], 0);
    assert_eq!(detail["merge_passport"]["status"], "pass");
}

#[tokio::test]
async fn control_plane_status_priorities_and_absence_states_are_live() {
    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    core.create_pull_request(
        "alice",
        "jeryu",
        "alice",
        CreatePullRequestRequest {
            title: "feature".to_string(),
            head: "feature".to_string(),
            base: "main".to_string(),
            head_sha: Some("head-without-checks".to_string()),
            draft: true,
            ..CreatePullRequestRequest::default()
        },
    )
    .unwrap();
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "ci/fast".to_string(),
            head_sha: "other-head".to_string(),
            status: Some(jeryu_core::CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Failure),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    let state = Arc::new(WebState::new(core));

    let status = super::control_plane::status(State(state.clone())).await.0;
    assert_eq!(status.schema_version, "jeryu.control_plane/v1");
    assert_eq!(status.summary.repo_count, 1);
    assert_eq!(status.summary.draft_pr_count, 1);
    assert_eq!(status.summary.failing_check_count, 1);
    assert_eq!(
        serde_json::to_value(&status.summary).unwrap()["mirrorState"],
        "missing"
    );
    assert!(
        status
            .priorities
            .iter()
            .any(|priority| priority.id.contains("checks-missing"))
    );
    assert!(!status.artifacts.absence_is_success);

    let priorities = super::control_plane::priorities(
        State(state.clone()),
        Query(super::control_plane::PriorityQuery { limit: Some(1) }),
    )
    .await
    .0;
    assert_eq!(priorities.len(), 1);
    assert_eq!(priorities[0].rules_version, "rules-v1");

    let artifacts = super::control_plane::artifacts_latest(State(state.clone()))
        .await
        .0;
    assert_eq!(
        artifacts.state,
        super::control_plane::EvidenceState::Missing
    );
    assert!(!artifacts.absence_is_success);

    let runners = super::control_plane::runners(State(state.clone())).await.0;
    assert_eq!(
        runners.local.state,
        super::control_plane::EvidenceState::Fresh
    );
    assert_eq!(
        runners.mirror.state,
        super::control_plane::EvidenceState::Missing
    );

    let graph = super::control_plane::repo_graph(
        State(state),
        Query(super::control_plane::RepoGraphQuery {
            repo: None,
            cluster_kind: Some("ci_blocker".to_string()),
            query: None,
            limit: None,
        }),
    )
    .await
    .0;
    assert_eq!(graph.schema_version, "jeryu.repo_graph/v1");
    assert!(
        graph
            .clusters
            .iter()
            .all(|cluster| cluster.kind == "ci_blocker")
    );
}

#[tokio::test]
async fn control_plane_agent_runs_list_route_starts_empty() {
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let runs = super::agent_runs::list(State(state)).await.0;
    assert!(runs.is_empty());
}

fn write_exec_script(label: &str, contents: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "jeryu-r5-{label}-{}-{}.sh",
        std::process::id(),
        jeryu_runner_core::receipt::now_ms()
    ));
    std::fs::write(&path, contents).expect("write staging script");
    #[cfg(unix)]
    {
        let mut perms = std::fs::metadata(&path)
            .expect("read script metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("mark script executable");
    }
    path
}

fn run_or_skip(
    driver: &AgentDriver,
    workspace: &std::path::Path,
    spec: &CommandSpec,
    sink: &CollectingSink,
) -> Option<jeryu_agentbridge::driver::AgentRunResult> {
    match driver.run(workspace, spec, sink) {
        Ok(result) => Some(result),
        Err(jeryu_agentbridge::driver::DriverError::SandboxUnavailable(reason)) => {
            eprintln!("SKIP: sandbox unavailable (cannot fail closed): {reason}");
            None
        }
        Err(other) => panic!("driver run failed unexpectedly: {other}"),
    }
}

async fn claim_run_workcell(
    state: Arc<WebState>,
    workspace_root: &std::path::Path,
    repo_roots: Vec<std::path::PathBuf>,
    runner_epoch: u64,
) -> String {
    let claim_body = serde_json::json!({
        "agent_id": format!("agent-wrath-run-{runner_epoch}"),
        "workspace_root": workspace_root,
        "repo_roots": repo_roots,
        "branch_budget": 1,
        "runner_id": format!("xbabe-run-{runner_epoch}"),
        "runner_epoch": runner_epoch,
        "git_status_summary": "clean",
        "ci_snapshot_age_ms": 0,
        "startup": {
            "state": "rebased",
            "main_ref": "origin/main",
            "base_sha": "base",
            "head_sha": "head"
        }
    });
    let lease = response_json(
        super::workcells::claim(
            State(state),
            axum::body::Bytes::from(serde_json::to_vec(&claim_body).unwrap()),
        )
        .await,
    )
    .await;
    lease["workcell_id"]
        .as_str()
        .expect("claimed workcell id")
        .to_string()
}

async fn run_agent_json(
    state: Arc<WebState>,
    path_workcell_id: &str,
    body: serde_json::Value,
) -> Value {
    response_json(
        super::workcells::run_agent(
            State(state),
            AxumPath(path_workcell_id.to_string()),
            axum::body::Bytes::from(serde_json::to_vec(&body).unwrap()),
        )
        .await,
    )
    .await
}

fn run_agent_test_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[tokio::test]
async fn workcell_run_agent_stays_in_claimed_repo_root_and_reports_events() {
    let _guard = run_agent_test_lock().lock().await;
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let workspace = tempdir().expect("create workcell workspace");
    let repo_root = workspace.path().join("repo-slice");
    std::fs::create_dir_all(&repo_root).expect("create claimed repo root");
    let claim_body = serde_json::json!({
        "agent_id": "agent-wrath-run",
        "workspace_root": workspace.path(),
        "repo_roots": [repo_root],
        "branch_budget": 1,
        "runner_id": "xbabe-run",
        "runner_epoch": 23,
        "git_status_summary": "clean",
        "ci_snapshot_age_ms": 0,
        "startup": {
            "state": "rebased",
            "main_ref": "origin/main",
            "base_sha": "base",
            "head_sha": "head"
        }
    });
    let lease = response_json(
        super::workcells::claim(
            State(state.clone()),
            axum::body::Bytes::from(serde_json::to_vec(&claim_body).unwrap()),
        )
        .await,
    )
    .await;
    let workcell_id = lease["workcell_id"]
        .as_str()
        .expect("claimed workcell id")
        .to_string();

    let outside_script = write_exec_script(
        "run-outside",
        r#"#!/bin/sh
echo outside
"#,
    );
    let denied_body = serde_json::json!({
        "workcell_id": workcell_id,
        "runner_epoch": 23,
        "program": outside_script,
        "require_cgroup": false
    });
    let denied = response_json(
        super::workcells::run_agent(
            State(state.clone()),
            AxumPath(workcell_id.clone()),
            axum::body::Bytes::from(serde_json::to_vec(&denied_body).unwrap()),
        )
        .await,
    )
    .await;
    assert_eq!(denied["code"], "workcell_run_path_denied");

    let script_src = write_exec_script(
        "run-agent",
        r#"#!/bin/sh
echo agent-out
echo agent-err >&2
"#,
    );
    let staged = stage_editbot(&repo_root, &script_src).expect("stage run script in repo root");
    let run_body = serde_json::json!({
        "workcell_id": workcell_id,
        "runner_epoch": 23,
        "program": staged,
        "require_cgroup": false,
        "timeout_ms": 10000,
        "output_budget_bytes": 4096
    });
    let run = response_json(
        super::workcells::run_agent(
            State(state.clone()),
            AxumPath(workcell_id.clone()),
            axum::body::Bytes::from(serde_json::to_vec(&run_body).unwrap()),
        )
        .await,
    )
    .await;
    let _ = std::fs::remove_file(&outside_script);
    let _ = std::fs::remove_file(&script_src);
    let _ = workspace.close();
    if run["code"] == "workcell_run_sandbox_unavailable" {
        eprintln!("SKIP: sandbox unavailable for workcell run route");
        return;
    }

    assert_eq!(run["workcell_id"], workcell_id);
    assert_eq!(run["outcome"]["succeeded"], true);
    let events = run["events"].as_array().expect("structured run events");
    assert!(events.iter().any(|event| event["kind"] == "started"));
    assert!(events.iter().any(|event| event["kind"] == "finished"));
    assert!(events.iter().any(|event| {
        event["stream"] == "stdout" && event["text"].as_str().unwrap_or("").contains("agent-out")
    }));
    assert!(events.iter().any(|event| {
        event["stream"] == "stderr" && event["text"].as_str().unwrap_or("").contains("agent-err")
    }));
}

#[tokio::test]
async fn workcell_run_agent_rejects_identity_epoch_and_inactive_cell() {
    let _guard = run_agent_test_lock().lock().await;
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let workspace = tempdir().expect("create workcell workspace");
    let repo_root = workspace.path().join("repo-slice");
    std::fs::create_dir_all(&repo_root).expect("create claimed repo root");
    let workcell_id =
        claim_run_workcell(state.clone(), workspace.path(), vec![repo_root], 31).await;

    let invalid = response_json(
        super::workcells::run_agent(
            State(state.clone()),
            AxumPath(workcell_id.clone()),
            axum::body::Bytes::from_static(b"{ not json"),
        )
        .await,
    )
    .await;
    assert_eq!(invalid["code"], "workcell_invalid_request");

    let mismatch = run_agent_json(
        state.clone(),
        "different-cell",
        serde_json::json!({
            "workcell_id": workcell_id,
            "runner_epoch": 31,
            "program": "/bin/sh",
            "require_cgroup": false
        }),
    )
    .await;
    assert_eq!(mismatch["code"], "workcell_id_mismatch");

    let missing = run_agent_json(
        state.clone(),
        "missing-cell",
        serde_json::json!({
            "workcell_id": "missing-cell",
            "runner_epoch": 31,
            "program": "/bin/sh",
            "require_cgroup": false
        }),
    )
    .await;
    assert_eq!(missing["code"], "not_found");

    let fenced = run_agent_json(
        state.clone(),
        &workcell_id,
        serde_json::json!({
            "workcell_id": workcell_id,
            "runner_epoch": 30,
            "program": "/bin/sh",
            "require_cgroup": false
        }),
    )
    .await;
    assert_eq!(fenced["code"], "workcell_epoch_fenced");

    let release = response_json(
        super::workcells::release(
            State(state.clone()),
            AxumPath(workcell_id.clone()),
            axum::body::Bytes::from(
                serde_json::to_vec(&serde_json::json!({
                    "runner_epoch": 31
                }))
                .unwrap(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(release["state"], "released");

    let inactive = run_agent_json(
        state,
        &workcell_id,
        serde_json::json!({
            "workcell_id": workcell_id,
            "runner_epoch": 31,
            "program": "/bin/sh",
            "require_cgroup": false
        }),
    )
    .await;
    assert_eq!(inactive["code"], "workcell_claim_denied");
}

#[tokio::test]
async fn workcell_run_agent_rejects_unclaimed_roots_and_missing_programs() {
    let _guard = run_agent_test_lock().lock().await;
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let empty_workspace = tempdir().expect("create empty workcell workspace");
    let empty_cell =
        claim_run_workcell(state.clone(), empty_workspace.path(), Vec::new(), 41).await;
    let no_roots = run_agent_json(
        state.clone(),
        &empty_cell,
        serde_json::json!({
            "workcell_id": empty_cell,
            "runner_epoch": 41,
            "program": "/bin/sh",
            "require_cgroup": false
        }),
    )
    .await;
    assert_eq!(no_roots["code"], "workcell_run_path_denied");
    assert!(
        no_roots["reason"]
            .as_str()
            .expect("reason")
            .contains("no claimed repo roots")
    );

    let workspace = tempdir().expect("create workcell workspace");
    let repo_root = workspace.path().join("repo-slice");
    let outside_root = workspace.path().join("outside-slice");
    std::fs::create_dir_all(&repo_root).expect("create claimed repo root");
    std::fs::create_dir_all(&outside_root).expect("create outside repo root");
    let workcell_id =
        claim_run_workcell(state.clone(), workspace.path(), vec![repo_root.clone()], 43).await;

    let outside = run_agent_json(
        state.clone(),
        &workcell_id,
        serde_json::json!({
            "workcell_id": workcell_id,
            "runner_epoch": 43,
            "repo_root": outside_root,
            "program": "/bin/sh",
            "require_cgroup": false
        }),
    )
    .await;
    assert_eq!(outside["code"], "workcell_run_path_denied");
    assert!(
        outside["reason"]
            .as_str()
            .expect("reason")
            .contains("outside the claimed workcell slice")
    );

    let missing_program = run_agent_json(
        state.clone(),
        &workcell_id,
        serde_json::json!({
            "workcell_id": workcell_id,
            "runner_epoch": 43,
            "repo_root": repo_root,
            "program": "missing-agent",
            "require_cgroup": false
        }),
    )
    .await;
    assert_eq!(missing_program["code"], "workcell_run_path_denied");
    assert!(
        missing_program["reason"]
            .as_str()
            .expect("reason")
            .contains("program does not exist")
    );
}

#[tokio::test]
async fn workcell_run_agent_handles_relative_programs_and_driver_failures() {
    let _guard = run_agent_test_lock().lock().await;
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let workspace = tempdir().expect("create workcell workspace");
    let repo_root = workspace.path().join("repo-slice");
    std::fs::create_dir_all(&repo_root).expect("create claimed repo root");
    let workcell_id =
        claim_run_workcell(state.clone(), workspace.path(), vec![repo_root.clone()], 53).await;

    let script = repo_root.join("agent-relative.sh");
    std::fs::write(
        &script,
        r#"#!/bin/sh
i=0
while [ "$i" -lt 200 ]; do
  echo "relative-agent-$i"
  i=$((i + 1))
  sleep 0.005
done
"#,
    )
    .expect("write relative program");
    #[cfg(unix)]
    {
        let mut perms = std::fs::metadata(&script)
            .expect("read script metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).expect("mark script executable");
    }

    let run = run_agent_json(
        state.clone(),
        &workcell_id,
        serde_json::json!({
            "workcell_id": workcell_id,
            "runner_epoch": 53,
            "repo_root": repo_root,
            "program": "agent-relative.sh",
            "require_cgroup": false,
            "output_budget_bytes": 32
        }),
    )
    .await;
    if run["code"] == "workcell_run_sandbox_unavailable" {
        eprintln!("SKIP: sandbox unavailable for relative workcell run route");
        return;
    }
    assert_eq!(run["workcell_id"], workcell_id);
    assert_eq!(run["outcome"]["budget_exceeded"], true);
    assert!(
        run["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|event| { event["kind"] == "budget" && event["limit"] == 32 })
    );

    let directory_program = run_agent_json(
        state,
        &workcell_id,
        serde_json::json!({
            "workcell_id": workcell_id,
            "runner_epoch": 53,
            "repo_root": repo_root,
            "program": ".",
            "require_cgroup": false
        }),
    )
    .await;
    assert_eq!(
        directory_program["code"],
        "workcell_run_sandbox_unavailable"
    );
}

#[tokio::test]
async fn r5_jail_loop_exports_namespaced_branch_and_ci_evidence() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();

    let storage = tempfile::tempdir().expect("git storage dir");
    let workspace = tempdir().expect("create throwaway workspace");
    let (base_sha, head_sha) = build_bare_repo_with_diff(
        storage.path(),
        "alice",
        "jeryu",
        &[],
        &[(
            "src/r5.rs",
            "pub fn repaired() -> &'static str { \"r5\" }\n",
        )],
    );
    let state = Arc::new(WebState::new_with_git_storage(
        core.clone(),
        storage.path().to_path_buf(),
    ));
    let repair_body = serde_json::json!({
        "agent_id": "agent-wrath-17",
        "workspace_root": workspace.path(),
        "repo_roots": [workspace.path()],
        "branch_budget": 2,
        "runner_id": "xbabe0",
        "runner_epoch": 17,
        "git_status_summary": "rebase clean",
        "ci_snapshot_age_ms": 0,
        "startup": {
            "state": "rebased",
            "main_ref": "origin/main",
            "base_sha": base_sha,
            "head_sha": head_sha
        },
        "ci_run_id": "ci-r5-17",
        "failed_run_id": "run-r5-17",
        "failed_receipt_id": "receipt-r5-17",
        "failure_log_digest": "sha256:feedface"
    });

    let response = response_json(
        super::workcells::repair_live(
            State(state.clone()),
            axum::body::Bytes::from(serde_json::to_vec(&repair_body).unwrap()),
        )
        .await,
    )
    .await;
    let workcell_id = response["held"]["workcell_id"]
        .as_str()
        .expect("held workcell id")
        .to_string();
    assert_eq!(response["held"]["state"], "held");
    assert_eq!(response["repairing"]["state"], "repairing");
    assert_eq!(response["held"]["startup_main_ref"], "origin/main");

    let script_src = write_exec_script(
        "editbot",
        r#"#!/bin/sh
set -eu
target_dir=${EDIT_TARGET%/*}
mkdir -p "$target_dir"
printf '%s' "$EDIT_CONTENT" > "$EDIT_TARGET"
"#,
    );
    let staged = stage_editbot(workspace.path(), &script_src).expect("stage edit script");
    let driver = AgentDriver::default()
        .with_require_cgroup(false)
        .with_timeout(Duration::from_secs(10));
    let spec = CommandSpec::new(staged.to_string_lossy().to_string())
        .env("EDIT_TARGET", "src/r5.rs")
        .env(
            "EDIT_CONTENT",
            "pub fn repaired() -> &'static str { \"r5\" }\n",
        );
    let sink = CollectingSink::new();
    let Some(result) = run_or_skip(&driver, workspace.path(), &spec, &sink) else {
        let _ = std::fs::remove_file(&script_src);
        let _ = workspace.close();
        return;
    };
    assert!(result.succeeded(), "edit inside the jail must succeed");
    assert!(
        workspace.path().join("src/r5.rs").is_file(),
        "the jailed edit must land inside the workspace"
    );
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("src/r5.rs")).expect("read repaired file"),
        "pub fn repaired() -> &'static str { \"r5\" }\n"
    );

    let export_body = serde_json::json!({
        "workcell_id": workcell_id,
        "runner_epoch": 17,
        "branch_suffix": "repair-17",
        "changed_files": ["src/r5.rs"],
        "owner": "alice",
        "repo": "jeryu",
        "author": "agent-wrath-17",
        "title": "Repair failed tree",
        "body": "Repaired from failed tree"
    });
    let export = response_json(
        super::workcells::export_pr(
            State(state.clone()),
            AxumPath(workcell_id.clone()),
            axum::http::HeaderMap::new(),
            axum::body::Bytes::from(serde_json::to_vec(&export_body).unwrap()),
        )
        .await,
    )
    .await;
    let branch = export["branch"].as_str().expect("branch");
    assert!(branch.starts_with("agents/agent-wrath-17/workcells/"));
    assert_eq!(export["target_branch"], "main");
    assert!(export["pull_request_number"].as_u64().unwrap() > 0);

    let pr_number = export["pull_request_number"].as_u64().unwrap();
    let pr = state
        .core
        .get_pull_request("alice", "jeryu", pr_number)
        .expect("pull request exists");
    assert_eq!(pr.head.ref_name, branch);
    assert_eq!(pr.base.ref_name, "main");
    assert_eq!(pr.changed_files, vec!["src/r5.rs"]);

    let run = core
        .create_check_run(
            "alice",
            "jeryu",
            CreateCheckRunRequest {
                name: "r5-loop".to_string(),
                head_sha: pr.head.sha.clone(),
                status: Some(jeryu_core::CheckRunStatus::Completed),
                conclusion: Some(CheckConclusion::Success),
                output: Some(jeryu_core::CheckRunOutput {
                    title: "R5 lane green".to_string(),
                    summary: "claim -> rebase -> jailed edit -> PR -> CI evidence".to_string(),
                    text: None,
                }),
                ..CreateCheckRunRequest::default()
            },
        )
        .unwrap();

    let router = || {
        app(
            WebState::new(core.clone()),
            std::path::Path::new("/tmp/jeryu-no-spa"),
        )
    };
    let evidence = response_json(
        router()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/ci/runs/{}/evidence", run.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    let items = evidence.as_array().expect("evidence array");
    assert!(
        items.len() >= 3,
        "completed CI run must produce a receipt with evidence facets"
    );
    assert_eq!(items[0]["kind"], "run-metadata");
    assert_eq!(items[1]["kind"], "head-commit");
    assert_eq!(
        items[1]["payload"]["headSha"].as_str(),
        Some(pr.head.sha.as_str())
    );
    assert!(
        items
            .iter()
            .any(|item| item["kind"] == "conclusion" && item["payload"]["conclusion"] == "success")
    );

    let _ = std::fs::remove_file(&script_src);
    let _ = workspace.close();
}

/// WC-6: a workcell whose lease only permits `crates/jeryu-api`, exporting a
/// head commit that touches `crates/jeryu-core/x.rs`, must be slice-denied AND
/// must NOT create a pull request. This is the adversarial proof that the gate
/// is wired (the prior `let changed_files = Vec::new();` bypass would have
/// happily created the PR with no slice check).
#[tokio::test]
async fn workcell_export_slice_denies_out_of_slice_head_and_creates_no_pr() {
    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    let storage = tempfile::tempdir().expect("git storage dir");
    // Repo root = workspace_root; the lease only allows the api crate, but the
    // head commit changes a file in the core crate (out of slice).
    let repo_root = storage.path().join("work").join("repo");
    let allowed_subdir = repo_root.join("crates").join("jeryu-api");
    let (base_sha, head_sha) = build_bare_repo_with_diff(
        storage.path(),
        "alice",
        "jeryu",
        &[],
        &[("crates/jeryu-core/x.rs", "// out of slice\n")],
    );
    let state = Arc::new(WebState::new_with_git_storage(
        core,
        storage.path().to_path_buf(),
    ));

    // repo_roots is the in-slice subdir only; the runner unions in the
    // workspace_root, so the derived prefixes are ["crates/jeryu-api"].
    let repair_body = serde_json::json!({
        "agent_id": "agent-wrath-6",
        "workspace_root": repo_root,
        "repo_roots": [allowed_subdir],
        "branch_budget": 2,
        "runner_id": "xbabe6",
        "runner_epoch": 6,
        "git_status_summary": "rebase failed",
        "ci_snapshot_age_ms": 1200,
        "startup": {
            "state": "rebased",
            "main_ref": "origin/main",
            "base_sha": base_sha,
            "head_sha": head_sha,
        },
        "ci_run_id": "ci-parent-6",
        "failed_run_id": "run-6",
        "failed_receipt_id": "receipt-6",
        "failure_log_digest": "sha256:cafebabe"
    });
    let response = response_json(
        super::workcells::repair_live(
            State(state.clone()),
            axum::body::Bytes::from(serde_json::to_vec(&repair_body).unwrap()),
        )
        .await,
    )
    .await;
    let workcell_id = response["held"]["workcell_id"]
        .as_str()
        .expect("held workcell id")
        .to_string();

    let export_body = serde_json::json!({
        "workcell_id": workcell_id,
        "runner_epoch": 6,
        "branch_suffix": "repair-6",
        "owner": "alice",
        "repo": "jeryu",
        "author": "agent-wrath-6",
        "title": "Repair failed tree",
        "body": "Repaired from failed tree"
    });
    let export = response_json(
        super::workcells::export_pr(
            State(state.clone()),
            AxumPath(workcell_id.clone()),
            axum::http::HeaderMap::new(),
            axum::body::Bytes::from(serde_json::to_vec(&export_body).unwrap()),
        )
        .await,
    )
    .await;

    // The export is slice-denied, naming the out-of-slice path.
    assert_eq!(export["code"], "workcell_export_slice_denied");
    assert!(
        export["message"]
            .as_str()
            .expect("denial message")
            .contains("crates/jeryu-core/x.rs"),
        "denial must name the out-of-slice path: {export:?}"
    );

    // And crucially, NO pull request was created (the bypass would have made one).
    let pulls = state
        .core
        .list_pull_requests("alice", "jeryu", None)
        .expect("list pull requests");
    assert!(
        pulls.is_empty(),
        "a slice-denied export must not create a pull request, found: {pulls:?}"
    );
}

#[tokio::test]
async fn workcell_repair_live_requires_ci_run_id() {
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let repair_body = serde_json::json!({
        "agent_id": "agent-wrath-17",
        "workspace_root": "/workspace/core/web",
        "repo_roots": ["/workspace/core/web"],
        "branch_budget": 1,
        "runner_id": "xbabe0",
        "runner_epoch": 7,
        "git_status_summary": "rebase failed",
        "startup": {
            "state": "rebased",
            "main_ref": "origin/main",
            "base_sha": "abc123",
            "head_sha": "def456",
        },
        "failed_run_id": "legacy-run-17",
        "failed_receipt_id": "receipt-17",
        "failure_log_digest": "sha256:deadbeef"
    });

    let response = super::workcells::repair_live(
        State(state),
        axum::body::Bytes::from(serde_json::to_vec(&repair_body).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let err = response_json(response).await;
    assert_eq!(err["code"], "ci_run_id_required");
    assert_eq!(
        err["purpose"], "hold a failed workcell and start live repair",
        "repair requests must carry the CI run they are repairing"
    );
    for key in ["reason", "common_fixes", "docs_url", "repair_hint"] {
        assert!(err.get(key).is_some(), "missing repair field: {key}");
    }
}

/// An empty server yields an empty pool fabric (Unknown health), and the
/// fixture sample remains available purely as a test fallback.
#[test]
fn empty_server_assembles_empty_pool_activity_and_fixture_still_available() {
    let model = crate::read_model::assemble_read_model(&ForgeCore::new());
    assert!(model.pool_activity.repos.is_empty());
    assert!(model.pool_activity.pools.is_empty());
    assert!(matches!(model.pool_activity.health(), HealthLevel::Unknown));
    // The fixture is still reachable as a fallback. Its `pool_activity` is the
    // empty default — exactly why serving it left the Pools pane blank, which
    // is what the live assembler above now replaces.
    assert!(sample_read_model().pool_activity.pools.is_empty());
}

#[test]
fn bootstrap_and_repo_list_reflect_core_repositories() {
    let core = ForgeCore::new();
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
    let state = WebState::new(core);
    let bootstrap = bootstrap_payload(&state).expect("bootstrap serializes");
    assert_eq!(bootstrap.websocket_url, "/api/v1/ws");
    assert_eq!(bootstrap.recent_repositories.len(), 1);
    assert!(bootstrap.feature_flags.workcells);
    let repos = repo_list_response(&state);
    assert_eq!(repos.total, 1);
    assert_eq!(repos.repositories[0].id.owner, "alice");
}

#[test]
fn repo_list_classifies_jeryu_split_portal_and_members() {
    let core = ForgeCore::new();
    for (owner, name) in [
        ("neverhuman", "jeryu"),
        ("neverhuman", "jeryu-core"),
        ("alice", "unrelated"),
    ] {
        core.create_repository(
            owner,
            CreateRepositoryRequest {
                name: name.to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    }
    let state = WebState::new(core);
    let repos = repo_list_response(&state);
    let portal = repos
        .repositories
        .iter()
        .find(|repo| repo.id.owner == "neverhuman" && repo.id.name == "jeryu")
        .expect("portal repo");
    assert_eq!(portal.family.as_deref(), Some("jeryu-split"));
    assert_eq!(portal.repo_role, Some(RepositoryRole::PublicPortal));
    // The advertised clone URL must match where the smart-HTTP transport is
    // actually mounted (/git/...), not the SPA surface (/repos/...).
    assert_eq!(
        portal.clone_http_url.as_deref(),
        Some("/git/neverhuman/jeryu.git")
    );

    let core_repo = repos
        .repositories
        .iter()
        .find(|repo| repo.id.owner == "neverhuman" && repo.id.name == "jeryu-core")
        .expect("split member repo");
    assert_eq!(core_repo.family.as_deref(), Some("jeryu-split"));
    assert_eq!(core_repo.repo_role, Some(RepositoryRole::SplitMember));

    let unrelated = repos
        .repositories
        .iter()
        .find(|repo| repo.id.owner == "alice" && repo.id.name == "unrelated")
        .expect("unrelated repo");
    assert_eq!(unrelated.family, None);
    assert_eq!(unrelated.repo_role, None);
    assert_eq!(repos.facets.families, vec!["jeryu-split".to_string()]);
}

#[test]
fn split_catalog_loads_multiple_manifest_families() {
    let root = tempdir().expect("split manifests dir");
    write_file(
        root.path(),
        "jeryu.toml",
        r#"
repo_family = "jeryu-split"

[[repo]]
github_slug = "neverhuman/jeryu"
jeryu_slug = "jeryu/jeryu"
profile = "public-portal"

[[repo]]
github_slug = "neverhuman/jeryu-core"
jeryu_slug = "jeryu/jeryu-core"
profile = "split-member"
"#,
    );
    write_file(
        root.path(),
        "jekko.toml",
        r#"
repo_family = "jekko-split"

[[repo]]
github_slug = "neverhuman/jekko"
jeryu_slug = "jeryu/jekko"
profile = "public-portal"

[[repo]]
github_slug = "neverhuman/jekko-core"
jeryu_slug = "jeryu/jekko-core"
profile = "split-member"
"#,
    );

    let manifests = vec![
        root.path().join("jeryu.toml"),
        root.path().join("jekko.toml"),
    ];
    let catalog = SplitCatalog::load(&manifests);
    assert_eq!(
        catalog.classify("jeryu", "jeryu"),
        Some(("jeryu-split".to_string(), RepositoryRole::PublicPortal))
    );
    assert_eq!(
        catalog.classify("jeryu", "jeryu-core"),
        Some(("jeryu-split".to_string(), RepositoryRole::SplitMember))
    );
    assert_eq!(
        catalog.classify("jeryu", "jekko"),
        Some(("jekko-split".to_string(), RepositoryRole::PublicPortal))
    );
    assert_eq!(
        catalog.classify("neverhuman", "jekko-core"),
        Some(("jekko-split".to_string(), RepositoryRole::SplitMember))
    );
    assert_eq!(catalog.classify("alice", "unrelated"), None);
}

#[test]
fn split_catalog_classifies_tool_control_plane_and_finder() {
    // The tool control plane and its discovery arm ride the same `public-portal`
    // build profile as the real portal; the `-tool` / `-tool-finder` names must
    // disambiguate so only `jeryu` is the portal.
    let root = tempdir().expect("split manifests dir");
    write_file(
        root.path(),
        "jeryu.toml",
        r#"
repo_family = "jeryu-split"

[[repo]]
name = "jeryu"
github_slug = "neverhuman/jeryu"
jeryu_slug = "jeryu/jeryu"
profile = "public-portal"

[[repo]]
name = "jeryu-tool"
github_slug = "neverhuman/jeryu-tool"
jeryu_slug = "jeryu/jeryu-tool"
profile = "public-portal"

[[repo]]
name = "jeryu-tool-finder"
github_slug = "neverhuman/jeryu-tool-finder"
jeryu_slug = "jeryu/jeryu-tool-finder"
profile = "public-portal"
"#,
    );
    let catalog = SplitCatalog::load(&[root.path().join("jeryu.toml")]);
    assert_eq!(
        catalog.classify("jeryu", "jeryu"),
        Some(("jeryu-split".to_string(), RepositoryRole::PublicPortal))
    );
    assert_eq!(
        catalog.classify("jeryu", "jeryu-tool"),
        Some(("jeryu-split".to_string(), RepositoryRole::ToolControlPlane))
    );
    assert_eq!(
        catalog.classify("neverhuman", "jeryu-tool"),
        Some(("jeryu-split".to_string(), RepositoryRole::ToolControlPlane))
    );
    // The finder rides public-portal but is a normal member, not a portal.
    assert_eq!(
        catalog.classify("jeryu", "jeryu-tool-finder"),
        Some(("jeryu-split".to_string(), RepositoryRole::SplitMember))
    );
}

#[test]
fn repo_list_family_filter_uses_multi_manifest_catalog() {
    let manifest_root = tempdir().expect("split manifests dir");
    write_file(
        manifest_root.path(),
        "jeryu.toml",
        r#"
repo_family = "jeryu-split"

[[repo]]
github_slug = "neverhuman/jeryu"
jeryu_slug = "jeryu/jeryu"
profile = "public-portal"

[[repo]]
github_slug = "neverhuman/jeryu-core"
jeryu_slug = "jeryu/jeryu-core"
profile = "split-member"
"#,
    );
    write_file(
        manifest_root.path(),
        "jekko.toml",
        r#"
repo_family = "jekko-split"

[[repo]]
github_slug = "neverhuman/jekko"
jeryu_slug = "jeryu/jekko"
profile = "public-portal"

[[repo]]
github_slug = "neverhuman/jekko-core"
jeryu_slug = "jeryu/jekko-core"
profile = "split-member"
"#,
    );

    let core = ForgeCore::new();
    for name in ["jeryu", "jeryu-core", "jekko", "jekko-core", "unrelated"] {
        core.create_repository(
            "jeryu",
            CreateRepositoryRequest {
                name: name.to_string(),
                private: true,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    }
    let manifests = vec![
        manifest_root.path().join("jeryu.toml"),
        manifest_root.path().join("jekko.toml"),
    ];
    let storage = tempdir().expect("git storage dir");
    let state = WebState::with_repo_manager(
        core,
        Arc::new(RepoManager::new(GitdConfig::new(
            storage.path().to_path_buf(),
        ))),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/dist"),
        std::env::temp_dir(),
        SplitCatalog::load(&manifests),
    );

    let family_only = super::repositories::filtered_repo_list_response(
        &state,
        &super::repositories::RepoListQuery {
            family: Some("jekko-split".to_string()),
            ..Default::default()
        },
    );
    let mut names: Vec<&str> = family_only
        .repositories
        .iter()
        .map(|repo| repo.id.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(family_only.total, 2);
    assert_eq!(names, vec!["jekko", "jekko-core"]);
    assert_eq!(
        family_only.facets.families,
        vec!["jekko-split".to_string(), "jeryu-split".to_string()]
    );
    let portal = family_only
        .repositories
        .iter()
        .find(|repo| repo.id.name == "jekko")
        .expect("jekko portal");
    assert_eq!(portal.repo_role, Some(RepositoryRole::PublicPortal));
}

#[tokio::test]
async fn repo_refs_use_the_repository_default_branch_for_protection() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "trunk-repo".to_string(),
                private: false,
                description: None,
                default_branch: Some("trunk".to_string()),
            },
        )
        .unwrap();
    let state = Arc::new(WebState::new(core));

    let response = repo_refs(State(state), AxumPath(repo.id.to_string())).await;
    let refs = response_json(response).await;
    assert_eq!(refs.as_array().expect("refs array")[0]["name"], "trunk");
    assert_eq!(refs[0]["protected"], true);
}

#[tokio::test]
async fn readme_update_round_trips_through_the_local_api() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: Some("forge".to_string()),
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    let state = Arc::new(WebState::new(core));
    let markdown = "# Managed README\n\n- score: 92\n".to_string();
    let payload = serde_json::json!({ "markdown": markdown.clone() });
    let updated = response_json(
        repo_readme_update(
            State(state.clone()),
            AxumPath(repo.id.to_string()),
            axum::body::Bytes::from(serde_json::to_vec(&payload).unwrap()),
        )
        .await,
    )
    .await;
    assert_eq!(updated["markdown"], markdown);
    assert!(updated["html"].as_str().unwrap().contains("Managed README"));

    let readme = response_json(
        repo_readme(
            State(state.clone()),
            AxumPath(repo.id.to_string()),
            Query(super::repositories::SourceQuery::default()),
        )
        .await,
    )
    .await;
    assert_eq!(readme["markdown"], markdown);
    assert!(readme["html"].as_str().unwrap().contains("Managed README"));

    let blob = response_json(
        repo_blob(
            State(state.clone()),
            AxumPath(repo.id.to_string()),
            Query(super::repositories::SourceQuery::default()),
        )
        .await,
    )
    .await;
    assert_eq!(blob["text"], markdown);
    assert!(
        blob["rendered_markdown"]["html"]
            .as_str()
            .unwrap()
            .contains("Managed README")
    );

    let raw = repo_raw(
        State(state),
        AxumPath(repo.id.to_string()),
        Query(super::repositories::SourceQuery::default()),
    )
    .await;
    let raw_bytes = axum::body::to_bytes(raw.into_body(), usize::MAX)
        .await
        .expect("raw response bytes");
    assert!(
        std::str::from_utf8(&raw_bytes)
            .unwrap()
            .contains("Managed README")
    );
}

#[test]
fn markdown_renderer_escapes_html_and_builds_toc() {
    let rendered = render_markdown("# Hello <world>\n\nbody");
    assert!(rendered.html.contains("&lt;world&gt;"));
    assert_eq!(rendered.toc[0].id, "hello-world");
}

#[test]
fn map_method_covers_supported_verbs_only() {
    assert!(matches!(map_method(&HttpMethod::GET), Some(Method::Get)));
    assert!(matches!(
        map_method(&HttpMethod::PATCH),
        Some(Method::Patch)
    ));
    assert!(matches!(map_method(&HttpMethod::POST), Some(Method::Post)));
    assert!(matches!(map_method(&HttpMethod::PUT), Some(Method::Put)));
    assert!(map_method(&HttpMethod::DELETE).is_none());
}

#[test]
fn github_rest_edge_dispatches_repos_user_and_404() {
    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    let state = WebState::new(core);
    // The forwarder targets `state.github.handle(method, path, body)`; the
    // mounted `GET /repos` must return a GitHub-shaped 200 listing the repo.
    let repos = state.github.handle(Method::Get, "/repos", "");
    assert_eq!(repos.status, 200);
    assert!(repos.body.contains("alice"));
    assert!(repos.body.contains("jeryu"));
    // `GET /user` is mounted so `gh auth status` resolves a principal.
    assert_eq!(state.github.handle(Method::Get, "/user", "").status, 200);
    // An unknown route returns a clean GitHub-shaped 404, never a panic/500.
    assert_eq!(
        state
            .github
            .handle(Method::Get, "/repos/x/y/nope", "")
            .status,
        404
    );
}

#[tokio::test]
async fn browser_repo_routes_serve_the_spa_shell() {
    use axum::body::Body;
    use axum::http::Request;
    use tempfile::tempdir;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    let spa_dir = tempdir().expect("temp SPA dir");
    std::fs::write(
        spa_dir.path().join("index.html"),
        r#"<!doctype html><html><body><div id="root"></div></body></html>"#,
    )
    .expect("write SPA stub");
    let app = app(WebState::new(core), spa_dir.path());

    let api = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/repos")
                .header(header::ACCEPT, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(api.status(), StatusCode::OK);
    assert_eq!(
        api.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    let api_body = response_json(api).await;
    assert!(
        api_body.to_string().contains("alice"),
        "JSON clients must still reach the REST edge"
    );

    for path in [
        "/repos",
        "/repos/family/jeryu-split",
        "/repos/alice/jeryu",
        "/repos/alice/jeryu/pulls/99",
        "/repos/alice/jeryu/settings/merge",
        "/repos/jeryu/alice/jeryu",
        "/repos/jeryu/alice/jeryu/code",
        "/repos/jeryu/alice/jeryu/blob/main/src/lib.rs",
        "/repos/jeryu/alice/jeryu/settings/general",
        "/repos/jeryu/alice/jeryu/agents/run-1",
        "/tools",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header(
                        header::ACCEPT,
                        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                    )
                    .header(header::USER_AGENT, "Mozilla/5.0 (browser)")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "path {path}");
        assert!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("text/html")),
            "path {path} must serve the SPA shell"
        );
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("browser shell body");
        let body = std::str::from_utf8(&bytes).expect("browser shell is utf-8");
        assert!(
            body.contains(r#"<div id="root"></div>"#),
            "path {path} must serve the SPA shell"
        );
    }
}

#[tokio::test]
async fn signup_issues_session_cookie_for_followup_api_calls() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = app(
        WebState::new(ForgeCore::new()).with_auth(true, false, false),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/api/v1/auth/signup")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "login": "newuser",
                        "password": "correct horse"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let signup_cookie_header = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("signup sets a session cookie")
        .to_string();
    assert!(
        !signup_cookie_header.contains("Max-Age="),
        "signup keeps the browser-session cookie behavior"
    );
    let cookie = signup_cookie_header
        .split(';')
        .next()
        .expect("cookie name and value")
        .to_string();
    let body = response_json(response).await;
    assert_eq!(body["login"], "newuser");
    assert_eq!(body["role"], "user");

    let me = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(me.status(), StatusCode::OK);
    let body = response_json(me).await;
    assert_eq!(body["login"], "newuser");
    assert_eq!(body["mustChangePassword"], false);
    assert!(body["csrfToken"].as_str().is_some());
}

#[tokio::test]
async fn login_remember_me_controls_session_cookie_max_age_and_logout_expiry() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    core.create_account("remembered", "correct horse battery", UserRole::User)
        .unwrap();
    let app = app(
        WebState::new(core).with_auth(true, false, false),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );

    let normal_login = app
        .clone()
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "login": "remembered",
                        "password": "correct horse battery"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(normal_login.status(), StatusCode::OK);
    let normal_cookie = normal_login
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("normal login sets a session cookie")
        .to_string();
    assert!(normal_cookie.contains("jeryu-session="));
    assert!(normal_cookie.contains("HttpOnly"));
    assert!(normal_cookie.contains("SameSite=Lax"));
    assert!(
        !normal_cookie.contains("Max-Age="),
        "normal logins stay browser-session scoped"
    );
    let normal_body = response_json(normal_login).await;
    assert!(normal_body["csrfToken"].as_str().is_some());

    let remembered_login = app
        .clone()
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "login": "remembered",
                        "password": "correct horse battery",
                        "rememberMe": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(remembered_login.status(), StatusCode::OK);
    let remembered_cookie_header = remembered_login
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("remembered login sets a persistent session cookie")
        .to_string();
    assert!(remembered_cookie_header.contains("jeryu-session="));
    assert!(remembered_cookie_header.contains("Max-Age=2592000"));
    assert!(remembered_cookie_header.contains("HttpOnly"));
    assert!(remembered_cookie_header.contains("SameSite=Lax"));
    let remembered_cookie = remembered_cookie_header
        .split(';')
        .next()
        .expect("cookie name and value")
        .to_string();
    let remembered_body = response_json(remembered_login).await;
    let remembered_csrf = remembered_body["csrfToken"]
        .as_str()
        .expect("remembered login includes csrf token")
        .to_string();

    let logout = app
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/api/v1/auth/logout")
                .header(header::COOKIE, remembered_cookie)
                .header("x-jeryu-csrf", remembered_csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::OK);
    let expired_cookie = logout
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("logout expires the session cookie");
    assert!(expired_cookie.contains("jeryu-session="));
    assert!(expired_cookie.contains("Max-Age=0"));
}

#[test]
fn bootstrap_admin_password_creates_and_resets_admin_without_receipt_secret() {
    let state = WebState::new(ForgeCore::new());
    let data_dir = tempdir().expect("bootstrap data dir");

    bootstrap_public_accounts_with_admin_password(
        &state,
        data_dir.path(),
        Some("operator-admin-password-123"),
    )
    .expect("bootstrap admin with operator password");
    let admin = state
        .core
        .authenticate_password("jeryu-admin", "operator-admin-password-123")
        .expect("operator admin password works");
    assert_eq!(admin.role, UserRole::Admin);
    assert!(!admin.must_change_password);

    assert!(!data_dir.path().join("bootstrap-credentials.json").exists());
    assert!(state.core.get_account("jordanh").is_err());
    assert!(state.core.get_account("jepsont").is_err());

    bootstrap_public_accounts_with_admin_password(
        &state,
        data_dir.path(),
        Some("operator-admin-password-456"),
    )
    .expect("bootstrap admin reset with operator password");
    assert!(
        state
            .core
            .authenticate_password("jeryu-admin", "operator-admin-password-123")
            .is_err(),
        "reset must revoke the prior bootstrap admin password"
    );
    let reset = state
        .core
        .authenticate_password("jeryu-admin", "operator-admin-password-456")
        .expect("reset operator admin password works");
    assert_eq!(reset.role, UserRole::Admin);
    assert!(!reset.must_change_password);
}

#[tokio::test]
async fn admin_sees_all_repositories_but_fresh_signup_sees_none() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    for name in ["jeryu-core", "jeryu-web"] {
        core.create_repository(
            "jeryu",
            CreateRepositoryRequest {
                name: name.to_string(),
                private: true,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    }
    core.create_account("jeryu-admin", "admin-password-123", UserRole::Admin)
        .unwrap();
    let app = app(
        WebState::new(core).with_auth(true, false, false),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );

    let admin_login = app
        .clone()
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "login": "jeryu-admin",
                        "password": "admin-password-123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin_login.status(), StatusCode::OK);
    let admin_cookie = admin_login
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("admin login sets a session cookie")
        .split(';')
        .next()
        .expect("cookie name and value")
        .to_string();
    let admin_repos = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/repos")
                .header(header::COOKIE, admin_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin_repos.status(), StatusCode::OK);
    let admin_repos_body = response_json(admin_repos).await;
    assert_eq!(admin_repos_body["total"], 2);
    let admin_names: Vec<_> = admin_repos_body["repositories"]
        .as_array()
        .expect("repositories array")
        .iter()
        .filter_map(|repo| repo["id"]["name"].as_str())
        .collect();
    assert!(admin_names.contains(&"jeryu-core"));
    assert!(admin_names.contains(&"jeryu-web"));

    let signup = app
        .clone()
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/api/v1/auth/signup")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "login": "freshuser",
                        "password": "fresh-password-123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let fresh_cookie = signup
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("signup sets a session cookie")
        .split(';')
        .next()
        .expect("cookie name and value")
        .to_string();
    let fresh_repos = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/repos")
                .header(header::COOKIE, fresh_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fresh_repos.status(), StatusCode::OK);
    let fresh_repos_body = response_json(fresh_repos).await;
    assert_eq!(fresh_repos_body["total"], 0);
    assert_eq!(
        fresh_repos_body["repositories"]
            .as_array()
            .expect("repositories array")
            .len(),
        0
    );
}

#[tokio::test]
async fn forced_password_change_blocks_other_authenticated_routes_until_changed() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    let temporary_password = ["temporary", "pass", "123"].join("-");
    core.create_temporary_account("resetuser", &temporary_password, UserRole::User)
        .expect("create temporary account");
    let session = core.create_session("resetuser").expect("create session");
    let cookie = format!("jeryu-session={}", session.token);
    let csrf = session.session.csrf_token.clone();
    let app = app(
        WebState::new(core).with_auth(true, false, false),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );

    let me = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(me.status(), StatusCode::OK);
    let me_body = response_json(me).await;
    assert_eq!(me_body["login"], "resetuser");
    assert_eq!(me_body["mustChangePassword"], true);

    let repos = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/repos")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repos.status(), StatusCode::FORBIDDEN);
    let repos_body = response_json(repos).await;
    assert_eq!(repos_body["code"], "password_change_required");

    let token_denied = app
        .clone()
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/api/v1/auth/tokens")
                .header(header::COOKIE, &cookie)
                .header("x-jeryu-csrf", &csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::json!({ "name": "cli" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(token_denied.status(), StatusCode::FORBIDDEN);
    let token_denied_body = response_json(token_denied).await;
    assert_eq!(token_denied_body["code"], "password_change_required");

    let changed = app
        .clone()
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/api/v1/auth/password")
                .header(header::COOKIE, &cookie)
                .header("x-jeryu-csrf", &csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "currentPassword": temporary_password,
                        "newPassword": "new-password-12345"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(changed.status(), StatusCode::OK);
    let rotated_cookie = changed
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("password change rotates the session cookie")
        .split(';')
        .next()
        .expect("rotated cookie name and value")
        .to_string();
    let changed_body = response_json(changed).await;
    assert_eq!(changed_body["mustChangePassword"], false);
    let rotated_csrf = changed_body["csrfToken"]
        .as_str()
        .expect("password change returns the rotated CSRF token")
        .to_string();

    let old_session = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(old_session.status(), StatusCode::UNAUTHORIZED);

    let token_created = app
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/api/v1/auth/tokens")
                .header(header::COOKIE, &rotated_cookie)
                .header("x-jeryu-csrf", &rotated_csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::json!({ "name": "cli" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(token_created.status(), StatusCode::OK);
}

#[tokio::test]
async fn cookie_auth_mutations_require_csrf_and_pat_lifecycle_is_user_scoped() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = app(
        WebState::new(ForgeCore::new()).with_auth(true, false, false),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );
    let signup = app
        .clone()
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/api/v1/auth/signup")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "login": "tokenuser",
                        "password": "correct horse"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = signup
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("signup sets cookie")
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let signup_body = response_json(signup).await;
    let csrf = signup_body["csrfToken"].as_str().expect("csrf token");

    let denied = app
        .clone()
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/api/v1/auth/tokens")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::json!({ "name": "cli" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/api/v1/auth/tokens")
                .header(header::COOKIE, &cookie)
                .header("x-jeryu-csrf", csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::json!({ "name": "cli" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let created_body = response_json(created).await;
    let token_id = created_body["id"].as_str().expect("token id").to_string();
    assert!(
        created_body["token"]
            .as_str()
            .unwrap_or("")
            .starts_with("jpat_")
    );
    assert!(created_body["expiresAt"].as_str().is_some());

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/tokens")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list_body = response_json(list).await;
    assert_eq!(list_body.as_array().unwrap().len(), 1);
    assert!(list_body[0].get("token").is_none(), "list omits PAT secret");

    let deleted = app
        .oneshot(
            Request::builder()
                .method(HttpMethod::DELETE)
                .uri(format!("/api/v1/auth/tokens/{token_id}"))
                .header(header::COOKIE, &cookie)
                .header("x-jeryu-csrf", csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn auth_rate_limit_returns_429_for_repeated_login_failures() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = app(
        WebState::new(ForgeCore::new()).with_auth(true, false, false),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );
    let mut last_status = StatusCode::OK;
    for _ in 0..=10 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(HttpMethod::POST)
                    .uri("/api/v1/auth/login")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "login": "missing",
                            "password": "incorrect password"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        last_status = response.status();
    }
    assert_eq!(last_status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn trust_local_dev_requires_loopback_peer() {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = app(
        WebState::new(ForgeCore::new()).with_auth(true, true, false),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );
    let no_peer = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_peer.status(), StatusCode::UNAUTHORIZED);

    let mut request = Request::builder()
        .uri("/api/v1/auth/me")
        .body(Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            9988,
        ))));
    let loopback = app.oneshot(request).await.unwrap();
    assert_eq!(loopback.status(), StatusCode::OK);
}

#[tokio::test]
async fn github_rest_repo_edge_requires_auth_and_filters_grants() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "secret".to_string(),
            private: true,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    core.create_account("jeryu-admin", "admin-password", UserRole::Admin)
        .unwrap();
    core.create_account("jordanh", "jordanh-password", UserRole::User)
        .unwrap();
    core.grant_repo_access(
        "jeryu-admin",
        "jordanh",
        "alice",
        "jeryu",
        RepoAccessLevel::Read,
    )
    .unwrap();
    let token = core
        .create_personal_access_token("jordanh", "test", None)
        .unwrap()
        .secret;

    let app = app(
        WebState::new(core).with_auth(true, false, false),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );
    let unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/repos")
                .header(header::ACCEPT, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/repos")
                .header(header::ACCEPT, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let body = response_json(list).await;
    let names: Vec<_> = body
        .as_array()
        .expect("repo list is an array")
        .iter()
        .filter_map(|repo| repo["name"].as_str())
        .collect();
    assert_eq!(names, vec!["jeryu"]);

    let denied = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/repos/alice/secret")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let api_v3 = app
        .oneshot(
            Request::builder()
                .uri("/api/v3/repos")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(api_v3.status(), StatusCode::OK);
    let body = response_json(api_v3).await;
    assert_eq!(body.as_array().expect("repo list is an array").len(), 1);
}

#[tokio::test]
async fn github_rest_binds_mutation_actor_to_authenticated_principal() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: true,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    core.create_account("alice", "alice-password", UserRole::Admin)
        .unwrap();
    let token = core
        .create_personal_access_token("alice", "test", None)
        .unwrap()
        .secret;

    let response = app(
        WebState::new(core).with_auth(true, false, false),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    )
    .oneshot(
        Request::builder()
            .method(HttpMethod::POST)
            .uri("/repos/alice/jeryu/issues")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "title": "principal binding",
                    "actor": "mallory"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    assert_eq!(body["user"]["login"], "alice");
}

#[tokio::test]
async fn github_rest_reserves_ci_evidence_and_protection_controls() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: true,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    core.create_account("jeryu-admin", "admin-password", UserRole::Admin)
        .unwrap();
    core.create_account("writer", "writer-password", UserRole::User)
        .unwrap();
    core.create_account("repo-admin", "repo-admin-password", UserRole::User)
        .unwrap();
    core.grant_repo_access(
        "jeryu-admin",
        "writer",
        "alice",
        "jeryu",
        RepoAccessLevel::Write,
    )
    .unwrap();
    core.grant_repo_access(
        "jeryu-admin",
        "repo-admin",
        "alice",
        "jeryu",
        RepoAccessLevel::Admin,
    )
    .unwrap();
    let writer_token = core
        .create_personal_access_token("writer", "test", None)
        .unwrap()
        .secret;
    let admin_token = core
        .create_personal_access_token("jeryu-admin", "test", None)
        .unwrap()
        .secret;
    let repo_admin_token = core
        .create_personal_access_token("repo-admin", "test", None)
        .unwrap()
        .secret;
    let app = app(
        WebState::new(core).with_auth(true, false, false),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );

    let check_body = serde_json::json!({
        "name": "jeryu-deploy/required",
        "head_sha": "deadbeef",
        "status": "completed",
        "conclusion": "success"
    })
    .to_string();
    let status_body = serde_json::json!({
        "state": "success",
        "context": "jeryu-deploy/required"
    })
    .to_string();
    let protection_body = serde_json::json!({
        "required_status_checks": ["jeryu-deploy/required"],
        "required_approving_review_count": 1,
        "enforce_admins": true,
        "required_linear_history": true,
        "allow_force_pushes": false,
        "allow_deletions": false
    })
    .to_string();

    for prefix in ["", "/api/v3"] {
        for (method, path, body) in [
            (
                HttpMethod::POST,
                "/repos/alice/jeryu/check-runs",
                check_body.as_str(),
            ),
            (
                HttpMethod::POST,
                "/repos/alice/jeryu/statuses/deadbeef",
                status_body.as_str(),
            ),
            (
                HttpMethod::PUT,
                "/repos/alice/jeryu/branches/main/protection",
                protection_body.as_str(),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(format!("{prefix}{path}"))
                        .header(header::AUTHORIZATION, format!("Bearer {writer_token}"))
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::FORBIDDEN,
                "repo writers must not mutate protected evidence controls through {prefix}{path}"
            );
        }
    }

    for (path, body) in [
        ("/repos/alice/jeryu/check-runs", check_body),
        ("/repos/alice/jeryu/statuses/deadbeef", status_body),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(HttpMethod::POST)
                    .uri(path)
                    .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let protection = app
        .oneshot(
            Request::builder()
                .method(HttpMethod::PUT)
                .uri("/repos/alice/jeryu/branches/main/protection")
                .header(header::AUTHORIZATION, format!("Bearer {repo_admin_token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(protection_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(protection.status(), StatusCode::OK);
}

#[tokio::test]
async fn source_browser_rejects_unsafe_paths_before_storage_lookup() {
    let core = ForgeCore::new();
    core.create_repository(
        "jeryu",
        CreateRepositoryRequest {
            name: "jeryu-core".to_string(),
            private: true,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    let state = Arc::new(WebState::new(core));
    for path in ["/etc/passwd", "../Cargo.toml", "src/\0secret"] {
        let response = super::repositories::repo_tree(
            State(state.clone()),
            AxumPath("jeryu/jeryu-core".to_string()),
            Query(super::repositories::SourceQuery {
                path: Some(path.to_string()),
                ..Default::default()
            }),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsafe path should be rejected before git storage lookup: {path:?}"
        );
    }
}

#[test]
fn app_router_builds_without_route_conflicts() {
    // Axum panics during construction on overlapping/ambiguous routes, so
    // building the full router is the regression guard for the REST mount,
    // the steering middleware layer, and the /.jeryu/capabilities route.
    let _app = app(
        WebState::new(ForgeCore::new()),
        std::path::Path::new("/tmp"),
    );
}

#[tokio::test]
async fn serve_rejects_trust_local_dev_on_public_bind() {
    let temp = tempfile::tempdir().unwrap();
    let config = WebServerConfig {
        bind: std::net::SocketAddr::from(([0, 0, 0, 0], 0)),
        spa_dir: temp.path().join("spa"),
        data_dir: temp.path().join("data"),
        git_storage_root: temp.path().join("git"),
        split_manifests: Vec::new(),
        auth_required: true,
        trust_local_dev: true,
        secure_cookies: false,
    };
    let err = serve(config).await.expect_err("public bind must fail");
    assert!(err.to_string().contains("trust_local_dev"));
}

fn header_value<'a>(headers: &'a [(&'static str, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v.as_str())
}

fn known_mcp_tools() -> BTreeSet<String> {
    jeryu_mcp::tool_manifest()
        .into_iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect()
}

/// Builds a real bare repository at `<storage_root>/<owner>/<repo>.git` with a
/// base commit and a head commit that adds `head_files` (repo-relative path +
/// contents). Optional `base_files` let the base carry shared fixture files
/// that should not appear in the export diff. Returns `(base_sha, head_sha)` so
/// the workcell export slice gate can run a genuine `git diff base..head`
/// against it.
fn build_bare_repo_with_diff(
    storage_root: &std::path::Path,
    owner: &str,
    repo: &str,
    base_files: &[(&str, &str)],
    head_files: &[(&str, &str)],
) -> (String, String) {
    use std::process::Command;

    let bare = storage_root.join(owner).join(format!("{repo}.git"));
    std::fs::create_dir_all(bare.parent().expect("bare parent")).expect("create owner dir");

    // Work tree to author commits, then mirror-push into the bare repo.
    let work = storage_root.join(format!("{owner}-{repo}-work"));
    std::fs::create_dir_all(&work).expect("create work dir");

    let git = |args: &[&str], cwd: &std::path::Path| {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "jeryu-test")
            .env("GIT_AUTHOR_EMAIL", "jeryu-test@example.com")
            .env("GIT_COMMITTER_NAME", "jeryu-test")
            .env("GIT_COMMITTER_EMAIL", "jeryu-test@example.com")
            .output()
            .unwrap_or_else(|e| panic!("git {args:?} failed to spawn: {e}"));
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    git(&["init", "--quiet", "-b", "main"], &work);
    // Base commit: a single placeholder file unrelated to the diff under test.
    std::fs::write(work.join("BASE.txt"), "base\n").expect("write base file");
    git(&["add", "BASE.txt"], &work);
    for (rel, contents) in base_files {
        let path = work.join(rel);
        std::fs::create_dir_all(path.parent().expect("base file parent"))
            .expect("create base file dir");
        std::fs::write(&path, contents).expect("write base file");
        git(&["add", rel], &work);
    }
    git(&["commit", "--quiet", "-m", "base"], &work);
    let base_sha = git(&["rev-parse", "HEAD"], &work);

    // Head commit: add the requested in-slice/out-of-slice files.
    for (rel, contents) in head_files {
        let path = work.join(rel);
        std::fs::create_dir_all(path.parent().expect("file parent")).expect("create file dir");
        std::fs::write(&path, contents).expect("write head file");
        git(&["add", rel], &work);
    }
    git(&["commit", "--quiet", "-m", "head"], &work);
    let head_sha = git(&["rev-parse", "HEAD"], &work);

    // Mirror into the bare repo the API's RepoManager will resolve.
    git(
        &[
            "clone",
            "--quiet",
            "--bare",
            ".",
            bare.to_str().expect("bare utf8"),
        ],
        &work,
    );

    (base_sha, head_sha)
}

fn build_bare_repo_with_main_and_feature(
    storage_root: &std::path::Path,
    owner: &str,
    repo: &str,
    feature_path: &str,
    feature_contents: &str,
) -> (String, String) {
    use std::process::Command;

    let bare = storage_root.join(owner).join(format!("{repo}.git"));
    std::fs::create_dir_all(bare.parent().expect("bare parent")).expect("create owner dir");
    let work = storage_root.join(format!("{owner}-{repo}-merge-work"));
    std::fs::create_dir_all(&work).expect("create work dir");

    let git = |args: &[&str], cwd: &std::path::Path| {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "jeryu-test")
            .env("GIT_AUTHOR_EMAIL", "jeryu-test@example.com")
            .env("GIT_COMMITTER_NAME", "jeryu-test")
            .env("GIT_COMMITTER_EMAIL", "jeryu-test@example.com")
            .output()
            .unwrap_or_else(|e| panic!("git {args:?} failed to spawn: {e}"));
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    git(&["init", "--quiet", "-b", "main"], &work);
    std::fs::write(work.join("BASE.txt"), "base\n").expect("write base file");
    git(&["add", "BASE.txt"], &work);
    git(&["commit", "--quiet", "-m", "base"], &work);
    let base_sha = git(&["rev-parse", "HEAD"], &work);

    git(&["checkout", "--quiet", "-b", "feature"], &work);
    let path = work.join(feature_path);
    std::fs::create_dir_all(path.parent().expect("feature file parent"))
        .expect("create feature file dir");
    std::fs::write(&path, feature_contents).expect("write feature file");
    git(&["add", feature_path], &work);
    git(&["commit", "--quiet", "-m", "feature"], &work);
    let head_sha = git(&["rev-parse", "HEAD"], &work);
    git(
        &[
            "clone",
            "--quiet",
            "--bare",
            ".",
            bare.to_str().expect("bare utf8"),
        ],
        &work,
    );
    git(&["symbolic-ref", "HEAD", "refs/heads/main"], bare.as_path());

    (base_sha, head_sha)
}

fn bare_ref(storage_root: &std::path::Path, owner: &str, repo: &str, ref_name: &str) -> String {
    let bare = storage_root.join(owner).join(format!("{repo}.git"));
    let output = std::process::Command::new("git")
        .args(["rev-parse", ref_name])
        .current_dir(&bare)
        .output()
        .unwrap_or_else(|e| panic!("git rev-parse {ref_name} failed to spawn: {e}"));
    assert!(
        output.status.success(),
        "git rev-parse {ref_name} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

async fn response_json(response: AxumResponse) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body reads");
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|err| panic!("response body is not JSON ({err}): {bytes:?}"))
}

#[tokio::test]
async fn work_repo_create_persists_item_and_linked_issue() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .expect("create repo");
    let state = Arc::new(WebState::new(core));
    let created = response_json(
        super::work::repo_create(
            State(state.clone()),
            AxumPath(repo.id.to_string()),
            Json(jeryu_jira::CreateWorkItemRequest {
                title: "Fix flaky CI".to_string(),
                body: Some("The required lane flickers.".to_string()),
                kind: Some(jeryu_jira::WorkItemKind::Ci),
                labels: vec!["ci".to_string()],
                ..jeryu_jira::CreateWorkItemRequest::default()
            }),
        )
        .await,
    )
    .await;
    assert_eq!(created["key"], "JRY-1");
    assert_eq!(created["repo"]["id"], repo.id.to_string());
    assert_eq!(created["issue"]["number"], 1);

    let issues = state
        .core
        .list_issues("alice", "jeryu", None)
        .expect("list issues");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].title, "Fix flaky CI");

    let listed = response_json(
        super::work::repo_list(
            State(state),
            AxumPath(repo.id.to_string()),
            Query(super::work::WorkListQuery::default()),
        )
        .await,
    )
    .await;
    assert_eq!(listed["total"], 1);
    assert_eq!(listed["items"][0]["issue"]["number"], 1);
}

#[test]
fn github_issue_create_is_mirrored_into_work() {
    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .expect("create repo");
    let state = WebState::new(core);
    let created = state.github.handle(
        Method::Post,
        "/repos/alice/jeryu/issues",
        r#"{"title":"bug report","body":"it broke","labels":["bug"],"assignees":["alice"]}"#,
    );
    assert_eq!(created.status, 201, "create issue: {}", created.body);

    let work = state
        .work
        .list(jeryu_jira::WorkFilter::default())
        .expect("list work");
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].title, "bug report");
    assert_eq!(work[0].kind, jeryu_jira::WorkItemKind::Bug);
    let issue = work[0].issue.as_ref().expect("issue link");
    assert_eq!(issue.number, 1);
    assert_eq!(
        issue.url.as_deref(),
        Some("/repos/jeryu/alice/jeryu/issues#1")
    );

    let comment = state.github.handle(
        Method::Post,
        "/repos/alice/jeryu/issues/1/comments",
        r#"{"body":"confirmed reproduction","actor":"bob"}"#,
    );
    assert_eq!(comment.status, 201, "create comment: {}", comment.body);
    let detail = state.work.detail("JRY-1").expect("work detail");
    assert_eq!(detail.comments.len(), 1);
    assert_eq!(detail.comments[0].body, "confirmed reproduction");
    assert_eq!(detail.comments[0].author.id, "bob");

    let updated = state.github.handle(
        Method::Patch,
        "/repos/alice/jeryu/issues/1",
        r#"{"title":"fixed bug report","state":"closed","labels":["bug","p1"]}"#,
    );
    assert_eq!(updated.status, 200, "update issue: {}", updated.body);
    let synced = state.work.get("JRY-1").expect("synced work");
    assert_eq!(synced.title, "fixed bug report");
    assert_eq!(synced.status, jeryu_jira::WorkStatus::Done);
    assert_eq!(synced.labels, vec!["bug", "p1"]);
}

#[test]
fn pull_request_marker_issues_do_not_appear_as_work() {
    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .expect("create repo");
    core.create_pull_request(
        "alice",
        "jeryu",
        "alice",
        CreatePullRequestRequest {
            title: "change".to_string(),
            head: "feature".to_string(),
            base: "main".to_string(),
            ..CreatePullRequestRequest::default()
        },
    )
    .expect("create pr");
    assert_eq!(
        core.list_issues("alice", "jeryu", None)
            .expect("list marker issues")
            .len(),
        1
    );
    let state = WebState::new(core);
    let work = state
        .work
        .list(jeryu_jira::WorkFilter::default())
        .expect("list work");
    assert!(work.is_empty());
}

#[test]
fn advisory_headers_always_present_on_any_route() {
    // A plain browser UA still gets the API + fast-path advisories, but no
    // tool hint (we only steer automation/gh-like clients).
    let headers = advisory_headers(
        "Mozilla/5.0 (browser)",
        &HttpMethod::GET,
        "/api/v1/bootstrap",
    );
    assert_eq!(header_value(&headers, HDR_API), Some("v4"));
    assert_eq!(
        header_value(&headers, HDR_FAST_PATH),
        Some("/.jeryu/capabilities")
    );
    assert!(header_value(&headers, HDR_TOOL).is_none());
}

#[test]
fn advisory_headers_steer_gh_like_agents_to_mcp_tools() {
    // The gh CLI UA on a PR-create maps to the propose_patch MCP tool.
    let gh = advisory_headers(
        "GitHub CLI 2.40.0 go-gh/2.0",
        &HttpMethod::POST,
        "/repos/alice/jeryu/pulls",
    );
    assert_eq!(header_value(&gh, HDR_TOOL), Some(MCP_PATCH_TOOL));

    // A merge PUT maps to request_merge for any automation UA (curl here).
    let merge = advisory_headers(
        "curl/8.0",
        &HttpMethod::PUT,
        "/repos/alice/jeryu/pulls/7/merge",
    );
    assert_eq!(header_value(&merge, HDR_TOOL), Some(MCP_MERGE_TOOL));

    // GET PR routes steer to blocker explanation for agent UAs.
    let read = advisory_headers(
        "jeryu-agent/1.0",
        &HttpMethod::GET,
        "/repos/alice/jeryu/pulls",
    );
    assert_eq!(header_value(&read, HDR_TOOL), Some(MCP_BLOCKERS_TOOL));

    // Issue create gets a dedicated mutation tool.
    assert_eq!(
        header_value(
            &advisory_headers(
                "python-requests/2.31",
                &HttpMethod::POST,
                "/repos/a/b/issues"
            ),
            HDR_TOOL
        ),
        Some(MCP_ISSUE_TOOL)
    );

    // Actions writes steer to the local CI runner entrypoint.
    assert_eq!(
        header_value(
            &advisory_headers(
                "GitHub CLI 2.40.0 go-gh/2.0",
                &HttpMethod::POST,
                "/repos/alice/jeryu/actions/workflows/ci-fast.yml/dispatches"
            ),
            HDR_TOOL
        ),
        Some("jeryu.run_tests")
    );
}

#[test]
fn automation_agent_detection_is_case_insensitive_and_scoped() {
    assert!(is_automation_agent("GitHub CLI 2.40.0"));
    assert!(is_automation_agent("github cli"));
    assert!(is_automation_agent("go-gh/2.0"));
    assert!(is_automation_agent("okhttp/4.12.0"));
    assert!(is_automation_agent("curl/8.4.0"));
    assert!(is_automation_agent("python-requests/2.31.0"));
    assert!(is_automation_agent("Jeryu-Agent/1.0"));
    assert!(is_automation_agent("some-agent-runner"));
    // A normal browser is not steered with a tool hint.
    assert!(!is_automation_agent(
        "Mozilla/5.0 (Macintosh) AppleWebKit Safari"
    ));
    assert!(!is_automation_agent(""));
}

#[test]
fn suggested_tool_covers_mutations_and_reads() {
    assert_eq!(
        suggested_tool(&HttpMethod::POST, "/repos/a/b/pulls"),
        Some(MCP_PATCH_TOOL)
    );
    assert_eq!(
        suggested_tool(&HttpMethod::PUT, "/repos/a/b/pulls/3/merge"),
        Some(MCP_MERGE_TOOL)
    );
    assert_eq!(
        suggested_tool(&HttpMethod::GET, "/repos/a/b"),
        Some(MCP_READ_TOOL)
    );
    assert_eq!(
        suggested_tool(&HttpMethod::GET, "/repos/a/b/commits/deadbeef/check-runs"),
        Some(MCP_CHECKS_TOOL)
    );
    // A DELETE (unsupported verb) yields no hint.
    assert!(suggested_tool(&HttpMethod::DELETE, "/repos/a/b").is_none());
}

#[test]
fn advertised_mcp_tools_exist_in_catalog() {
    let known = known_mcp_tools();
    for tool in MCP_GUIDANCE_TOOLS {
        assert!(known.contains(*tool), "missing MCP catalog tool: {tool}");
    }
    for tool in [
        suggested_tool(&HttpMethod::POST, "/repos/a/b/pulls"),
        suggested_tool(&HttpMethod::PUT, "/repos/a/b/pulls/3/merge"),
        suggested_tool(&HttpMethod::GET, "/repos/a/b/commits/deadbeef/check-runs"),
        suggested_tool(&HttpMethod::GET, "/repos/a/b/pulls"),
        suggested_tool(&HttpMethod::GET, "/repos/a/b"),
    ] {
        let tool = tool.expect("tool hint");
        assert!(known.contains(tool), "invalid suggested MCP tool: {tool}");
    }
    let payload = capabilities_payload();
    for tool in payload["mcp_tools"].as_array().expect("mcp_tools array") {
        let tool = tool.as_str().expect("tool string");
        assert!(known.contains(tool), "invalid capability MCP tool: {tool}");
    }
}

#[tokio::test]
async fn live_unknown_github_route_returns_guided_json_not_spa() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = app(
        WebState::new(ForgeCore::new()),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri("/repos/alice/jeryu/unknown-thing")
                .header(header::USER_AGENT, "GitHub CLI 2.40.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let parsed = response_json(response).await;
    assert_eq!(
        parsed["jeryu_repair_hint"]["purpose"],
        "route unsupported GitHub-compatible REST request"
    );
    assert!(parsed["jeryu_mcp_tools"].as_array().unwrap().len() >= 4);
}

#[tokio::test]
async fn live_gh_auth_workaround_route_returns_guided_json_not_spa() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = app(
        WebState::new(ForgeCore::new()),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );
    let response = app
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/login/device/code")
                .header(header::USER_AGENT, "GitHub CLI 2.40.0 go-gh/2.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    let parsed = response_json(response).await;
    assert_eq!(
        parsed["jeryu_repair_hint"]["purpose"],
        "route GitHub CLI auth setup through Jeryu"
    );
    assert_eq!(
        parsed["jeryu_connection"]["gh_setup"],
        "jeryu gh-setup --host http://127.0.0.1:8787 --token-file ~/.jeryu/secrets/merge-token"
    );
    assert_eq!(
        parsed["jeryu_connection"]["gh_token_file"],
        "~/.jeryu/secrets/merge-token"
    );
    assert!(!parsed.to_string().contains("JERYU-TOKEN"));
}

#[tokio::test]
async fn live_api_v3_user_alias_serves_github_cli_status() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = app(
        WebState::new(ForgeCore::new()),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v3/user")
                .header(header::USER_AGENT, "GitHub CLI 2.40.0 go-gh/2.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let parsed = response_json(response).await;
    assert_eq!(parsed["login"], "jeryu-admin");
}

#[tokio::test]
async fn github_vendor_json_accept_is_not_served_the_spa_shell() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    core.create_repository(
        "jeryu",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    let app = app(
        WebState::new(core),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri("/repos")
                .header(header::ACCEPT, "application/vnd.github+json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(content_type.starts_with("application/json"));
    let parsed = response_json(response).await;
    assert_eq!(parsed.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn live_actions_write_returns_guided_json_and_steering_headers() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = app(
        WebState::new(ForgeCore::new()),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );
    let response = app
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/repos/alice/jeryu/actions/workflows/ci-fast.yml/dispatches")
                .header(header::USER_AGENT, "GitHub CLI 2.40.0 go-gh/2.0")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"ref":"main"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    assert_eq!(
        response
            .headers()
            .get("x-jeryu-api")
            .and_then(|value| value.to_str().ok()),
        Some("v4")
    );
    assert_eq!(
        response
            .headers()
            .get("x-jeryu-fast-path")
            .and_then(|value| value.to_str().ok()),
        Some("/.jeryu/capabilities")
    );
    assert_eq!(
        response
            .headers()
            .get("x-jeryu-tool")
            .and_then(|value| value.to_str().ok()),
        Some("jeryu.run_tests")
    );
    let parsed = response_json(response).await;
    assert_eq!(
        parsed["jeryu_repair_hint"]["purpose"],
        "route unsupported GitHub Actions write request"
    );
    assert_eq!(parsed["jeryu_connection"]["mcp"], "/mcp");
    assert_eq!(parsed["jeryu_steering"]["mcp_tool"], "jeryu.run_tests");
}

#[tokio::test]
async fn live_actions_workflow_routes_return_json_and_steering_headers() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "ci/fast".to_string(),
            head_sha: "deadbeef".to_string(),
            status: Some(jeryu_core::CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Success),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();

    let app = app(
        WebState::new(core),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );

    let detail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/repos/alice/jeryu/actions/workflows/1")
                .header(header::USER_AGENT, "GitHub CLI 2.40.0 go-gh/2.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    assert_eq!(
        detail
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    assert_eq!(
        detail
            .headers()
            .get("x-jeryu-tool")
            .and_then(|value| value.to_str().ok()),
        Some("jeryu.get_ci_run_jobs")
    );
    let detail_body = response_json(detail).await;
    assert_eq!(detail_body["name"], "ci/fast");

    let runs = app
        .oneshot(
            Request::builder()
                .uri("/repos/alice/jeryu/actions/workflows/ci-fast.yml/runs")
                .header(header::USER_AGENT, "GitHub CLI 2.40.0 go-gh/2.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(runs.status(), StatusCode::OK);
    assert_eq!(
        runs.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    let runs_body = response_json(runs).await;
    assert_eq!(runs_body["total_count"], 1);
    assert_eq!(runs_body["workflow_runs"][0]["workflow_id"], 1);
}

#[tokio::test]
async fn live_unsupported_verb_returns_guided_json() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = app(
        WebState::new(ForgeCore::new()),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );
    let delete = app
        .oneshot(
            Request::builder()
                .method(HttpMethod::DELETE)
                .uri("/repos/alice/jeryu")
                .header(header::USER_AGENT, "curl/8.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::METHOD_NOT_ALLOWED);
    let parsed = response_json(delete).await;
    assert_eq!(
        parsed["jeryu_repair_hint"]["purpose"],
        "route unsupported GitHub-compatible REST method"
    );
}

/// A list request with `?per_page`/`?page` now passes through (no longer a
/// guided 501) and the RFC5988 `Link` header is surfaced on the wire via
/// `github_response`'s header passthrough.
#[tokio::test]
async fn live_list_query_paginates_and_surfaces_link_header() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    // Two open PRs so a per_page=1 page leaves a `next`/`last` link.
    for (head, sha) in [("feat-a", "sha-a"), ("feat-b", "sha-b")] {
        core.create_pull_request(
            "alice",
            "jeryu",
            "alice",
            CreatePullRequestRequest {
                title: head.to_string(),
                head: head.to_string(),
                base: "main".to_string(),
                head_sha: Some(sha.to_string()),
                ..CreatePullRequestRequest::default()
            },
        )
        .unwrap();
    }

    let response = app(
        WebState::new(core),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    )
    .oneshot(
        Request::builder()
            .uri("/repos/alice/jeryu/pulls?per_page=1&page=1")
            .header(header::USER_AGENT, "go-gh/2.0")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let link = response
        .headers()
        .get("Link")
        .expect("Link header present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(link.contains("rel=\"next\""), "Link has next: {link}");
    assert!(link.contains("rel=\"last\""), "Link has last: {link}");
    let parsed = response_json(response).await;
    assert_eq!(
        parsed.as_array().expect("pulls array").len(),
        1,
        "per_page=1 returns a single PR"
    );
}

/// The overlap engine's `X-Jeryu-Reused-PR` header reaches the wire through
/// `github_response`'s passthrough when a create-PR request coalesces.
#[tokio::test]
async fn live_overlap_routing_surfaces_reused_pr_header() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    // An existing mergeable PR touching one file.
    core.create_pull_request(
        "alice",
        "jeryu",
        "alice",
        CreatePullRequestRequest {
            title: "existing".to_string(),
            head: "feat-a".to_string(),
            base: "main".to_string(),
            head_sha: Some("sha-a".to_string()),
            changed_files: vec!["src/a.rs".to_string()],
            ..CreatePullRequestRequest::default()
        },
    )
    .unwrap();

    let response = app(WebState::new(core), std::path::Path::new("/tmp/jeryu-no-spa"))
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri("/repos/alice/jeryu/pulls")
                .header(header::USER_AGENT, "go-gh/2.0")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"title":"hot-fix","head":"feat-a2","base":"main","changed_files":["src/a.rs"]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("X-Jeryu-Reused-PR")
            .expect("reused-pr header present")
            .to_str()
            .unwrap(),
        "1",
        "the header points at the reused PR number"
    );
}

#[tokio::test]
async fn advertised_mcp_endpoint_is_mounted() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let response = app(
        WebState::new(ForgeCore::new()),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    )
    .oneshot(Request::builder().uri("/mcp").body(Body::empty()).unwrap())
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn mcp_endpoint_requires_configured_authentication() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    core.create_account("alice", "alice-password", UserRole::Admin)
        .unwrap();
    let token = core
        .create_personal_access_token("alice", "test", None)
        .unwrap()
        .secret;
    let app = app(
        WebState::new(core).with_auth(true, false, false),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    );

    let unauthenticated = app
        .clone()
        .oneshot(Request::builder().uri("/mcp").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let authenticated = app
        .oneshot(
            Request::builder()
                .uri("/mcp")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authenticated.status(), StatusCode::METHOD_NOT_ALLOWED);
}

/// The live `/api/v1/ecosystem` route returns the camelCase tool-graph with
/// real catalog data through the mounted router.
#[tokio::test]
async fn ecosystem_route_serves_live_tool_graph() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    let response = app(
        WebState::new(core),
        std::path::Path::new("/tmp/jeryu-no-spa"),
    )
    .oneshot(
        Request::builder()
            .uri("/api/v1/ecosystem")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let parsed = response_json(response).await;
    assert_eq!(parsed["live"], true);
    assert_eq!(parsed["degradedReason"], "");
    let tools = parsed["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), jeryu_mcp::tool_manifest().len());
    // The first node carries the exact camelCase contract keys + live repo.
    let node = &tools[0];
    for key in [
        "name",
        "className",
        "conformance",
        "sideEffects",
        "dataClasses",
        "dependsOn",
    ] {
        assert!(node.get(key).is_some(), "missing contract key: {key}");
    }
    assert_eq!(node["provider"], "jeryu");
    assert_eq!(node["repo"], "alice/jeryu");
}

/// The live `/api/v1/ci/runs/{id}/evidence` route returns derived evidence
/// for a real run and a structured 404 for an unknown run id.
#[tokio::test]
async fn ci_run_evidence_route_serves_evidence_and_404() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    let run = core
        .create_check_run(
            "alice",
            "jeryu",
            CreateCheckRunRequest {
                name: "ci".to_string(),
                head_sha: "deadbeef".to_string(),
                status: Some(jeryu_core::CheckRunStatus::Completed),
                conclusion: Some(CheckConclusion::Success),
                ..CreateCheckRunRequest::default()
            },
        )
        .unwrap();
    let router = || {
        app(
            WebState::new(core.clone()),
            std::path::Path::new("/tmp/jeryu-no-spa"),
        )
    };

    let ok = router()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/ci/runs/{}/evidence", run.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    let parsed = response_json(ok).await;
    let items = parsed.as_array().expect("evidence array");
    assert!(!items.is_empty(), "a completed run yields evidence");
    for item in items {
        assert!(
            item["uri"]
                .as_str()
                .unwrap()
                .starts_with(&format!("jeryu://ci/run/{}/", run.id))
        );
        assert!(item["digest"].as_str().unwrap().starts_with("sha256:"));
        assert!(item.get("capturedAt").is_some());
    }

    // An unknown run id is a structured 404, not a silent empty list.
    let missing = router()
        .oneshot(
            Request::builder()
                .uri("/api/v1/ci/runs/00000000-0000-0000-0000-000000000000/evidence")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let err = response_json(missing).await;
    assert_eq!(err["code"], "not_found");
    assert_eq!(
        err["purpose"], "retrieve evidence for one live CI run",
        "repairable failures must carry typed guidance"
    );
    for key in ["reason", "common_fixes", "docs_url", "repair_hint"] {
        assert!(err.get(key).is_some(), "missing repair field: {key}");
    }
}

#[tokio::test]
async fn ci_run_evidence_enforces_repo_grants_and_revocation() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let core = ForgeCore::new();
    let [admin_token, reader_token, outsider_token] = [
        ("evidence-admin", UserRole::Admin),
        ("evidence-reader", UserRole::User),
        ("evidence-outsider", UserRole::User),
    ]
    .map(|(login, role)| {
        core.create_account(login, "ci-evidence-test-password", role)
            .unwrap();
        core.create_personal_access_token(login, "ci evidence route test", None)
            .unwrap()
            .secret
    });
    let runs = ["private-a", "private-b"].map(|repo| {
        core.create_repository(
            "alice",
            CreateRepositoryRequest {
                name: repo.to_string(),
                private: true,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
        core.create_check_run(
            "alice",
            repo,
            CreateCheckRunRequest {
                name: format!("restricted-check-{repo}"),
                head_sha: if repo == "private-a" {
                    "a".repeat(40)
                } else {
                    "b".repeat(40)
                },
                status: Some(jeryu_core::CheckRunStatus::Completed),
                conclusion: Some(CheckConclusion::Failure),
                output: Some(jeryu_core::CheckRunOutput {
                    title: format!("restricted-title-{repo}"),
                    summary: format!("restricted-summary-{repo}"),
                    text: None,
                }),
                ..CreateCheckRunRequest::default()
            },
        )
        .unwrap()
    });
    core.grant_repo_access(
        "evidence-admin",
        "evidence-reader",
        "alice",
        "private-a",
        RepoAccessLevel::Read,
    )
    .unwrap();
    let router = app(
        WebState::new(core.clone()).with_auth(true, false, false),
        Path::new("/tmp/jeryu-no-spa"),
    );
    let request = |id: &str, token: Option<&str>| {
        let mut builder = Request::builder().uri(format!("/api/v1/ci/runs/{id}/evidence"));
        if let Some(token) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        builder.body(Body::empty()).unwrap()
    };
    let run_ids = runs.each_ref().map(|run| run.id.to_string());
    let absent_id = uuid::Uuid::nil().to_string();

    // The normal auth gate is exercised, with no development bypass.
    for id in &run_ids {
        let response = router.clone().oneshot(request(id, None)).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response_json(response).await["code"], "unauthorized");
    }

    // A read grant is sufficient, and the administrator needs no explicit grant.
    for (run, token) in [
        (&runs[0], reader_token.as_str()),
        (&runs[0], admin_token.as_str()),
        (&runs[1], admin_token.as_str()),
    ] {
        let response = router
            .clone()
            .oneshot(request(&run.id.to_string(), Some(token)))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let facets = body.as_array().expect("authorized evidence array");
        assert_eq!(facets.len(), 4);
        assert_eq!(facets[0]["payload"]["name"], run.name);
        assert_eq!(facets[0]["payload"]["repo"], format!("alice/{}", run.repo));
        assert_eq!(facets[1]["payload"]["headSha"], run.head_sha);
        assert_eq!(
            facets[3]["payload"]["summary"],
            run.output.as_ref().unwrap().summary
        );
    }

    let missing = router
        .clone()
        .oneshot(request(&absent_id, Some(&reader_token)))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let missing_body = response_json(missing).await;
    assert_eq!(missing_body["code"], "not_found");
    for key in [
        "purpose",
        "reason",
        "common_fixes",
        "docs_url",
        "repair_hint",
    ] {
        assert!(missing_body.get(key).is_some(), "missing guidance: {key}");
    }

    // An unrelated account cannot use a known UUID. A grant to A does not grant B.
    // Missing, malformed, and inaccessible runs have identical response bodies.
    for (id, token) in [
        (run_ids[1].as_str(), reader_token.as_str()),
        (run_ids[0].as_str(), outsider_token.as_str()),
        (run_ids[1].as_str(), outsider_token.as_str()),
        ("not-a-uuid", reader_token.as_str()),
        (absent_id.as_str(), admin_token.as_str()),
    ] {
        let response = router
            .clone()
            .oneshot(request(id, Some(token)))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response_json(response).await;
        assert_eq!(body, missing_body);
        let text = body.to_string();
        for run in &runs {
            for hidden in [
                run.id.to_string(),
                run.name.clone(),
                run.repo.clone(),
                run.head_sha.clone(),
                run.output.as_ref().unwrap().title.clone(),
                run.output.as_ref().unwrap().summary.clone(),
            ] {
                assert!(!text.contains(&hidden), "denial leaked a run field");
            }
        }
    }

    // The same router and still-valid token must observe a grant revocation.
    assert!(
        core.revoke_repo_access_checked("evidence-admin", "evidence-reader", "alice", "private-a")
            .unwrap()
    );
    let revoked = router
        .oneshot(request(&run_ids[0], Some(&reader_token)))
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::NOT_FOUND);
    assert_eq!(response_json(revoked).await, missing_body);
}

#[test]
fn capabilities_payload_exposes_the_gh_command_map() {
    let payload = capabilities_payload();
    assert_eq!(payload["server"], "jeryu");
    assert_eq!(payload["api_version"], "v4");
    assert_eq!(payload["graphql"], "/graphql");
    assert_eq!(payload["websocket"], "/api/v1/ws");
    assert_eq!(payload["mcp_endpoint"], "/mcp");
    assert!(payload["fast_path_advice"].is_string());

    let map = &payload["gh_command_map"];
    for key in [
        "gh auth login",
        "gh auth refresh",
        "gh auth status",
        "gh pr create",
        "gh pr merge",
        "gh pr list",
        "gh issue create",
        "gh api",
        "gh repo create",
    ] {
        assert!(map.get(key).is_some(), "missing gh_command_map key: {key}");
    }
    assert_eq!(map["gh pr create"], MCP_PATCH_TOOL);
    assert_eq!(map["gh pr merge"], MCP_MERGE_TOOL);
    assert_eq!(map["gh issue create"], MCP_ISSUE_TOOL);
    assert_eq!(map["gh repo create"], "POST /repos");
    assert!(
        map["gh auth login"]
            .as_str()
            .expect("gh auth login guidance")
            .contains("jeryu gh-setup")
    );
    assert_eq!(
        payload["gh_auth_policy"]["run_instead"],
        "jeryu gh-setup --host http://127.0.0.1:8787 --token-file ~/.jeryu/secrets/merge-token"
    );
    assert_eq!(
        payload["gh_auth_policy"]["token_file"],
        "~/.jeryu/secrets/merge-token"
    );
    assert!(
        payload["gh_auth_policy"]["stale_host_repair"]
            .as_str()
            .expect("stale host repair")
            .contains("--token-file ~/.jeryu/secrets/merge-token")
    );
    assert!(
        payload["gh_auth_policy"]["host_auth_boundary"]
            .as_str()
            .expect("host auth boundary")
            .contains("GitHub.com auth and local Jeryu host auth are separate")
    );
    assert!(!payload.to_string().contains("JERYU-TOKEN"));
}

#[test]
fn payload_serialization_errors_are_not_silently_replaced() {
    struct FailingSerialize;

    impl serde::Serialize for FailingSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(<S::Error as serde::ser::Error>::custom("synthetic failure"))
        }
    }

    assert!(serialize_payload(&FailingSerialize).is_err());
}

/// A `WebState` whose read model has one saturated pool, so the activity
/// and pool scopes produce non-trivial snapshot frames.
fn ws_state_with_pool() -> WebState {
    use jeryu_readmodel::{PoolActivity, PoolRollup, RepoActivity};
    let mut state = WebState::new(ForgeCore::new());
    let mut pool = PoolRollup::new("trusted");
    pool.active_slots = 2;
    pool.running_jobs = 2;
    pool.queued_jobs = 3; // saturated
    pool.online_runners = 2;
    state.tui.pool_activity = PoolActivity {
        repos: vec![RepoActivity {
            repo: "alice/jeryu".into(),
            queued_jobs: 3,
            running_jobs: 2,
            ..RepoActivity::default()
        }],
        pools: vec![pool],
        ..PoolActivity::default()
    };
    state
}

#[test]
fn subscribe_frame_yields_scopes_and_snapshot_events() {
    let state = ws_state_with_pool();
    // A real client `subscribe` frame per the ClientWsMessage contract.
    let frame = json!({
        "type": "subscribe",
        "subscriptions": [
            { "scope": "global.activity", "filters": {} },
            { "scope": "pool.trusted", "filters": {} },
            { "scope": "system.health", "filters": {} },
        ],
    });
    // It deserializes into the typed wire contract (format is genuine).
    let parsed: jeryu_readmodel::contracts::ClientWsMessage =
        serde_json::from_value(frame.clone()).expect("subscribe frame parses");
    assert!(matches!(
        parsed,
        jeryu_readmodel::contracts::ClientWsMessage::Subscribe { .. }
    ));

    // The handler's scope extractor pulls every requested scope.
    let scopes = requested_scopes(&frame);
    assert_eq!(scopes.len(), 3);

    // Each subscribed scope yields a monotonic Event snapshot frame.
    let mut last_seq = 0u64;
    for scope in &scopes {
        let event = snapshot_event(&state, scope)
            .unwrap_or_else(|| panic!("scope {scope} should produce a snapshot"));
        assert_eq!(&event.scope, scope);
        assert!(event.seq > last_seq, "seq must be strictly monotonic");
        last_seq = event.seq;
        // The frame round-trips as a ServerWsMessage::Event on the wire.
        let msg = ServerWsMessage::Event { event };
        let encoded = serde_json::to_string(&msg).unwrap();
        assert!(encoded.contains("\"type\":\"event\""));
        assert!(encoded.contains(scope.as_str()));
    }

    // The activity snapshot reports the saturated pool's bottleneck.
    let activity = snapshot_event(&state, "global.activity").unwrap();
    let bottlenecks = activity.payload.get("bottlenecks").unwrap();
    assert!(
        bottlenecks.as_array().is_some_and(|b| !b.is_empty()),
        "saturated pool must surface a bottleneck"
    );
}

#[test]
fn unknown_scope_produces_no_snapshot() {
    let state = ws_state_with_pool();
    assert!(snapshot_event(&state, "pool.does-not-exist").is_none());
    assert!(snapshot_event(&state, "totally.unknown").is_none());
}

#[test]
fn ws_hub_seq_is_monotonic_and_tracks_subscribers() {
    let hub = WsHub::new();
    assert_eq!(hub.current_seq(), 0);
    let a = hub.next_seq();
    let b = hub.next_seq();
    assert!(b > a);
    assert_eq!(hub.current_seq(), b);

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let conn = hub.register(tx);
    let mut scopes = BTreeSet::new();
    scopes.insert("global.activity".to_string());
    scopes.insert("pool.trusted".to_string());
    hub.set_scopes(conn, &scopes);
    hub.remove_scopes(conn, &["pool.trusted".to_string()]);
    // Unregister must not panic and leaves the hub usable.
    hub.unregister(conn);
    assert!(hub.next_seq() > b);
}

#[test]
fn ws_hub_publish_fans_out_to_subscribed_connections_only() {
    let hub = WsHub::new();
    let (tx_sub, mut rx_sub) = tokio::sync::mpsc::unbounded_channel();
    let (tx_other, mut rx_other) = tokio::sync::mpsc::unbounded_channel();
    let subscribed = hub.register(tx_sub);
    let other = hub.register(tx_other);
    let mut scopes = BTreeSet::new();
    scopes.insert("tool_finder.scan".to_string());
    hub.set_scopes(subscribed, &scopes);
    let mut other_scopes = BTreeSet::new();
    other_scopes.insert("global.activity".to_string());
    hub.set_scopes(other, &other_scopes);

    let delivered = hub.publish("tool_finder.scan", |seq| {
        jeryu_readmodel::contracts::WebEvent {
            seq,
            timestamp: "t".to_string(),
            scope: "tool_finder.scan".to_string(),
            kind: "tool_finder.scan.progress".to_string(),
            entity: "system/host".to_string(),
            summary: "scan progress".to_string(),
            payload: json!({}),
        }
    });
    assert_eq!(delivered, 1, "only the subscribed connection receives");
    match rx_sub.try_recv() {
        Ok(ServerWsMessage::Event { event }) => {
            assert_eq!(event.scope, "tool_finder.scan");
            assert!(event.seq > 0);
        }
        other => panic!("expected pushed event, got {other:?}"),
    }
    assert!(
        rx_other.try_recv().is_err(),
        "unsubscribed conn gets nothing"
    );

    // A connection whose socket loop died (receiver dropped) is pruned.
    drop(rx_sub);
    let delivered = hub.publish("tool_finder.scan", |seq| {
        jeryu_readmodel::contracts::WebEvent {
            seq,
            timestamp: "t".to_string(),
            scope: "tool_finder.scan".to_string(),
            kind: "tool_finder.scan.progress".to_string(),
            entity: "system/host".to_string(),
            summary: "scan progress".to_string(),
            payload: json!({}),
        }
    });
    assert_eq!(delivered, 0, "dead subscriber pruned on publish");
}

#[test]
fn hello_frame_reports_current_seq() {
    let state = ws_state_with_pool();
    // Hand out two sequences, then the hello frame must echo current_seq.
    let _ = state.ws.next_seq();
    let _ = state.ws.next_seq();
    match hello_message(&state) {
        ServerWsMessage::Hello { current_seq, .. } => assert_eq!(current_seq, 2),
        other => panic!("expected hello, got {other:?}"),
    }
}

#[test]
fn unsubscribe_frame_extracts_scopes() {
    let frame = json!({ "type": "unsubscribe", "scopes": ["pool.trusted", "system.health"] });
    let dropped = unsubscribe_scopes(&frame);
    assert_eq!(dropped, vec!["pool.trusted", "system.health"]);
}

/// Health and the failing/running badges must reflect the repository's CURRENT
/// state — the latest run per check on a live head — not the append-only
/// check-run history. Legacy failures on stale shas and superseded failures on
/// a live head are both invisible once a newer green run exists.
#[test]
fn repo_health_scopes_check_runs_to_current_heads() {
    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: true,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    // Legacy failure on a sha no open PR (or branch head) points at anymore.
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "jeryu/ci".to_string(),
            head_sha: "stale-sha".to_string(),
            conclusion: Some(CheckConclusion::Failure),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    // Open PR whose head first failed, then passed on a rerun: only the
    // newest verdict for (head, name) may count.
    core.create_pull_request(
        "alice",
        "jeryu",
        "alice",
        CreatePullRequestRequest {
            title: "feature".to_string(),
            head: "feature".to_string(),
            base: "main".to_string(),
            head_sha: Some("live-sha".to_string()),
            ..CreatePullRequestRequest::default()
        },
    )
    .unwrap();
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "jeryu/ci".to_string(),
            head_sha: "live-sha".to_string(),
            conclusion: Some(CheckConclusion::Failure),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "jeryu/ci".to_string(),
            head_sha: "live-sha".to_string(),
            conclusion: Some(CheckConclusion::Success),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();

    let state = WebState::new(core);
    let repos = repo_list_response(&state);
    let summary = &repos.repositories[0];
    assert_eq!(
        summary.failing_checks, 0,
        "stale + superseded failures must not count"
    );
    assert_eq!(summary.health, "healthy");
    assert_eq!(summary.open_pull_requests, 1);
}

/// A failure that IS the latest verdict on a live head flips health to
/// warning, while a failing `jeryu/github-mirror` bookkeeping run never does —
/// mirror state has its own surface. In-progress runs only count on live heads.
#[test]
fn repo_health_counts_live_failures_and_ignores_mirror_checks() {
    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: true,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    core.create_pull_request(
        "alice",
        "jeryu",
        "alice",
        CreatePullRequestRequest {
            title: "feature".to_string(),
            head: "feature".to_string(),
            base: "main".to_string(),
            head_sha: Some("live-sha".to_string()),
            ..CreatePullRequestRequest::default()
        },
    )
    .unwrap();
    // Latest verdict for jeryu/ci on the live head is a failure.
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "jeryu/ci".to_string(),
            head_sha: "live-sha".to_string(),
            conclusion: Some(CheckConclusion::Failure),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    // A failed mirror push on the same head is bookkeeping, not ill-health.
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "jeryu/github-mirror".to_string(),
            head_sha: "live-sha".to_string(),
            conclusion: Some(CheckConclusion::Failure),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    // Running job on the live head counts; on a stale sha it does not.
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "jeryu/agent-review".to_string(),
            head_sha: "live-sha".to_string(),
            status: Some(jeryu_core::CheckRunStatus::InProgress),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "jeryu/agent-review".to_string(),
            head_sha: "stale-sha".to_string(),
            status: Some(jeryu_core::CheckRunStatus::InProgress),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();

    let state = WebState::new(core);
    let repos = repo_list_response(&state);
    let summary = &repos.repositories[0];
    assert_eq!(
        summary.failing_checks, 1,
        "mirror failure excluded, ci failure counted"
    );
    assert_eq!(summary.health, "warning");
    assert_eq!(summary.running_jobs, 1, "stale in-progress run excluded");
}

/// PATCH /api/v1/repos/:id applies only the keys present in the body:
/// a string sets the family, an explicit null clears it, junk is rejected,
/// and the families facet reflects the live values.
#[tokio::test]
async fn repo_update_sets_and_clears_family() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "jeryu",
            CreateRepositoryRequest {
                name: "veox-nht".to_string(),
                private: true,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    let state = Arc::new(WebState::new(core));
    let id = repo.id.to_string();

    let updated = response_json(
        repo_update(
            State(state.clone()),
            AxumPath(id.clone()),
            axum::body::Bytes::from_static(br#"{"family": "veox-split"}"#),
        )
        .await,
    )
    .await;
    assert_eq!(updated["family"], "veox-split");
    let list = repo_list_response(&state);
    assert_eq!(list.facets.families, vec!["veox-split".to_string()]);

    let cleared = response_json(
        repo_update(
            State(state.clone()),
            AxumPath("jeryu/veox-nht".to_string()),
            axum::body::Bytes::from_static(br#"{"family": null}"#),
        )
        .await,
    )
    .await;
    assert_eq!(cleared["family"], serde_json::Value::Null);
    assert!(repo_list_response(&state).facets.families.is_empty());

    // Unknown fields, non-string family, and blank family are 422s.
    for body in [
        br#"{"name": "nope"}"#.as_slice(),
        br#"{"family": 7}"#.as_slice(),
        br#"{"family": "  "}"#.as_slice(),
    ] {
        let response = repo_update(
            State(state.clone()),
            AxumPath(id.clone()),
            axum::body::Bytes::copy_from_slice(body),
        )
        .await;
        assert_eq!(
            response.into_response().status(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    let missing = repo_update(
        State(state),
        AxumPath("jeryu/missing".to_string()),
        axum::body::Bytes::from_static(br#"{"family": "x"}"#),
    )
    .await;
    assert_eq!(
        missing.into_response().status(),
        axum::http::StatusCode::NOT_FOUND
    );
}

/// DELETE /api/v1/repos/:id — an unknown repository is a structured 404.
#[tokio::test]
async fn repo_delete_unknown_repo_is_404() {
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let response = repo_admin::repo_delete(
        State(state),
        AxumPath("jeryu/missing".to_string()),
        axum::body::Bytes::from_static(br#"{"confirm_full_name": "jeryu/missing"}"#),
    )
    .await;
    assert_eq!(
        response.into_response().status(),
        axum::http::StatusCode::NOT_FOUND
    );
}

/// The confirmation must byte-match the repository's full name; anything else
/// (case drift, malformed body) is a 422 and the repository stays registered.
#[tokio::test]
async fn repo_delete_requires_byte_exact_confirmation() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    let state = Arc::new(WebState::new(core));
    for body in [
        br#"{"confirm_full_name": "alice/Jeryu"}"#.as_slice(),
        br#"{"confirm_full_name": "alice/*"}"#.as_slice(),
        br#"{"confirm_full_name": ""}"#.as_slice(),
        br#"not json"#.as_slice(),
    ] {
        let response = repo_admin::repo_delete(
            State(state.clone()),
            AxumPath(repo.id.to_string()),
            axum::body::Bytes::copy_from_slice(body),
        )
        .await;
        assert_eq!(
            response.into_response().status(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_eq!(repo_list_response(&state).repositories.len(), 1);
}

/// Happy-path registry deletion: a 200 receipt with per-collection counts and
/// an audit id, and the repository disappears from the list response. With
/// `delete_storage` unset nothing on disk is touched.
#[tokio::test]
async fn repo_delete_registry_returns_receipt_and_unlists_repo() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    core.create_label(
        "alice",
        "jeryu",
        jeryu_core::CreateLabelRequest {
            name: "bug".to_string(),
            color: "ff0000".to_string(),
            description: None,
        },
    )
    .unwrap();
    let state = Arc::new(WebState::new(core));

    let response = repo_admin::repo_delete(
        State(state.clone()),
        AxumPath(repo.id.to_string()),
        axum::body::Bytes::from_static(br#"{"confirm_full_name": "alice/jeryu"}"#),
    )
    .await
    .into_response();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let receipt = response_json(response).await;
    assert_eq!(receipt["registry_deleted"], true);
    assert_eq!(receipt["storage_deleted"], false);
    assert_eq!(receipt["storage_path"], Value::Null);
    assert_eq!(receipt["repo"]["owner"], "alice");
    assert_eq!(receipt["repo"]["name"], "jeryu");
    assert!(
        !receipt["audit_id"].as_str().unwrap_or_default().is_empty(),
        "the receipt must carry the audit entry id"
    );
    let counts = receipt["deleted_counts"].as_array().expect("counts array");
    let removed = |collection: &str| {
        counts
            .iter()
            .find(|count| count["collection"] == collection)
            .map(|count| count["removed"].as_u64().unwrap_or_default())
            .unwrap_or_else(|| panic!("missing collection {collection}"))
    };
    assert_eq!(removed("labels"), 1);
    assert_eq!(removed("branch_protections"), 1);
    assert_eq!(removed("counters"), 1);
    assert_eq!(removed("pulls"), 0);

    assert!(
        repo_list_response(&state).repositories.is_empty(),
        "the deleted repository must vanish from the list"
    );
}

/// `delete_storage: true` against a real managed bare repository removes the
/// directory and reports its path in the receipt.
#[tokio::test]
async fn repo_delete_storage_removes_managed_bare_dir() {
    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    let storage = tempdir().expect("git storage dir");
    let state = Arc::new(WebState::new_with_git_storage(
        core,
        storage.path().to_path_buf(),
    ));
    let bare = state
        .repo_manager
        .create_bare(&jeryu_gitd::RepoId::new("alice", "jeryu").expect("repo id"))
        .expect("create bare repo");
    assert!(bare.path.join("HEAD").is_file());

    let response = repo_admin::repo_delete(
        State(state.clone()),
        AxumPath("alice/jeryu".to_string()),
        axum::body::Bytes::from_static(
            br#"{"confirm_full_name": "alice/jeryu", "delete_storage": true}"#,
        ),
    )
    .await
    .into_response();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let receipt = response_json(response).await;
    assert_eq!(receipt["registry_deleted"], true);
    assert_eq!(receipt["storage_deleted"], true);
    assert!(
        !receipt["storage_path"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );
    assert!(!bare.path.exists(), "the bare dir must be removed");
    assert!(repo_list_response(&state).repositories.is_empty());
}

/// A symlinked `name.git` under the storage root is refused with a 422 and
/// the symlink target stays untouched (registry tier already committed).
#[cfg(unix)]
#[tokio::test]
async fn repo_delete_storage_refuses_symlinked_bare_dir() {
    let core = ForgeCore::new();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    let storage = tempdir().expect("git storage dir");
    let victim_root = tempdir().expect("victim dir");
    let victim = victim_root.path().join("victim.git");
    std::fs::create_dir_all(victim.join("objects")).expect("victim objects");
    std::fs::create_dir_all(victim.join("refs")).expect("victim refs");
    std::fs::write(victim.join("HEAD"), "ref: refs/heads/main\n").expect("victim HEAD");
    std::fs::create_dir_all(storage.path().join("alice")).expect("owner dir");
    std::os::unix::fs::symlink(&victim, storage.path().join("alice").join("jeryu.git"))
        .expect("symlink bare dir");
    let state = Arc::new(WebState::new_with_git_storage(
        core,
        storage.path().to_path_buf(),
    ));

    let response = repo_admin::repo_delete(
        State(state),
        AxumPath("alice/jeryu".to_string()),
        axum::body::Bytes::from_static(
            br#"{"confirm_full_name": "alice/jeryu", "delete_storage": true}"#,
        ),
    )
    .await
    .into_response();
    assert_eq!(
        response.status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(
        victim.join("HEAD").is_file(),
        "the symlink target must be untouched"
    );
}

/// Live work blocks deletion: a running repo-scoped agent run yields a 409
/// and the repository stays registered.
#[tokio::test]
async fn repo_delete_conflicts_with_live_agent_run() {
    let core = ForgeCore::new();
    // seed_test_run pins its owning repo to "owner/repo".
    core.create_repository(
        "owner",
        CreateRepositoryRequest {
            name: "repo".to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    let state = Arc::new(WebState::new(core));
    state.agent_runs.seed_test_run("run-live-409", 4);

    let response = repo_admin::repo_delete(
        State(state.clone()),
        AxumPath("owner/repo".to_string()),
        axum::body::Bytes::from_static(br#"{"confirm_full_name": "owner/repo"}"#),
    )
    .await;
    assert_eq!(
        response.into_response().status(),
        axum::http::StatusCode::CONFLICT
    );
    assert_eq!(repo_list_response(&state).repositories.len(), 1);
}

/// Negative authorization / data-isolation proof for the DELETE surface:
/// a confirmation naming ANOTHER owner's repository never deletes anything
/// (the confirm is bound to the addressed resource, so a non-owner name is
/// refused), and deleting one owner's repository leaves the other owner's
/// same-named repository and its data fully intact.
#[tokio::test]
async fn repo_delete_cannot_cross_owner_boundaries() {
    let core = ForgeCore::new();
    let alice = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    // Bob owns a repository with the SAME name: the sharpest isolation probe
    // for the (owner, name)-keyed state maps.
    let bob = core
        .create_repository(
            "bob",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    core.create_label(
        "bob",
        "jeryu",
        jeryu_core::CreateLabelRequest {
            name: "keep".to_string(),
            color: "00ff00".to_string(),
            description: None,
        },
    )
    .unwrap();
    let state = Arc::new(WebState::new(core));

    // Non-owner confirmation: addressing bob's repo while confirming alice's
    // full name (and vice versa) is refused and deletes nothing.
    for (target, wrong_confirm) in [
        (
            bob.id.to_string(),
            br#"{"confirm_full_name": "alice/jeryu"}"#.as_slice(),
        ),
        (
            alice.id.to_string(),
            br#"{"confirm_full_name": "bob/jeryu"}"#.as_slice(),
        ),
    ] {
        let response = repo_admin::repo_delete(
            State(state.clone()),
            AxumPath(target),
            axum::body::Bytes::from_static(wrong_confirm),
        )
        .await;
        assert_eq!(
            response.into_response().status(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "a non-owner confirmation must never authorize a delete"
        );
    }
    assert_eq!(repo_list_response(&state).repositories.len(), 2);

    // Deleting alice's repo must not touch bob's same-named repo or its data.
    let response = repo_admin::repo_delete(
        State(state.clone()),
        AxumPath(alice.id.to_string()),
        axum::body::Bytes::from_static(br#"{"confirm_full_name": "alice/jeryu"}"#),
    )
    .await
    .into_response();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let remaining = repo_list_response(&state);
    assert_eq!(remaining.repositories.len(), 1);
    assert_eq!(remaining.repositories[0].id.owner, "bob");
    assert_eq!(remaining.repositories[0].id.name, "jeryu");
    let bob_labels = state.github.core().list_labels("bob", "jeryu").unwrap();
    assert_eq!(
        bob_labels.len(),
        1,
        "bob's data must survive alice's delete"
    );
    assert_eq!(bob_labels[0].name, "keep");
}

/// Score ingest → list → repo-summary badge join, plus mirror status derived
/// from jeryu/github-mirror bookkeeping runs.
#[tokio::test]
async fn jankurai_scores_ingest_and_surface_on_the_repo_summary() {
    let core = ForgeCore::new();
    let repo = core
        .create_repository(
            "jeryu",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: true,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
    // Mirror bookkeeping: an old success then a newer failure -> last attempt
    // failed, last success still reported, repo health untouched.
    core.create_check_run(
        "jeryu",
        "jeryu",
        CreateCheckRunRequest {
            name: "jeryu/github-mirror".to_string(),
            head_sha: "m1".to_string(),
            conclusion: Some(CheckConclusion::Success),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    core.create_check_run(
        "jeryu",
        "jeryu",
        CreateCheckRunRequest {
            name: "jeryu/github-mirror".to_string(),
            head_sha: "m2".to_string(),
            conclusion: Some(CheckConclusion::Failure),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    let state = Arc::new(WebState::new(core));
    let id = repo.id.to_string();

    let denied = repo_jankurai_scores_ingest(
        State(state.clone()),
        authenticated_account("writer"),
        AxumPath(id.clone()),
        axum::body::Bytes::from_static(
            br#"{"branch":"main","commit_sha":"abc","score":100,"hard_findings":0,"decision":"scored","caps_applied":[]}"#,
        ),
    )
    .await
    .into_response();
    assert_eq!(denied.status(), axum::http::StatusCode::FORBIDDEN);
    assert!(
        state
            .github
            .core()
            .list_jankurai_scores("jeryu", "jeryu", None, None)
            .unwrap()
            .is_empty(),
        "a denied writer submission must not create score state"
    );

    // Ingest: a scored run on main.
    let created = repo_jankurai_scores_ingest(
        State(state.clone()),
        authenticated_admin_account("jeryu-admin"),
        AxumPath(id.clone()),
        axum::body::Bytes::from_static(
            br#"{"branch":"main","commit_sha":"abc","score":92,"hard_findings":0,"decision":"scored","caps_applied":[]}"#,
        ),
    )
    .await
    .into_response();
    assert_eq!(created.status(), axum::http::StatusCode::CREATED);

    // The backfill probe shape: GET ?sha= returns {"scores": [...]}.
    let listed = response_json(
        repo_jankurai_scores_list(
            State(state.clone()),
            AxumPath("jeryu/jeryu".to_string()),
            Query(super::repositories::ScoreListQuery {
                branch: None,
                sha: Some("abc".to_string()),
            }),
        )
        .await,
    )
    .await;
    assert_eq!(listed["scores"].as_array().unwrap().len(), 1);
    assert_eq!(listed["scores"][0]["score"], 92);

    // Summary join + mirror posture.
    let repos = repo_list_response(&state);
    let summary = &repos.repositories[0];
    assert_eq!(summary.jankurai_score, Some(92));
    assert_eq!(summary.jankurai_decision.as_deref(), Some("scored"));
    assert!(summary.jankurai_scored_at.is_some());
    let mirror = summary.mirror.as_ref().expect("mirror reported");
    assert!(mirror.configured);
    assert!(!mirror.last_attempt_ok, "newest mirror run failed");
    assert_eq!(mirror.last_attempt_conclusion.as_deref(), Some("failure"));
    assert!(
        mirror.last_success_at.is_some(),
        "old success still visible"
    );
    assert_eq!(
        summary.failing_checks, 0,
        "mirror failures are not repo ill-health"
    );
    assert_eq!(summary.health, "healthy");

    // Tool-failed ingest: null score + decision surfaces, badge score stays None.
    let failed = repo_jankurai_scores_ingest(
        State(state.clone()),
        authenticated_admin_account("jeryu-admin"),
        AxumPath(id.clone()),
        axum::body::Bytes::from_static(
            br#"{"branch":"main","commit_sha":"zzz","score":null,"decision":"tool-failed","tool_exit":2}"#,
        ),
    )
    .await
    .into_response();
    assert_eq!(failed.status(), axum::http::StatusCode::CREATED);
    let summary = &repo_list_response(&state).repositories[0];
    assert_eq!(summary.jankurai_score, None);
    assert_eq!(summary.jankurai_decision.as_deref(), Some("tool-failed"));

    // Garbage and unknown repos are rejected cleanly.
    let bad = repo_jankurai_scores_ingest(
        State(state.clone()),
        authenticated_admin_account("jeryu-admin"),
        AxumPath(id),
        axum::body::Bytes::from_static(br#"{"branch":"main"}"#),
    )
    .await
    .into_response();
    assert_eq!(bad.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    let missing = repo_jankurai_scores_ingest(
        State(state),
        authenticated_admin_account("jeryu-admin"),
        AxumPath("jeryu/missing".to_string()),
        axum::body::Bytes::from_static(
            br#"{"branch":"main","commit_sha":"a","decision":"scored"}"#,
        ),
    )
    .await
    .into_response();
    assert_eq!(missing.status(), axum::http::StatusCode::NOT_FOUND);
}

/// GET /api/v1/repos must apply the SPA's filters SERVER-SIDE — the family
/// drill-down page is nothing but `?family=`, and it shipped against a
/// handler that ignored every query parameter (the e2e mock honoured the
/// filter, masking the gap). This drives the real handler through Query
/// extraction so a mock can never hide it again.
#[tokio::test]
async fn repo_list_filters_apply_server_side() {
    let core = ForgeCore::new();
    for (name, family) in [
        ("jmcp-core", Some("jmcp-split")),
        ("jmcp-web", Some("jmcp-split")),
        ("veox-nht", Some("veox-split")),
        ("openQG", None),
    ] {
        core.create_repository(
            "jeryu",
            CreateRepositoryRequest {
                name: name.to_string(),
                private: true,
                description: Some(format!("{name} repository")),
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
        if let Some(family) = family {
            core.set_repository_family("jeryu", name, Some(family.to_string()))
                .unwrap();
        }
    }
    let state = Arc::new(WebState::new(core));
    let account = Extension(AccountSummary {
        login: "jeryu-admin".to_string(),
        display_name: "Jeryu Admin".to_string(),
        role: jeryu_core::UserRole::Admin,
        status: AccountStatus::Active,
        auth_epoch: 0,
        must_change_password: false,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    });

    let family_only = repos(
        State(state.clone()),
        account.clone(),
        Query(super::repositories::RepoListQuery {
            family: Some("jmcp-split".to_string()),
            ..Default::default()
        }),
    )
    .await
    .0;
    let names: Vec<&str> = family_only
        .repositories
        .iter()
        .map(|repo| repo.id.name.as_str())
        .collect();
    assert_eq!(
        family_only.total, 2,
        "only the family members may be listed"
    );
    assert!(names.contains(&"jmcp-core") && names.contains(&"jmcp-web"));
    // Facets keep the full picture so the filter chips stay populated.
    assert_eq!(
        family_only.facets.families,
        vec!["jmcp-split".to_string(), "veox-split".to_string()]
    );

    let searched = repos(
        State(state.clone()),
        account.clone(),
        Query(super::repositories::RepoListQuery {
            q: Some("veox".to_string()),
            ..Default::default()
        }),
    )
    .await
    .0;
    assert_eq!(searched.total, 1);
    assert_eq!(searched.repositories[0].id.name, "veox-nht");

    let sorted = repos(
        State(state.clone()),
        account.clone(),
        Query(super::repositories::RepoListQuery {
            sort: Some("name".to_string()),
            ..Default::default()
        }),
    )
    .await
    .0;
    let sorted_names: Vec<&str> = sorted
        .repositories
        .iter()
        .map(|repo| repo.id.name.as_str())
        .collect();
    assert_eq!(
        sorted_names,
        vec!["jmcp-core", "jmcp-web", "openQG", "veox-nht"]
    );

    // Archived repos are excluded by default and exclusive under ?archived=1.
    let archived = repos(
        State(state),
        account,
        Query(super::repositories::RepoListQuery {
            archived: Some("1".to_string()),
            ..Default::default()
        }),
    )
    .await
    .0;
    assert_eq!(archived.total, 0, "no archived repos exist in this fixture");
}

#[tokio::test]
async fn tool_finder_scan_status_idle_busy_guard_and_snapshot_arm() {
    let core = ForgeCore::new();
    let mut raw_state = WebState::new(core);
    // No manifests wired: a scan cannot start, with a typed repair path.
    let state = Arc::new(raw_state.clone());
    let idle = response_json(
        super::tool_finder::scan_status(State(state.clone()))
            .await
            .into_response(),
    )
    .await;
    assert_eq!(idle["phase"], "idle");
    assert_eq!(idle["running"], false);
    let no_manifests =
        response_json(super::tool_finder::scan_start(State(state.clone())).await).await;
    assert_eq!(no_manifests["code"], "tool_finder_no_manifests");

    // With manifests wired and the single flight already claimed, POST is a
    // 409 with the running snapshot's repair hints.
    raw_state.split_manifests = vec![PathBuf::from("/tmp/fixture-split/repos.manifest.toml")];
    let state = Arc::new(raw_state);
    state
        .tool_finder_scan
        .try_begin()
        .expect("claim the single flight");
    let busy = response_json(super::tool_finder::scan_start(State(state.clone())).await).await;
    assert_eq!(busy["code"], "tool_finder_scan_running");

    // The websocket snapshot arm serves the live status for the scope.
    let event = snapshot_event(&state, "tool_finder.scan").expect("tool_finder.scan snapshot");
    assert_eq!(event.kind, "tool_finder.scan.snapshot");
    assert_eq!(event.entity, "system/host");
    assert_eq!(event.payload["running"], true);
    assert_eq!(event.payload["phase"], "discover");
}

#[tokio::test]
async fn tool_finder_dashboard_and_propose_round_trip() {
    let core = ForgeCore::new();
    let mut raw_state = WebState::new(core);

    // A writable fixture registry so propose can append.
    let registry_dir = tempdir().expect("registry fixture");
    write_file(
        registry_dir.path(),
        "tools-registry.toml",
        "schema_version = \"1\"\n\n[[tool]]\nid = \"existing\"\nname = \"Existing\"\nkind = \"rust-crate\"\nstatus = \"published\"\nadopting_repos = []\ncandidate_repos = []\nloc_saved = 10\nloc_saved_estimate = 0\n",
    );
    raw_state.tool_registry_path = Some(registry_dir.path().join("tools-registry.toml"));
    let state = Arc::new(raw_state);

    // Persist a cross-repo fixture scan under the system repo id.
    let repo_a = tempdir().expect("repo-a");
    let repo_b = tempdir().expect("repo-b");
    let repeated = r#"
pub fn alpha(input: &str) -> Result<String, String> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        let response = call_remote(input);
        if response.is_ok() {
            return response;
        }
        if attempts > 3 {
            return Err("failed".to_string());
        }
    }
}
"#;
    write_file(repo_a.path(), "src/lib.rs", repeated);
    write_file(
        repo_b.path(),
        "src/lib.rs",
        &repeated.replace("alpha", "beta"),
    );
    let report = jeryu_codegraph::scan_tool_build_family(
        &[
            ("repo-a".to_string(), repo_a.path().to_path_buf()),
            ("repo-b".to_string(), repo_b.path().to_path_buf()),
        ],
        "system/host",
        "working-tree",
        ToolBuildScanConfig {
            window_lines: 5,
            min_normalized_tokens: 12,
            min_occurrences: 2,
            max_file_bytes: 64 * 1024,
            max_clusters: 10,
            min_repo_count: 2,
        },
    )
    .unwrap();
    assert!(!report.clusters.is_empty());
    state
        .codegraph_store
        .persist_tool_build_report(&report)
        .unwrap();

    // Dashboard: families over the persisted scan, enriched per cluster.
    let dashboard = response_json(
        super::tool_finder::dashboard(
            State(state.clone()),
            Query(super::tool_finder::DashboardQuery {
                limit: Some(50),
                include_ignored: false,
            }),
        )
        .await,
    )
    .await;
    assert!(dashboard["family_count"].as_u64().unwrap() >= 1);
    assert!(dashboard["cluster_count"].as_u64().unwrap() >= 1);
    let family = &dashboard["families"][0];
    let cluster = &family["clusters"][0];
    let cluster_id = cluster["cluster_id"]
        .as_str()
        .expect("cluster id")
        .to_string();
    assert!(cluster["suggested_kind"].as_str().is_some());
    assert!(cluster["anticipated_loc_saved"].as_u64().is_some());
    assert_eq!(cluster["occurrences"][0]["repo_id"], "repo-a");
    assert!(dashboard["scan"]["scanned_at"].as_str().is_some());

    // Propose: files a registry entry + build task, idempotent on re-post.
    let receipt = response_json(
        super::tool_finder::propose(
            State(state.clone()),
            AxumPath(cluster_id.clone()),
            axum::body::Bytes::new(),
        )
        .await,
    )
    .await;
    assert_eq!(receipt["created"], true);
    let tool_id = receipt["tool_id"].as_str().expect("tool id").to_string();
    let task_id = receipt["task_id"].as_str().expect("task id").to_string();
    let registry_text =
        std::fs::read_to_string(registry_dir.path().join("tools-registry.toml")).unwrap();
    assert!(registry_text.contains(&format!("origin_cluster = \"{cluster_id}\"")));
    assert!(registry_text.contains("status = \"proposed\""));
    assert!(
        registry_text.starts_with("schema_version"),
        "append preserves header"
    );
    assert!(
        registry_dir
            .path()
            .join("tasks")
            .join(format!("{task_id}-{tool_id}.toml"))
            .is_file()
    );

    let replay = response_json(
        super::tool_finder::propose(
            State(state.clone()),
            AxumPath(cluster_id.clone()),
            axum::body::Bytes::new(),
        )
        .await,
    )
    .await;
    assert_eq!(replay["created"], false);
    assert_eq!(replay["tool_id"], tool_id);

    // Unknown cluster ids get a typed not-found.
    let missing = response_json(
        super::tool_finder::propose(
            State(state.clone()),
            AxumPath("toolbuild-doesnotexist".to_string()),
            axum::body::Bytes::new(),
        )
        .await,
    )
    .await;
    assert_eq!(missing["code"], "tool_finder_cluster_not_found");
}
