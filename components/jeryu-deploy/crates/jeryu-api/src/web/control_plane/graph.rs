use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::web::{WebState, server_time};

use super::*;

pub(crate) fn repo_graph_response(
    state: &Arc<WebState>,
    query: Option<RepoGraphQuery>,
) -> RepoGraphResponse {
    let core = state.github.core();
    let repos = collect_repos(core);
    let pull_requests = collect_pull_requests(core, &repos);
    let check_runs = collect_check_runs(core, &repos);
    let codegraph = codegraph_summary(state);
    let tool_build = tool_build_summary(state);
    let runners = runner_fabric(state);
    let mirror = remote_status();
    let mut graph = build_repo_graph(
        &repos,
        &pull_requests,
        &check_runs,
        &codegraph,
        &tool_build,
        &runners,
        &mirror,
    );
    if let Some(query) = query {
        filter_graph(&mut graph, query);
    }
    graph
}

fn build_repo_graph(
    repos: &[ControlRepo],
    prs: &[ControlPullRequest],
    checks: &[ControlCheckRun],
    codegraph: &CodegraphControlSummary,
    tool_build: &ToolBuildControlSummary,
    runners: &RunnerFabricResponse,
    mirror: &RemoteStatusResponse,
) -> RepoGraphResponse {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for repo in repos {
        let mut metadata = BTreeMap::new();
        metadata.insert("owner".to_string(), repo.owner.clone());
        metadata.insert("defaultBranch".to_string(), repo.default_branch.clone());
        nodes.push(GraphNode {
            id: format!("repo:{}", repo.full_name),
            label: repo.full_name.clone(),
            kind: "repo".to_string(),
            state: repo.state.clone(),
            weight: 1.0 + repo.open_pull_requests as f64,
            metadata,
        });
    }
    for pr in prs {
        let pr_id = format!("pr:{}#{}", pr.repo, pr.number);
        nodes.push(GraphNode {
            id: pr_id.clone(),
            label: format!("{}#{}", pr.repo, pr.number),
            kind: "pull_request".to_string(),
            state: pr.state_evidence.clone(),
            weight: 1.0 + pr.checks.total as f64,
            metadata: BTreeMap::from([
                ("headSha".to_string(), pr.head_sha.clone()),
                ("baseRef".to_string(), pr.base_ref.clone()),
            ]),
        });
        edges.push(GraphEdge {
            source: format!("repo:{}", pr.repo),
            target: pr_id,
            kind: "has_pr".to_string(),
            state: pr.state_evidence.clone(),
            weight: 1.0,
        });
    }
    for check in checks {
        let check_id = format!("check:{}", check.id);
        nodes.push(GraphNode {
            id: check_id.clone(),
            label: check.name.clone(),
            kind: "check_run".to_string(),
            state: check.state.clone(),
            weight: if check.state == EvidenceState::Failed {
                3.0
            } else {
                1.0
            },
            metadata: BTreeMap::from([
                ("repo".to_string(), check.repo.clone()),
                ("headSha".to_string(), check.head_sha.clone()),
            ]),
        });
        edges.push(GraphEdge {
            source: format!("repo:{}", check.repo),
            target: check_id,
            kind: "has_check".to_string(),
            state: check.state.clone(),
            weight: 1.0,
        });
    }
    nodes.push(GraphNode {
        id: "runner:fabric".to_string(),
        label: "Runner fabric".to_string(),
        kind: "runner_capacity".to_string(),
        state: runners.local.state.clone(),
        weight: f64::from(runners.local.active_slots.max(1)),
        metadata: BTreeMap::from([(
            "utilization".to_string(),
            format!("{:.2}", runners.local.utilization),
        )]),
    });
    nodes.push(GraphNode {
        id: "mirror:github".to_string(),
        label: "GitHub mirror".to_string(),
        kind: "remote_mirror".to_string(),
        state: mirror.state.clone(),
        weight: 1.0,
        metadata: BTreeMap::new(),
    });

    let mut clusters = Vec::new();
    let mut insights = Vec::new();
    clusters.push(GraphCluster {
        id: "cluster:ownership-test-lanes".to_string(),
        label: "Ownership and proof lanes".to_string(),
        kind: "ownership_test_lane".to_string(),
        state: EvidenceState::Fresh,
        severity: InsightSeverity::Info,
        node_ids: repos
            .iter()
            .map(|repo| format!("repo:{}", repo.full_name))
            .collect(),
        insights: vec![
            "owner-map and test-map route public paths to local proof lanes".to_string(),
        ],
    });
    if checks
        .iter()
        .any(|check| check.state == EvidenceState::Failed)
    {
        clusters.push(GraphCluster {
            id: "cluster:ci-blockers".to_string(),
            label: "CI blockers".to_string(),
            kind: "ci_blocker".to_string(),
            state: EvidenceState::Failed,
            severity: InsightSeverity::High,
            node_ids: checks
                .iter()
                .filter(|check| check.state == EvidenceState::Failed)
                .map(|check| format!("check:{}", check.id))
                .collect(),
            insights: vec!["failing check-runs block PR and release confidence".to_string()],
        });
    }
    clusters.push(GraphCluster {
        id: "cluster:runner-capacity".to_string(),
        label: "Runner capacity".to_string(),
        kind: "runner_capacity".to_string(),
        state: runners.local.state.clone(),
        severity: if runners.local.offline_runners > 0 {
            InsightSeverity::High
        } else {
            InsightSeverity::Info
        },
        node_ids: vec!["runner:fabric".to_string()],
        insights: vec![format!(
            "{} online runner(s), {} active slot(s)",
            runners.local.online_runners, runners.local.active_slots
        )],
    });
    clusters.push(GraphCluster {
        id: "cluster:superseded-mirror".to_string(),
        label: "Mirror evidence".to_string(),
        kind: "superseded_mirror".to_string(),
        state: mirror.state.clone(),
        severity: InsightSeverity::Medium,
        node_ids: vec!["mirror:github".to_string()],
        insights: vec![mirror.divergence.reason.clone()],
    });
    if tool_build.cluster_count > 0 {
        clusters.push(GraphCluster {
            id: "cluster:tool-build".to_string(),
            label: "Repeated-code tool-build clusters".to_string(),
            kind: "tool_build".to_string(),
            state: EvidenceState::Fresh,
            severity: InsightSeverity::Low,
            node_ids: tool_build
                .top_clusters
                .iter()
                .map(|cluster| format!("tool-build:{}", cluster.cluster_id))
                .collect(),
            insights: tool_build
                .top_clusters
                .iter()
                .map(|cluster| cluster.insight.clone())
                .collect(),
        });
    }
    clusters.push(GraphCluster {
        id: "cluster:codegraph-freshness".to_string(),
        label: "Codegraph freshness".to_string(),
        kind: "codegraph_freshness".to_string(),
        state: codegraph.state.clone(),
        severity: if codegraph.state == EvidenceState::Fresh {
            InsightSeverity::Info
        } else {
            InsightSeverity::Low
        },
        node_ids: repos
            .iter()
            .map(|repo| format!("repo:{}", repo.full_name))
            .collect(),
        insights: vec![codegraph.reason.clone()],
    });
    for cluster in &clusters {
        insights.push(GraphInsight {
            id: format!("insight:{}", cluster.id.trim_start_matches("cluster:")),
            cluster_id: cluster.id.clone(),
            title: cluster.label.clone(),
            evidence: cluster.insights.clone(),
        });
    }
    RepoGraphResponse {
        schema_version: "jeryu.repo_graph/v1".to_string(),
        generated_at: server_time(),
        nodes,
        edges,
        clusters,
        insights,
    }
}

