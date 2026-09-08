use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::web::WebState;

use super::*;

pub(crate) fn mcp_status(state: &Arc<WebState>) -> Value {
    control_value(snapshot(state), "control-plane snapshot serializes for MCP")
}

pub(crate) fn mcp_priorities(state: &Arc<WebState>, args: &Value) -> Value {
    let mut priorities = snapshot(state).priorities;
    if let Some(limit) = args
        .get("limit")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
    {
        priorities.truncate(limit.max(1));
    }
    json!({ "priorities": priorities })
}

pub(crate) fn mcp_repo_graph_clusters(state: &Arc<WebState>, args: &Value) -> Value {
    let query = RepoGraphQuery {
        repo: None,
        cluster_kind: args
            .get("cluster_kind")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        query: None,
        limit: args
            .get("limit")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok()),
    };
    let graph = repo_graph_response(state, Some(query));
    json!({ "schemaVersion": graph.schema_version, "clusters": graph.clusters })
}

pub(crate) fn mcp_repo_graph_query(state: &Arc<WebState>, args: &Value) -> Value {
    let query = RepoGraphQuery {
        repo: args
            .get("repo")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        cluster_kind: args
            .get("cluster_kind")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        query: args
            .get("query")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        limit: args
            .get("limit")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok()),
    };
    control_value(
        repo_graph_response(state, Some(query)),
        "repo graph serializes for MCP",
    )
}

pub(crate) fn mcp_remote_status() -> Value {
    control_value(remote_status(), "remote status serializes for MCP")
}

pub(crate) fn mcp_artifacts_latest(state: &Arc<WebState>) -> Value {
    control_value(artifacts(state), "artifact status serializes for MCP")
}

pub(crate) fn mcp_runner_fabric_status(state: &Arc<WebState>) -> Value {
    control_value(runner_fabric(state), "runner fabric serializes for MCP")
}

pub(crate) fn mcp_ci_run_jobs(state: &Arc<WebState>, args: &Value) -> Value {
    let ci_run_id = args.get("ci_run_id").cloned().unwrap_or(Value::Null);
    let jobs: Vec<_> = snapshot(state)
        .check_runs
        .into_iter()
        .map(|check| {
            json!({
                "id": check.id,
                "repo": check.repo,
                "name": check.name,
                "head_sha": check.head_sha,
                "status": check.status,
                "conclusion": check.conclusion,
                "state": check.state,
            })
        })
        .collect();
    json!({ "ci_run_id": ci_run_id, "jobs": jobs, "source": "local_jeryu" })
}

pub(crate) fn mcp_ci_bottlenecks(state: &Arc<WebState>, args: &Value) -> Value {
    let snapshot = snapshot(state);
    json!({
        "repo": args.get("repo").cloned().unwrap_or(Value::Null),
        "bottlenecks": snapshot.priorities.iter().filter(|item| {
            matches!(item.severity, InsightSeverity::Critical | InsightSeverity::High)
        }).collect::<Vec<_>>(),
    })
}

pub(crate) fn mcp_explain_blockers(state: &Arc<WebState>, args: &Value) -> Value {
    let priorities = snapshot(state).priorities;
    json!({
        "entity_type": args.get("entity_type").cloned().unwrap_or(Value::Null),
        "entity_id": args.get("entity_id").cloned().unwrap_or(Value::Null),
        "mergeable": priorities.iter().all(|p| !matches!(p.severity, InsightSeverity::Critical | InsightSeverity::High)),
        "blockers": priorities,
    })
}

pub(crate) fn mcp_plan_validation(state: &Arc<WebState>, args: &Value) -> Value {
    let priorities = snapshot(state).priorities;
    let lanes: Vec<String> = priorities
        .iter()
        .map(|priority| priority.proof_lane.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    json!({
        "repo": args.get("repo").cloned().unwrap_or(Value::Null),
        "ref_name": args.get("ref_name").cloned().unwrap_or(Value::Null),
        "lanes": lanes,
        "blockers": priorities,
        "rules_version": RULES_VERSION,
    })
}
