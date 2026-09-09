use std::sync::Arc;

use chrono::Utc;
use jeryu_agent_stream::{AgentOutputStream, AgentRunStreamKey, AgentTtyEvent};
use jeryu_core::{
    CheckConclusion, CheckRun, CheckRunStatus, CreateCheckRunRequest, CreatePullRequestRequest,
    CreateRepositoryRequest, ForgeCore, check_conclusion_wire_value,
};
use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::web::WebState;

fn seeded_state() -> Arc<WebState> {
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
            head_sha: Some("head-no-checks".to_string()),
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
            status: Some(CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Failure),
            ..CreateCheckRunRequest::default()
        },
    )
    .unwrap();
    Arc::new(WebState::new(core))
}

#[test]
fn priority_rules_rank_missing_pr_checks_and_failing_ci() {
    let snapshot = snapshot(&seeded_state());
    let ids: Vec<&str> = snapshot
        .priorities
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert!(
        ids.iter().any(|id| id.contains("checks-missing")),
        "missing PR head checks must be explicit priority evidence"
    );
    assert!(ids.contains(&"ci-failing-checks"));
    assert_eq!(snapshot.priorities[0].rules_version, RULES_VERSION);
    assert!(snapshot.priorities[0].score >= snapshot.priorities[1].score);
}

#[test]
fn artifacts_absence_is_not_success() {
    let response = artifacts(&seeded_state());
    assert_eq!(response.state, EvidenceState::Missing);
    assert!(!response.absence_is_success);
    assert_eq!(response.latest_release.artifact_count, 0);
}

#[test]
fn mirror_degrades_explicitly_when_unavailable() {
    let remote = remote_status();
    assert_eq!(remote.state, EvidenceState::Missing);
    assert_eq!(remote.divergence.state, EvidenceState::Unknown);
    assert!(remote.divergence.reason.contains("unknown"));
}

#[test]
fn repo_graph_contains_ci_runner_and_mirror_clusters() {
    let graph = repo_graph_response(&seeded_state(), None);
    assert!(graph.nodes.iter().any(|node| node.kind == "repo"));
    assert!(
        graph
            .clusters
            .iter()
            .any(|cluster| cluster.kind == "ci_blocker")
    );
    assert!(
        graph
            .clusters
            .iter()
            .any(|cluster| cluster.kind == "runner_capacity")
    );
    assert!(
        graph
            .clusters
            .iter()
            .any(|cluster| cluster.kind == "superseded_mirror")
    );
}

#[test]
fn runner_fabric_reports_local_capacity() {
    let state = seeded_state();
    let runners = runner_fabric(&state);
    assert_eq!(runners.local.state, EvidenceState::Fresh);
    assert!(runners.local.total_slots >= runners.local.active_slots);
    assert_eq!(runners.mirror.state, EvidenceState::Missing);
}

#[test]
fn mcp_facade_returns_limited_graph_jobs_and_blockers() {
    let state = seeded_state();

    let status = mcp_status(&state);
    assert_eq!(status["schemaVersion"], SCHEMA_VERSION);
    assert_eq!(status["localAuthority"]["state"], "fresh");

    let priorities = mcp_priorities(&state, &json!({ "limit": 1 }));
    assert_eq!(priorities["priorities"].as_array().unwrap().len(), 1);

    let clusters = mcp_repo_graph_clusters(
        &state,
        &json!({ "cluster_kind": "runner_capacity", "limit": 1 }),
    );
    assert_eq!(clusters["clusters"].as_array().unwrap().len(), 1);
    assert_eq!(clusters["clusters"][0]["kind"], "runner_capacity");

    let graph = mcp_repo_graph_query(
        &state,
        &json!({
            "repo": "alice/jeryu",
            "query": "feature",
            "limit": 3
        }),
    );
    assert_eq!(graph["schemaVersion"], "jeryu.repo_graph/v1");
    assert!(graph["nodes"].as_array().unwrap().len() <= 3);

    let remote = mcp_remote_status();
    assert_eq!(remote["state"], "missing");
    let artifacts = mcp_artifacts_latest(&state);
    assert_eq!(artifacts["absenceIsSuccess"], false);
    let runners = mcp_runner_fabric_status(&state);
    assert_eq!(runners["local"]["state"], "fresh");

    let jobs = mcp_ci_run_jobs(&state, &json!({ "ci_run_id": "run-1" }));
    assert_eq!(jobs["ci_run_id"], "run-1");
    assert_eq!(jobs["jobs"].as_array().unwrap().len(), 1);

    let bottlenecks = mcp_ci_bottlenecks(&state, &json!({ "repo": "alice/jeryu" }));
    assert_eq!(bottlenecks["repo"], "alice/jeryu");
    assert!(!bottlenecks["bottlenecks"].as_array().unwrap().is_empty());

    let blockers = mcp_explain_blockers(
        &state,
        &json!({ "entity_type": "pull_request", "entity_id": "alice/jeryu#1" }),
    );
    assert_eq!(blockers["mergeable"], false);
    assert_eq!(blockers["entity_type"], "pull_request");

    let plan = mcp_plan_validation(
        &state,
        &json!({ "repo": "alice/jeryu", "ref_name": "feature" }),
    );
    assert_eq!(plan["rules_version"], RULES_VERSION);
    assert!(!plan["lanes"].as_array().unwrap().is_empty());
}