fn filter_graph(graph: &mut RepoGraphResponse, query: RepoGraphQuery) {
    if let Some(kind) = query.cluster_kind {
        graph.clusters.retain(|cluster| cluster.kind == kind);
    }
    if let Some(repo) = query.repo {
        let repo_node = format!("repo:{repo}");
        let mut keep = BTreeSet::from([repo_node.clone()]);
        for edge in &graph.edges {
            if edge.source == repo_node {
                keep.insert(edge.target.clone());
            }
        }
        graph.nodes.retain(|node| keep.contains(&node.id));
        graph
            .edges
            .retain(|edge| keep.contains(&edge.source) && keep.contains(&edge.target));
        graph.clusters.retain(|cluster| {
            cluster
                .node_ids
                .iter()
                .any(|node_id| keep.contains(node_id))
        });
    }
    if let Some(text) = query.query {
        let needle = text.to_ascii_lowercase();
        graph.nodes.retain(|node| {
            node.id.to_ascii_lowercase().contains(&needle)
                || node.label.to_ascii_lowercase().contains(&needle)
                || node.kind.to_ascii_lowercase().contains(&needle)
        });
    }
    if let Some(limit) = query.limit {
        let limit = limit.max(1);
        graph.nodes.truncate(limit);
        graph.clusters.truncate(limit);
        graph.insights.truncate(limit);
    }
}
