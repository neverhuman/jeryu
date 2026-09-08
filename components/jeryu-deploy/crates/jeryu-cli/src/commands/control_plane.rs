//! API-backed JMCP/control-plane command adapters.

use std::io::Write;

use serde_json::{Value, json};

use crate::cli::{ArtifactsCommands, RepoGraphCommands, RunnersCommands, ToolFinderCommands};
use crate::client::{ClientError, ClientResult};
use crate::commands::api::ApiClient;
use crate::commands::render;

pub(crate) fn run_status(
    json_output: bool,
    api_url: Option<&str>,
    out: &mut dyn Write,
) -> ClientResult<()> {
    let value = api(api_url)?.get("/api/v1/control-plane/status")?;
    let summary = value.get("summary").unwrap_or(&Value::Null);
    let human = format!(
        "control plane: repos={} priorities={} mirror={} artifacts={} runners={}",
        number(summary, "repoCount"),
        number(summary, "priorityCount"),
        text(summary, "mirrorState"),
        text(summary, "artifactState"),
        text(summary, "runnerState")
    );
    render(out, json_output, &value, &human)
}

pub(crate) fn run_priorities(
    json_output: bool,
    api_url: Option<&str>,
    limit: Option<usize>,
    out: &mut dyn Write,
) -> ClientResult<()> {
    let path = match limit {
        Some(limit) => format!("/api/v1/control-plane/priorities?limit={}", limit.max(1)),
        None => "/api/v1/control-plane/priorities".to_string(),
    };
    let value = api(api_url)?.get(&path)?;
    let count = value.as_array().map_or(0, Vec::len);
    render(
        out,
        json_output,
        &value,
        &format!("control-plane priorities: {count} item(s)"),
    )
}

pub(crate) fn run_repo_graph(
    json_output: bool,
    api_url: Option<&str>,
    command: RepoGraphCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match command {
        RepoGraphCommands::Clusters {
            cluster_kind,
            limit,
        } => {
            let mut params = Vec::new();
            if let Some(kind) = cluster_kind {
                params.push(format!("cluster_kind={kind}"));
            }
            if let Some(limit) = limit {
                params.push(format!("limit={}", limit.max(1)));
            }
            let path = if params.is_empty() {
                "/api/v1/control-plane/repo-graph".to_string()
            } else {
                format!("/api/v1/control-plane/repo-graph?{}", params.join("&"))
            };
            let graph = api(api_url)?.get(&path)?;
            let clusters = graph
                .get("clusters")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new()));
            let value = json!({
                "schemaVersion": graph.get("schemaVersion").cloned().unwrap_or(Value::Null),
                "clusters": clusters,
                "insights": graph.get("insights").cloned().unwrap_or(Value::Array(Vec::new())),
            });
            let count = value
                .get("clusters")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            render(
                out,
                json_output,
                &value,
                &format!("repo graph clusters: {count} item(s)"),
            )
        }
    }
}

pub(crate) fn run_artifacts(
    json_output: bool,
    api_url: Option<&str>,
    command: ArtifactsCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match command {
        ArtifactsCommands::Latest { repo } => {
            let path = match repo {
                Some(repo) => format!("/api/v1/control-plane/artifacts/latest?repo={repo}"),
                None => "/api/v1/control-plane/artifacts/latest".to_string(),
            };
            let value = api(api_url)?.get(&path)?;
            render(
                out,
                json_output,
                &value,
                &format!(
                    "latest artifacts: state={} absenceIsSuccess={}",
                    text(&value, "state"),
                    bool_text(&value, "absenceIsSuccess")
                ),
            )
        }
    }
}

pub(crate) fn run_runners(
    json_output: bool,
    api_url: Option<&str>,
    command: RunnersCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match command {
        RunnersCommands::Status => {
            let value = api(api_url)?.get("/api/v1/control-plane/runners")?;
            let local = value.get("local").unwrap_or(&Value::Null);
            render(
                out,
                json_output,
                &value,
                &format!(
                    "runner fabric: online={} offline={} activeSlots={}",
                    number(local, "onlineRunners"),
                    number(local, "offlineRunners"),
                    number(local, "activeSlots")
                ),
            )
        }
    }
}

pub(crate) fn run_tool_finder(
    json_output: bool,
    api_url: Option<&str>,
    command: ToolFinderCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match command {
        ToolFinderCommands::Clusters { repo, limit } => {
            let mut path = format!(
                "/api/v1/codegraph/tool-build/clusters?repo={}",
                urlencode(&repo)
            );
            if let Some(limit) = limit {
                path.push_str(&format!("&top={}", limit.max(1)));
            }
            let value = api(api_url)?.get(&path)?;
            let clusters = match &value {
                Value::Array(items) => items.clone(),
                other => other
                    .get("clusters")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            };
            render(
                out,
                json_output,
                &json!({ "repo": repo, "clusters": clusters }),
                &format!(
                    "tool-finder clusters ({repo}): {} candidate(s)",
                    clusters.len()
                ),
            )
        }
        ToolFinderCommands::Summary => {
            let value = api(api_url)?.get("/api/v1/tools/registry/summary")?;
            let human = format!(
                "tool registry: tools={} (published={} proposed={}) adopting_repos={} open_tasks={} loc_saved={} (+{} anticipated)",
                number(&value, "tool_count"),
                number(&value, "published_count"),
                number(&value, "proposed_count"),
                number(&value, "adopting_repo_count"),
                number(&value, "open_task_count"),
                number(&value, "realized_loc_saved"),
                number(&value, "anticipated_loc_saved"),
            );
            render(out, json_output, &value, &human)
        }
    }
}

/// Minimal percent-encoding for the `/` in a repo id like `family/jeryu-split`.
fn urlencode(value: &str) -> String {
    value.replace('/', "%2F")
}

fn api(api_url: Option<&str>) -> ClientResult<ApiClient> {
    let Some(api_url) = api_url else {
        return Err(ClientError::NotWired(
            "control-plane commands require --api-url or JERYU_API_URL".to_string(),
        ));
    };
    ApiClient::new(api_url)
}

fn number(value: &Value, field: &str) -> u64 {
    value.get(field).and_then(Value::as_u64).unwrap_or(0)
}

fn text(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

fn bool_text(value: &Value, field: &str) -> &'static str {
    match value.get(field).and_then(Value::as_bool) {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    }
}