#[test]
fn helper_branches_normalize_tty_time_and_check_states() {
    let run = AgentRunStreamKey {
        repo: Some("alice/jeryu".to_string()),
        workcell_id: "wc-1".to_string(),
        agent_run_id: "run-1".to_string(),
        agent: "codex".to_string(),
        model: "gpt-5".to_string(),
    };
    let events: Vec<_> = (0..7)
        .map(|seq| {
            AgentTtyEvent::text(
                seq,
                1_700_000_000_000 + seq,
                &run,
                AgentOutputStream::Stdout,
                format!("line-{seq}\n"),
            )
        })
        .collect();
    let preview = tty_preview_lines(&events);
    assert_eq!(preview.len(), 5);
    assert_eq!(preview[0], "line-2");
    assert_eq!(task_label("/usr/bin/codex"), "codex");
    assert_eq!(task_label("/"), "/");
    assert!(rfc3339_from_ms(0).starts_with("1970-01-01T00:00:00"));
    assert_eq!(rfc3339_from_ms(u64::MAX), u64::MAX.to_string());
    assert_eq!(normalize_node_state(""), "unknown");
    assert_eq!(normalize_node_state("ready"), "ready");

    let checks = vec![
        CheckRun {
            id: Uuid::from_u128(1),
            owner: "alice".to_string(),
            repo: "jeryu".to_string(),
            name: "queued".to_string(),
            head_sha: "head".to_string(),
            status: CheckRunStatus::Queued,
            conclusion: None,
            started_at: Utc::now(),
            completed_at: None,
            details_url: None,
            output: None,
        },
        CheckRun {
            id: Uuid::from_u128(2),
            owner: "alice".to_string(),
            repo: "jeryu".to_string(),
            name: "running".to_string(),
            head_sha: "head".to_string(),
            status: CheckRunStatus::InProgress,
            conclusion: None,
            started_at: Utc::now(),
            completed_at: None,
            details_url: None,
            output: None,
        },
        CheckRun {
            id: Uuid::from_u128(3),
            owner: "alice".to_string(),
            repo: "jeryu".to_string(),
            name: "pass".to_string(),
            head_sha: "head".to_string(),
            status: CheckRunStatus::Completed,
            conclusion: Some(CheckConclusion::Success),
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            details_url: None,
            output: None,
        },
        CheckRun {
            id: Uuid::from_u128(4),
            owner: "alice".to_string(),
            repo: "jeryu".to_string(),
            name: "fail".to_string(),
            head_sha: "head".to_string(),
            status: CheckRunStatus::Completed,
            conclusion: Some(CheckConclusion::TimedOut),
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            details_url: None,
            output: None,
        },
    ];
    let summary = summarize_checks(&checks);
    assert_eq!(summary.queued, 1);
    assert_eq!(summary.running, 1);
    assert_eq!(summary.successful, 1);
    assert_eq!(summary.failing, 1);
    assert_eq!(check_state(&checks[0]), EvidenceState::Queued);
    assert_eq!(check_state(&checks[1]), EvidenceState::Fresh);
    assert_eq!(check_state(&checks[3]), EvidenceState::Failed);
    assert_eq!(check_status(&CheckRunStatus::Queued), "queued");
    assert_eq!(check_status(&CheckRunStatus::InProgress), "in_progress");
    assert_eq!(check_status(&CheckRunStatus::Completed), "completed");
    assert_eq!(
        check_conclusion(&CheckConclusion::ActionRequired),
        "action_required"
    );
    assert_eq!(check_conclusion(&CheckConclusion::Cancelled), "cancelled");
    assert_eq!(check_conclusion(&CheckConclusion::Failure), "failure");
    assert_eq!(check_conclusion(&CheckConclusion::Neutral), "neutral");
    assert_eq!(check_conclusion(&CheckConclusion::Success), "success");
    assert_eq!(check_conclusion(&CheckConclusion::Skipped), "skipped");
    assert_eq!(
        check_conclusion(&CheckConclusion::Superseded),
        check_conclusion_wire_value(&CheckConclusion::Superseded)
    );
    assert_eq!(check_conclusion(&CheckConclusion::TimedOut), "timed_out");
}
