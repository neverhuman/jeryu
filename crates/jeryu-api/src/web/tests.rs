use super::*;
use crate::Method;
use crate::web::markdown::render_markdown;
use crate::web::repositories::repo_list_response;
use crate::web::surface::serialize_payload;
use crate::web::surface::{bootstrap_payload, map_method};
use crate::web::ws::{hello_message, requested_scopes, snapshot_event, unsubscribe_scopes};
use axum::extract::Query;
use jeryu_agentbridge::driver::{AgentDriver, CollectingSink, CommandSpec, stage_editbot};
use jeryu_codegraph::{
    CrateDepRow, GraphSnapshot, SymbolRefRow, SymbolRow, ToolBuildScanConfig,
    scan_tool_build_clusters,
};
use jeryu_core::CheckConclusion;
use jeryu_core::{
    CreateCheckRunRequest, CreatePullRequestRequest, CreateRepositoryRequest,
    SetBranchProtectionRequest,
};
use jeryu_readmodel::contracts::ServerWsMessage;
use jeryu_readmodel::{HealthLevel, sample_read_model};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;

fn write_file(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture parent");
    }
    std::fs::write(path, contents).expect("write fixture file");
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
        "exporting a PR should seed CI check-runs, got {check_runs:?}"
    );
    assert!(
        check_runs
            .check_runs
            .iter()
            .any(|run| run.conclusion == Some(CheckConclusion::Success)),
        "the seeded export CI set should include a green run"
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
    assert_eq!(pack["provenance"]["storage_schema"], "3");
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
            AxumPath((repo.id.to_string(), pr.number)),
        )
        .await,
    )
    .await;
    assert_eq!(detail["summary"]["head_sha"], "head-a");
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

    let missing_pr =
        super::pulls::detail(State(state.clone()), AxumPath((repo.id.to_string(), 404))).await;
    assert_eq!(missing_pr.status(), StatusCode::NOT_FOUND);
    assert_eq!(response_json(missing_pr).await["code"], "not_found");

    let invalid_review = super::pulls::review(
        State(state.clone()),
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
            AxumPath((repo.id.to_string(), pr.number)),
        )
        .await,
    )
    .await;
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
}

#[tokio::test]
async fn pulls_mutations_approve_and_merge_clean_pr() {
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
                title: "merge-ready".to_string(),
                head: "merge-ready".to_string(),
                base: "main".to_string(),
                head_sha: Some("head-merge".to_string()),
                base_sha: Some("base-ready".to_string()),
                changed_files: vec!["src/merge.rs".to_string()],
                ..CreatePullRequestRequest::default()
            },
        )
        .unwrap();
    core.create_check_run(
        "alice",
        "jeryu",
        CreateCheckRunRequest {
            name: "ci/fast".to_string(),
            head_sha: "head-merge".to_string(),
            status: Some(jeryu_core::CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Success),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    let state = Arc::new(WebState::new(core));
    let path = || AxumPath((repo.id.to_string(), pr.number));

    let approved = response_json(
        super::pulls::approve(
            State(state.clone()),
            path(),
            axum::body::Bytes::from(
                serde_json::json!({
                    "expected_head_sha": "head-merge",
                    "body_markdown": "ship it"
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(approved["summary"]["review"]["approvals"], 1);

    let detail = response_json(super::pulls::detail(State(state.clone()), path()).await).await;
    assert_eq!(detail["merge_passport"]["status"], "pass");
    let passport_hash = detail["passport_hash"].as_str().unwrap();

    let merged = response_json(
        super::pulls::merge(
            State(state),
            path(),
            axum::body::Bytes::from(
                serde_json::json!({
                    "expected_head_sha": "head-merge",
                    "expected_passport_hash": passport_hash,
                    "merge_method": "merge",
                    "commit_title": "Merge ready",
                    "commit_message": "route merge"
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(merged["summary"]["state"], "merged");
    assert_eq!(merged["merge_passport"]["head_sha"], "head-merge");
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
async fn workcell_repair_live_legacy_request_uses_failed_run_as_ci_run() {
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

    let response = response_json(
        super::workcells::repair_live(
            State(state),
            axum::body::Bytes::from(serde_json::to_vec(&repair_body).unwrap()),
        )
        .await,
    )
    .await;

    assert_eq!(
        response["held"]["frozen_snapshot"]["ci_run_id"],
        "legacy-run-17"
    );
    assert_eq!(
        response["held"]["frozen_snapshot"]["failed_run_id"],
        "legacy-run-17"
    );
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

    let readme =
        response_json(repo_readme(State(state.clone()), AxumPath(repo.id.to_string())).await).await;
    assert_eq!(readme["markdown"], markdown);
    assert!(readme["html"].as_str().unwrap().contains("Managed README"));

    let blob =
        response_json(repo_blob(State(state.clone()), AxumPath(repo.id.to_string())).await).await;
    assert_eq!(blob["text"], markdown);
    assert!(
        blob["rendered_markdown"]["html"]
            .as_str()
            .unwrap()
            .contains("Managed README")
    );

    let raw = repo_raw(State(state), AxumPath(repo.id.to_string())).await;
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
    assert!(matches!(map_method(&HttpMethod::POST), Some(Method::Post)));
    assert!(matches!(map_method(&HttpMethod::PUT), Some(Method::Put)));
    assert!(map_method(&HttpMethod::DELETE).is_none());
    assert!(map_method(&HttpMethod::PATCH).is_none());
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
        "/repos/alice/jeryu",
        "/repos/alice/jeryu/pulls/99",
        "/repos/alice/jeryu/settings/merge",
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

async fn response_json(response: AxumResponse) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body reads");
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|err| panic!("response body is not JSON ({err}): {bytes:?}"))
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
        "jeryu gh-setup --host http://127.0.0.1:8787 --token JERYU-TOKEN"
    );
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
    assert_eq!(parsed["login"], "jeryu");
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
    let patch = app
        .oneshot(
            Request::builder()
                .method(HttpMethod::PATCH)
                .uri("/repos/alice/jeryu")
                .header(header::USER_AGENT, "curl/8.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::METHOD_NOT_ALLOWED);
    let parsed = response_json(patch).await;
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
        "jeryu gh-setup --host http://127.0.0.1:8787 --token JERYU-TOKEN"
    );
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

    let conn = hub.register();
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
