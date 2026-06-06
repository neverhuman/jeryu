//! JMCP/control-plane intelligence read model.
//!
//! This module is deliberately pure aggregation over already-owned local
//! surfaces: [`ForgeCore`], the runner fleet snapshot, the live agent-run store,
//! and the auxiliary codegraph/tool-build store. GitHub mirror data is optional
//! read-only evidence and is represented here as explicit `missing` state until
//! a live mirror adapter supplies it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, Utc};
use jeryu_core::{CheckConclusion, CheckRun, CheckRunStatus, ForgeCore, PullRequestState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::WebState;
use super::agent_runs::{AgentRunSourceSnapshot, AgentRunState, AgentRunStatusResponse};
use super::server_time;
use super::workcells_support::manager;

const SCHEMA_VERSION: &str = "jeryu.control_plane/v1";
const RULES_VERSION: &str = "rules-v1";
const MIRROR_DOCS: &str = "docs/agent-native-standard.md";
const ARTIFACT_DOCS: &str = "docs/release.md#release-receipt";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum EvidenceState {
    Fresh,
    Missing,
    Queued,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum InsightSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ControlPlaneSnapshot {
    pub schema_version: String,
    pub generated_at: String,
    pub local_authority: LocalAuthority,
    pub summary: ControlPlaneSummary,
    pub repos: Vec<ControlRepo>,
    pub pull_requests: Vec<ControlPullRequest>,
    pub check_runs: Vec<ControlCheckRun>,
    pub workflows: Vec<ControlWorkflow>,
    pub releases: ControlReleaseSummary,
    pub artifacts: ArtifactLatestResponse,
    pub runners: RunnerFabricResponse,
    pub workcells: Value,
    pub agent_runs: Vec<Value>,
    pub codegraph: CodegraphControlSummary,
    pub tool_build: ToolBuildControlSummary,
    pub mcp: McpToolHealth,
    pub mirror: RemoteStatusResponse,
    pub priorities: Vec<PriorityInsight>,
    pub repo_graph: RepoGraphResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct LocalAuthority {
    pub source_of_truth: String,
    pub state: EvidenceState,
    pub docs_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ControlPlaneSummary {
    pub repo_count: usize,
    pub open_pr_count: usize,
    pub draft_pr_count: usize,
    pub queued_check_count: usize,
    pub running_check_count: usize,
    pub failing_check_count: usize,
    pub missing_check_pr_count: usize,
    pub priority_count: usize,
    pub critical_priority_count: usize,
    pub high_priority_count: usize,
    pub mirror_state: EvidenceState,
    pub artifact_state: EvidenceState,
    pub runner_state: EvidenceState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ControlRepo {
    pub id: String,
    pub full_name: String,
    pub owner: String,
    pub name: String,
    pub default_branch: String,
    pub private: bool,
    pub archived: bool,
    pub disabled: bool,
    pub open_pull_requests: usize,
    pub draft_pull_requests: usize,
    pub queued_checks: usize,
    pub running_checks: usize,
    pub failing_checks: usize,
    pub latest_head_sha: Option<String>,
    pub state: EvidenceState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ControlPullRequest {
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub draft: bool,
    pub state: String,
    pub head_ref: String,
    pub head_sha: String,
    pub base_ref: String,
    pub base_sha: String,
    pub mergeable: bool,
    pub mergeable_state: String,
    pub changed_files: Vec<String>,
    pub checks: CheckSummary,
    pub state_evidence: EvidenceState,
    pub source_links: Vec<SourceLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct CheckSummary {
    pub total: usize,
    pub queued: usize,
    pub running: usize,
    pub failing: usize,
    pub successful: usize,
    pub missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ControlCheckRun {
    pub id: String,
    pub repo: String,
    pub name: String,
    pub head_sha: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub details_url: Option<String>,
    pub state: EvidenceState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ControlWorkflow {
    pub id: String,
    pub repo: String,
    pub name: String,
    pub head_sha: String,
    pub state: EvidenceState,
    pub check_run_id: String,
    pub jobs_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ControlReleaseSummary {
    pub state: EvidenceState,
    pub latest_release: Option<String>,
    pub release_count: usize,
    pub reason: String,
    pub docs_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ArtifactLatestResponse {
    pub schema_version: String,
    pub state: EvidenceState,
    pub latest_build: ArtifactEvidence,
    pub latest_release: ArtifactEvidence,
    pub mirror_artifacts: ArtifactEvidence,
    pub docs_url: String,
    pub absence_is_success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ArtifactEvidence {
    pub state: EvidenceState,
    pub artifact_count: usize,
    pub reason: String,
    pub source_links: Vec<SourceLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct RunnerFabricResponse {
    pub schema_version: String,
    pub local: RunnerLocalFabric,
    pub mirror: MirrorEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct RunnerLocalFabric {
    pub state: EvidenceState,
    pub nodes: u32,
    pub online_runners: u32,
    pub offline_runners: u32,
    pub busy_runners: u32,
    pub idle_runners: u32,
    pub total_slots: u32,
    pub active_slots: u32,
    pub utilization: f64,
    pub last_updated: Option<String>,
    pub node_details: Vec<RunnerNodeSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct RunnerNodeSummary {
    pub runner_id: String,
    pub source: String,
    pub state: String,
    pub capacity: u32,
    pub in_flight: u32,
    pub labels: Vec<String>,
    pub classes: Vec<String>,
    pub active_task_count: u32,
    pub last_updated: Option<String>,
    pub active_tasks: Vec<RunnerTaskSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct RunnerTaskSummary {
    pub task_id: String,
    pub job_id: String,
    pub agent_run_id: Option<String>,
    pub workcell_id: Option<String>,
    pub repo: Option<String>,
    pub label: String,
    pub program: String,
    pub state: String,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub tty_preview: RunnerTtyPreview,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct RunnerTtyPreview {
    pub state: EvidenceState,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct CodegraphControlSummary {
    pub state: EvidenceState,
    pub indexed_symbols: usize,
    pub indexed_references: usize,
    pub crate_edges: usize,
    pub indexed_files: usize,
    pub latest_index_run: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ToolBuildControlSummary {
    pub state: EvidenceState,
    pub cluster_count: usize,
    pub ignored_count: usize,
    pub top_clusters: Vec<ToolBuildClusterSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ToolBuildClusterSummary {
    pub cluster_id: String,
    pub repo_id: String,
    pub score: u64,
    pub occurrence_count: usize,
    pub file_count: usize,
    pub insight: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct McpToolHealth {
    pub state: EvidenceState,
    pub tool_count: usize,
    pub live_backed_tools: Vec<String>,
    pub degraded_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct RemoteStatusResponse {
    pub schema_version: String,
    pub state: EvidenceState,
    pub mirrors: Vec<MirrorEvidence>,
    pub divergence: MirrorDivergence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct MirrorEvidence {
    pub name: String,
    pub state: EvidenceState,
    pub reason: String,
    pub docs_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct MirrorDivergence {
    pub state: EvidenceState,
    pub reason: String,
    pub local_default_branches: Vec<SourceLink>,
    pub mirror_default_branches: Vec<SourceLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct PriorityInsight {
    pub id: String,
    pub title: String,
    pub severity: InsightSeverity,
    pub score: u32,
    pub confidence: f64,
    pub owner: String,
    pub proof_lane: String,
    pub recommended_action: String,
    pub evidence: Vec<String>,
    pub source_links: Vec<SourceLink>,
    pub state: EvidenceState,
    pub rules_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct SourceLink {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct RepoGraphResponse {
    pub schema_version: String,
    pub generated_at: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub clusters: Vec<GraphCluster>,
    pub insights: Vec<GraphInsight>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub state: EvidenceState,
    pub weight: f64,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub state: EvidenceState,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct GraphCluster {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub state: EvidenceState,
    pub severity: InsightSeverity,
    pub node_ids: Vec<String>,
    pub insights: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct GraphInsight {
    pub id: String,
    pub cluster_id: String,
    pub title: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct PriorityQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct RepoGraphQuery {
    pub repo: Option<String>,
    pub cluster_kind: Option<String>,
    pub query: Option<String>,
    pub limit: Option<usize>,
}

pub(super) async fn status(State(state): State<Arc<WebState>>) -> Json<ControlPlaneSnapshot> {
    Json(snapshot(&state))
}

pub(super) async fn priorities(
    State(state): State<Arc<WebState>>,
    Query(query): Query<PriorityQuery>,
) -> Json<Vec<PriorityInsight>> {
    let mut priorities = snapshot(&state).priorities;
    if let Some(limit) = query.limit {
        priorities.truncate(limit.max(1));
    }
    Json(priorities)
}

pub(super) async fn repo_graph(
    State(state): State<Arc<WebState>>,
    Query(query): Query<RepoGraphQuery>,
) -> Json<RepoGraphResponse> {
    Json(repo_graph_response(&state, Some(query)))
}

pub(super) async fn artifacts_latest(
    State(state): State<Arc<WebState>>,
) -> Json<ArtifactLatestResponse> {
    Json(artifacts(&state))
}

pub(super) async fn runners(State(state): State<Arc<WebState>>) -> Json<RunnerFabricResponse> {
    Json(runner_fabric(&state))
}

pub(super) fn snapshot(state: &Arc<WebState>) -> ControlPlaneSnapshot {
    let core = state.github.core();
    let repos = collect_repos(core);
    let pull_requests = collect_pull_requests(core, &repos);
    let check_runs = collect_check_runs(core, &repos);
    let workflows = collect_workflows(&check_runs);
    let artifacts = artifacts(state);
    let runners = runner_fabric(state);
    let codegraph = codegraph_summary(state);
    let tool_build = tool_build_summary(state);
    let mcp = mcp_health();
    let mirror = remote_status();
    let workcells = control_value(
        super::workcells::live_tui(state).workcells,
        "workcell dashboard serializes for control-plane snapshot",
    );
    let agent_runs = state.agent_runs.list_json();
    let repo_graph = repo_graph_response(state, None);
    let priorities = priority_insights(PriorityInputs {
        repos: &repos,
        prs: &pull_requests,
        checks: &check_runs,
        runners: &runners,
        artifacts: &artifacts,
        mirror: &mirror,
        codegraph: &codegraph,
        tool_build: &tool_build,
    });
    let summary = summary(
        &repos,
        &pull_requests,
        &check_runs,
        priorities.as_slice(),
        &artifacts,
        &runners,
        &mirror,
    );
    ControlPlaneSnapshot {
        schema_version: SCHEMA_VERSION.to_string(),
        generated_at: server_time(),
        local_authority: LocalAuthority {
            source_of_truth: "local_jeryu".to_string(),
            state: EvidenceState::Fresh,
            docs_url: "docs/architecture.md".to_string(),
        },
        summary,
        repos,
        pull_requests,
        check_runs,
        workflows,
        releases: releases(),
        artifacts,
        runners,
        workcells,
        agent_runs,
        codegraph,
        tool_build,
        mcp,
        mirror,
        priorities,
        repo_graph,
    }
}

pub(super) fn artifacts(_state: &Arc<WebState>) -> ArtifactLatestResponse {
    ArtifactLatestResponse {
        schema_version: "jeryu.artifacts.latest/v1".to_string(),
        state: EvidenceState::Missing,
        latest_build: ArtifactEvidence {
            state: EvidenceState::Missing,
            artifact_count: 0,
            reason: "local build artifacts are not stored in the forge read model yet".to_string(),
            source_links: vec![SourceLink {
                label: "artifact evidence requirements".to_string(),
                url: ARTIFACT_DOCS.to_string(),
            }],
        },
        latest_release: ArtifactEvidence {
            state: EvidenceState::Missing,
            artifact_count: 0,
            reason:
                "releases are read-only compatibility responses and not durable domain state yet"
                    .to_string(),
            source_links: vec![SourceLink {
                label: "release receipt".to_string(),
                url: "docs/release.md#release-receipt".to_string(),
            }],
        },
        mirror_artifacts: ArtifactEvidence {
            state: EvidenceState::Missing,
            artifact_count: 0,
            reason: "optional GitHub mirror artifact adapter is not configured".to_string(),
            source_links: vec![SourceLink {
                label: "agent-native standard".to_string(),
                url: MIRROR_DOCS.to_string(),
            }],
        },
        docs_url: ARTIFACT_DOCS.to_string(),
        absence_is_success: false,
    }
}

pub(super) fn runner_fabric(state: &Arc<WebState>) -> RunnerFabricResponse {
    let fleet = jeryu_runnerd::RunnerFleet::deterministic_fixture();
    runner_fabric_from_parts(state, fleet.snapshot(), fleet.health())
}

fn runner_fabric_from_parts(
    state: &Arc<WebState>,
    fleet: jeryu_runnerd::RunnerFleetSnapshot,
    node_health: Vec<jeryu_runnerd::FleetNodeHealth>,
) -> RunnerFabricResponse {
    let workcells = manager(state).workcells();
    let agent_runs = state.agent_runs.list();
    let node_details = build_runner_nodes(node_health, &workcells, &agent_runs);
    let last_updated = node_details
        .iter()
        .filter_map(|node| node.last_updated.as_ref())
        .max()
        .cloned();
    let utilization = if fleet.active_slots == 0 {
        0.0
    } else {
        f64::from(fleet.busy_runners) / f64::from(fleet.active_slots)
    };
    RunnerFabricResponse {
        schema_version: "jeryu.runner_fabric/v1".to_string(),
        local: RunnerLocalFabric {
            state: if node_details.is_empty() {
                EvidenceState::Unknown
            } else {
                EvidenceState::Fresh
            },
            nodes: fleet.nodes,
            online_runners: fleet.online_runners,
            offline_runners: fleet.stuck_runners,
            busy_runners: fleet.busy_runners,
            idle_runners: fleet.idle_runners,
            total_slots: fleet.total_slots,
            active_slots: fleet.active_slots,
            utilization,
            last_updated,
            node_details,
        },
        mirror: MirrorEvidence {
            name: "github_actions_runners".to_string(),
            state: EvidenceState::Missing,
            reason: "optional GitHub mirror runner adapter is not configured".to_string(),
            docs_url: MIRROR_DOCS.to_string(),
        },
    }
}

fn build_runner_nodes(
    node_health: Vec<jeryu_runnerd::FleetNodeHealth>,
    workcells: &[jeryu_runnerd::WorkcellLease],
    agent_runs: &[AgentRunStatusResponse],
) -> Vec<RunnerNodeSummary> {
    let mut nodes: BTreeMap<String, RunnerNodeSummary> = node_health
        .into_iter()
        .map(|node| {
            let state = normalize_node_state(&node.state);
            (
                node.runner_id.clone(),
                RunnerNodeSummary {
                    runner_id: node.runner_id,
                    source: node.source,
                    state,
                    capacity: node.capacity,
                    in_flight: node.in_flight,
                    labels: node.labels,
                    classes: node.classes,
                    active_task_count: 0,
                    last_updated: None,
                    active_tasks: Vec::new(),
                },
            )
        })
        .collect();

    for lease in workcells {
        if lease.runner_id.is_empty() {
            continue;
        }
        nodes
            .entry(lease.runner_id.clone())
            .or_insert_with(|| RunnerNodeSummary {
                runner_id: lease.runner_id.clone(),
                source: "workcell".to_string(),
                state: "active".to_string(),
                capacity: 0,
                in_flight: 0,
                labels: Vec::new(),
                classes: Vec::new(),
                active_task_count: 0,
                last_updated: None,
                active_tasks: Vec::new(),
            });
    }

    let workcell_by_id: BTreeMap<_, _> = workcells
        .iter()
        .cloned()
        .map(|lease| (lease.workcell_id.clone(), lease))
        .collect();

    for run in agent_runs
        .iter()
        .filter(|run| matches!(run.state, AgentRunState::Running))
    {
        let AgentRunSourceSnapshot::Workcell { workcell_id, .. } = &run.source else {
            continue;
        };
        let Some(lease) = workcell_by_id.get(workcell_id.as_str()) else {
            continue;
        };
        let task = runner_task_summary(run, lease);
        let node = nodes
            .entry(lease.runner_id.clone())
            .or_insert_with(|| RunnerNodeSummary {
                runner_id: lease.runner_id.clone(),
                source: "workcell".to_string(),
                state: "active".to_string(),
                capacity: 0,
                in_flight: 0,
                labels: Vec::new(),
                classes: Vec::new(),
                active_task_count: 0,
                last_updated: None,
                active_tasks: Vec::new(),
            });
        node.active_tasks.push(task);
    }

    let mut out: Vec<_> = nodes
        .into_values()
        .map(|mut node| {
            node.active_tasks.sort_by(|a, b| a.task_id.cmp(&b.task_id));
            node.active_task_count = node.active_tasks.len() as u32;
            let task_last_updated = node
                .active_tasks
                .iter()
                .filter_map(|task| task.updated_at.clone())
                .max();
            node.last_updated = node.last_updated.take().or(task_last_updated);
            node
        })
        .collect();
    out.sort_by(|a, b| a.runner_id.cmp(&b.runner_id));
    out
}

fn runner_task_summary(
    run: &AgentRunStatusResponse,
    lease: &jeryu_runnerd::WorkcellLease,
) -> RunnerTaskSummary {
    let tty_lines = tty_preview_lines(&run.tty_events);
    let repo = run
        .tty_events
        .iter()
        .rev()
        .find_map(|event| event.repo.clone())
        .or_else(|| {
            lease
                .repo_roots
                .first()
                .map(|path| path.to_string_lossy().to_string())
        });
    let started_at = run
        .tty_events
        .first()
        .map(|event| rfc3339_from_ms(event.occurred_at_ms));
    let updated_at = run
        .tty_events
        .last()
        .map(|event| rfc3339_from_ms(event.occurred_at_ms));
    RunnerTaskSummary {
        task_id: run.agent_run_id.clone(),
        job_id: lease.workcell_id.clone(),
        agent_run_id: Some(run.agent_run_id.clone()),
        workcell_id: Some(lease.workcell_id.clone()),
        repo,
        label: task_label(&run.program),
        program: run.program.clone(),
        state: format!("{:?}", run.state).to_ascii_lowercase(),
        started_at,
        updated_at: updated_at.clone(),
        tty_preview: RunnerTtyPreview {
            state: if tty_lines.is_empty() {
                EvidenceState::Missing
            } else {
                EvidenceState::Fresh
            },
            lines: tty_lines,
        },
    }
}

fn tty_preview_lines(events: &[jeryu_agent_stream::AgentTtyEvent]) -> Vec<String> {
    let mut lines = Vec::new();
    for event in events {
        if let Some(text) = &event.text {
            for line in text.lines() {
                let line = line.trim_end();
                if !line.is_empty() {
                    lines.push(line.to_string());
                }
            }
        }
    }
    const MAX_PREVIEW_LINES: usize = 5;
    if lines.len() > MAX_PREVIEW_LINES {
        lines = lines[lines.len() - MAX_PREVIEW_LINES..].to_vec();
    }
    lines
}

fn task_label(program: &str) -> String {
    Path::new(program)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(program)
        .to_string()
}

fn rfc3339_from_ms(ms: u64) -> String {
    DateTime::<Utc>::from_timestamp_millis(i64::try_from(ms).unwrap_or(i64::MAX))
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| ms.to_string())
}

fn normalize_node_state(state: &str) -> String {
    if state.is_empty() {
        "unknown".to_string()
    } else {
        state.to_string()
    }
}

pub(super) fn remote_status() -> RemoteStatusResponse {
    let missing = MirrorEvidence {
        name: "github".to_string(),
        state: EvidenceState::Missing,
        reason:
            "GitHub mirror evidence is optional, read-only, and unavailable in this local snapshot"
                .to_string(),
        docs_url: MIRROR_DOCS.to_string(),
    };
    RemoteStatusResponse {
        schema_version: "jeryu.remote.status/v1".to_string(),
        state: EvidenceState::Missing,
        mirrors: vec![missing],
        divergence: MirrorDivergence {
            state: EvidenceState::Unknown,
            reason: "mirror default-branch state is unavailable, so divergence is unknown rather than healthy"
                .to_string(),
            local_default_branches: Vec::new(),
            mirror_default_branches: Vec::new(),
        },
    }
}

pub(super) fn repo_graph_response(
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

fn collect_repos(core: &ForgeCore) -> Vec<ControlRepo> {
    core.list_repositories(None)
        .into_iter()
        .map(|repo| {
            let checks = core
                .list_check_runs(&repo.owner, &repo.name, None)
                .map(|list| list.check_runs)
                .unwrap_or_default();
            let prs = core
                .list_pull_requests(&repo.owner, &repo.name, None)
                .unwrap_or_default();
            let open_pull_requests = prs
                .iter()
                .filter(|pr| {
                    matches!(
                        pr.state,
                        PullRequestState::Draft
                            | PullRequestState::Open
                            | PullRequestState::ReadyForReview
                            | PullRequestState::BlockedByPolicy
                            | PullRequestState::BlockedByChecks
                            | PullRequestState::Approved
                            | PullRequestState::Queued
                            | PullRequestState::SpeculativeMergeTesting
                            | PullRequestState::Mergeable
                    )
                })
                .count();
            let draft_pull_requests = prs.iter().filter(|pr| pr.draft).count();
            let queued_checks = checks
                .iter()
                .filter(|check| check.status == CheckRunStatus::Queued)
                .count();
            let running_checks = checks
                .iter()
                .filter(|check| check.status == CheckRunStatus::InProgress)
                .count();
            let failing_checks = checks.iter().filter(|check| failing_check(check)).count();
            let latest_head_sha = checks
                .iter()
                .max_by_key(|check| check.completed_at.or(Some(check.started_at)))
                .map(|check| check.head_sha.clone());
            ControlRepo {
                id: repo.id.to_string(),
                full_name: repo.full_name,
                owner: repo.owner,
                name: repo.name,
                default_branch: repo.default_branch,
                private: repo.private,
                archived: repo.archived,
                disabled: repo.disabled,
                open_pull_requests,
                draft_pull_requests,
                queued_checks,
                running_checks,
                failing_checks,
                latest_head_sha,
                state: EvidenceState::Fresh,
            }
        })
        .collect()
}

fn collect_pull_requests(core: &ForgeCore, repos: &[ControlRepo]) -> Vec<ControlPullRequest> {
    let mut out = Vec::new();
    for repo in repos {
        let checks = core
            .list_check_runs(&repo.owner, &repo.name, None)
            .map(|list| list.check_runs)
            .unwrap_or_default();
        for pr in core
            .list_pull_requests(&repo.owner, &repo.name, None)
            .unwrap_or_default()
        {
            let pr_checks: Vec<CheckRun> = checks
                .iter()
                .filter(|check| check.head_sha == pr.head.sha)
                .cloned()
                .collect();
            let checks = summarize_checks(&pr_checks);
            let state_evidence = if checks.missing {
                EvidenceState::Missing
            } else if checks.failing > 0 {
                EvidenceState::Failed
            } else if checks.queued > 0 {
                EvidenceState::Queued
            } else {
                EvidenceState::Fresh
            };
            out.push(ControlPullRequest {
                repo: repo.full_name.clone(),
                number: pr.number,
                title: pr.title,
                draft: pr.draft,
                state: format!("{:?}", pr.state).to_ascii_lowercase(),
                head_ref: pr.head.ref_name,
                head_sha: pr.head.sha,
                base_ref: pr.base.ref_name,
                base_sha: pr.base.sha,
                mergeable: pr.mergeable,
                mergeable_state: pr.mergeable_state,
                changed_files: pr.changed_files,
                checks,
                state_evidence,
                source_links: vec![SourceLink {
                    label: format!("{}#{}", repo.full_name, pr.number),
                    url: format!("/{}/pull/{}", repo.full_name, pr.number),
                }],
            });
        }
    }
    out
}

fn collect_check_runs(core: &ForgeCore, repos: &[ControlRepo]) -> Vec<ControlCheckRun> {
    let mut checks = Vec::new();
    for repo in repos {
        let list = core
            .list_check_runs(&repo.owner, &repo.name, None)
            .map(|list| list.check_runs)
            .unwrap_or_default();
        for check in list {
            let state = check_state(&check);
            checks.push(ControlCheckRun {
                id: check.id.to_string(),
                repo: repo.full_name.clone(),
                name: check.name,
                head_sha: check.head_sha,
                status: check_status(&check.status).to_string(),
                conclusion: check
                    .conclusion
                    .as_ref()
                    .map(check_conclusion)
                    .map(str::to_string),
                started_at: check.started_at.to_rfc3339(),
                completed_at: check.completed_at.map(|ts| ts.to_rfc3339()),
                details_url: check.details_url,
                state,
            });
        }
    }
    checks
}

fn collect_workflows(checks: &[ControlCheckRun]) -> Vec<ControlWorkflow> {
    checks
        .iter()
        .enumerate()
        .map(|(index, check)| ControlWorkflow {
            id: format!("wf-{:06}", index + 1),
            repo: check.repo.clone(),
            name: check.name.clone(),
            head_sha: check.head_sha.clone(),
            state: check.state.clone(),
            check_run_id: check.id.clone(),
            jobs_url: format!("/api/v1/ci/runs/{}/evidence", check.id),
        })
        .collect()
}

fn releases() -> ControlReleaseSummary {
    ControlReleaseSummary {
        state: EvidenceState::Missing,
        latest_release: None,
        release_count: 0,
        reason: "release persistence is not yet durable in the local forge domain".to_string(),
        docs_url: "docs/release.md".to_string(),
    }
}

fn codegraph_summary(state: &Arc<WebState>) -> CodegraphControlSummary {
    match state.codegraph_store.load_snapshot() {
        Ok(snapshot) => {
            let latest_index_run = snapshot
                .index_runs
                .last()
                .map(|run| format!("{}@{}", run.repo_id, run.ref_name));
            let state = if snapshot.symbols.is_empty() && snapshot.symbol_refs.is_empty() {
                EvidenceState::Missing
            } else {
                EvidenceState::Fresh
            };
            CodegraphControlSummary {
                state,
                indexed_symbols: snapshot.symbols.len(),
                indexed_references: snapshot.symbol_refs.len(),
                crate_edges: snapshot.crate_deps.len(),
                indexed_files: snapshot.files.len(),
                latest_index_run,
                reason: if snapshot.symbols.is_empty() {
                    "codegraph store is reachable but has no indexed symbols".to_string()
                } else {
                    "codegraph store is reachable".to_string()
                },
            }
        }
        Err(error) => CodegraphControlSummary {
            state: EvidenceState::Failed,
            indexed_symbols: 0,
            indexed_references: 0,
            crate_edges: 0,
            indexed_files: 0,
            latest_index_run: None,
            reason: error.to_string(),
        },
    }
}

fn tool_build_summary(state: &Arc<WebState>) -> ToolBuildControlSummary {
    let counts = state.codegraph_store.tool_build_cluster_counts(None);
    let clusters = state.codegraph_store.tool_build_clusters(None, 5, false);
    match (counts, clusters) {
        (Ok((cluster_count, ignored_count)), Ok(clusters)) => ToolBuildControlSummary {
            state: if cluster_count == 0 {
                EvidenceState::Missing
            } else {
                EvidenceState::Fresh
            },
            cluster_count,
            ignored_count,
            top_clusters: clusters
                .into_iter()
                .map(|cluster| ToolBuildClusterSummary {
                    cluster_id: cluster.cluster_id,
                    repo_id: cluster.repo_id,
                    score: cluster.score,
                    occurrence_count: cluster.occurrence_count,
                    file_count: cluster.file_count,
                    insight: cluster.insight,
                })
                .collect(),
        },
        (Err(_), _) | (_, Err(_)) => ToolBuildControlSummary {
            state: EvidenceState::Failed,
            cluster_count: 0,
            ignored_count: 0,
            top_clusters: Vec::new(),
        },
    }
}

fn mcp_health() -> McpToolHealth {
    let tool_count = jeryu_mcp::tool_manifest().len();
    McpToolHealth {
        state: EvidenceState::Fresh,
        tool_count,
        live_backed_tools: vec![
            "jeryu.control_plane.status".to_string(),
            "jeryu.control_plane.priorities".to_string(),
            "jeryu.repo_graph.clusters".to_string(),
            "jeryu.repo_graph.query".to_string(),
            "jeryu.remote.status".to_string(),
            "jeryu.artifacts.latest".to_string(),
            "jeryu.runner_fabric.status".to_string(),
            "jeryu.get_system_snapshot".to_string(),
            "jeryu.get_ci_run_jobs".to_string(),
            "jeryu.get_ci_bottlenecks".to_string(),
            "jeryu.explain_blockers".to_string(),
            "jeryu.plan_validation".to_string(),
        ],
        degraded_tools: Vec::new(),
    }
}

fn summary(
    repos: &[ControlRepo],
    prs: &[ControlPullRequest],
    checks: &[ControlCheckRun],
    priorities: &[PriorityInsight],
    artifacts: &ArtifactLatestResponse,
    runners: &RunnerFabricResponse,
    mirror: &RemoteStatusResponse,
) -> ControlPlaneSummary {
    ControlPlaneSummary {
        repo_count: repos.len(),
        open_pr_count: prs.len(),
        draft_pr_count: prs.iter().filter(|pr| pr.draft).count(),
        queued_check_count: checks
            .iter()
            .filter(|check| check.state == EvidenceState::Queued)
            .count(),
        running_check_count: checks
            .iter()
            .filter(|check| check.status == "in_progress")
            .count(),
        failing_check_count: checks
            .iter()
            .filter(|check| check.state == EvidenceState::Failed)
            .count(),
        missing_check_pr_count: prs.iter().filter(|pr| pr.checks.missing).count(),
        priority_count: priorities.len(),
        critical_priority_count: priorities
            .iter()
            .filter(|p| p.severity == InsightSeverity::Critical)
            .count(),
        high_priority_count: priorities
            .iter()
            .filter(|p| p.severity == InsightSeverity::High)
            .count(),
        mirror_state: mirror.state.clone(),
        artifact_state: artifacts.state.clone(),
        runner_state: runners.local.state.clone(),
    }
}

struct PriorityInputs<'a> {
    repos: &'a [ControlRepo],
    prs: &'a [ControlPullRequest],
    checks: &'a [ControlCheckRun],
    runners: &'a RunnerFabricResponse,
    artifacts: &'a ArtifactLatestResponse,
    mirror: &'a RemoteStatusResponse,
    codegraph: &'a CodegraphControlSummary,
    tool_build: &'a ToolBuildControlSummary,
}

struct PriorityDraft<'a> {
    id: String,
    title: String,
    severity: InsightSeverity,
    score: u32,
    owner: &'a str,
    proof_lane: &'a str,
    recommended_action: &'a str,
    evidence: Vec<String>,
    source_links: Vec<SourceLink>,
    state: EvidenceState,
}

fn priority_insights(input: PriorityInputs<'_>) -> Vec<PriorityInsight> {
    let PriorityInputs {
        repos,
        prs,
        checks,
        runners,
        artifacts,
        mirror,
        codegraph,
        tool_build,
    } = input;
    let mut insights = Vec::new();
    for pr in prs {
        if pr.checks.missing {
            insights.push(priority(PriorityDraft {
                id: format!(
                    "pr-{}-{}-checks-missing",
                    pr.repo.replace('/', "-"),
                    pr.number
                ),
                title: format!("PR #{} has no head checks", pr.number),
                severity: InsightSeverity::High,
                score: 840,
                owner: "forge-api",
                proof_lane: "cargo test -p jeryu-api --features web --jobs 40 control_plane",
                recommended_action:
                    "create or refresh check-runs for the PR head before merge evaluation",
                evidence: vec![
                    format!("repo={}", pr.repo),
                    format!("head_sha={}", pr.head_sha),
                    "missing checks are unsafe evidence, not success".to_string(),
                ],
                source_links: pr.source_links.clone(),
                state: EvidenceState::Missing,
            }));
        }
    }
    let failing = checks
        .iter()
        .filter(|check| check.state == EvidenceState::Failed)
        .collect::<Vec<_>>();
    if !failing.is_empty() {
        insights.push(priority(PriorityDraft {
            id: "ci-failing-checks".to_string(),
            title: format!("{} failing check run(s)", failing.len()),
            severity: InsightSeverity::High,
            score: 780,
            owner: "forge-api",
            proof_lane: "cargo test -p jeryu-api --features web --jobs 40 control_plane",
            recommended_action: "inspect failing check-run evidence and route repair through typed errors",
            evidence: failing
                .iter()
                .take(5)
                .map(|check| format!("{} {} {}", check.repo, check.name, check.head_sha))
                .collect(),
            source_links: failing
                .iter()
                .take(5)
                .map(|check| SourceLink {
                    label: check.name.clone(),
                    url: format!("/api/v1/ci/runs/{}/evidence", check.id),
                })
                .collect(),
            state: EvidenceState::Failed,
        }));
    }
    if artifacts.state == EvidenceState::Missing {
        insights.push(priority(PriorityDraft {
            id: "artifacts-latest-missing".to_string(),
            title: "Latest artifacts are absent".to_string(),
            severity: InsightSeverity::Medium,
            score: 640,
            owner: "release-security",
            proof_lane: "cargo test -p jeryu-api --features web --jobs 40 control_plane",
            recommended_action:
                "record artifact evidence or keep the absence explicit for release decisions",
            evidence: vec![artifacts.latest_release.reason.clone()],
            source_links: vec![SourceLink {
                label: "release receipt".to_string(),
                url: ARTIFACT_DOCS.to_string(),
            }],
            state: EvidenceState::Missing,
        }));
    }
    if mirror.state == EvidenceState::Missing {
        insights.push(priority(PriorityDraft {
            id: "github-mirror-missing".to_string(),
            title: "GitHub mirror evidence unavailable".to_string(),
            severity: InsightSeverity::Medium,
            score: 600,
            owner: "forge-api",
            proof_lane: "cargo test -p jeryu-api --features web --jobs 40 control_plane",
            recommended_action:
                "treat mirror state as missing until a read-only adapter supplies fresh evidence",
            evidence: vec![mirror.divergence.reason.clone()],
            source_links: vec![SourceLink {
                label: "agent native standard".to_string(),
                url: MIRROR_DOCS.to_string(),
            }],
            state: EvidenceState::Missing,
        }));
    }
    if runners.local.offline_runners > 0 {
        insights.push(priority(PriorityDraft {
            id: "runner-offline-capacity".to_string(),
            title: format!(
                "{} runner(s) offline or fenced",
                runners.local.offline_runners
            ),
            severity: InsightSeverity::High,
            score: 760,
            owner: "ci-runtime",
            proof_lane: "cargo test -p jeryu-api --features web --jobs 40 control_plane",
            recommended_action:
                "repair runner registration or drain evidence before scheduling more work",
            evidence: vec![format!(
                "online={} offline={} active_slots={}",
                runners.local.online_runners,
                runners.local.offline_runners,
                runners.local.active_slots
            )],
            source_links: Vec::new(),
            state: EvidenceState::Failed,
        }));
    }
    if codegraph.state != EvidenceState::Fresh {
        insights.push(priority(PriorityDraft {
            id: "codegraph-index-missing".to_string(),
            title: "Codegraph index is not fresh".to_string(),
            severity: InsightSeverity::Low,
            score: 360,
            owner: "rust-ci",
            proof_lane: "bash ops/ci/codegraph-oracle.sh",
            recommended_action: "rerun the codegraph oracle lane before relying on impact analysis",
            evidence: vec![codegraph.reason.clone()],
            source_links: vec![SourceLink {
                label: "codegraph oracle".to_string(),
                url: "docs/codegraph-oracle.md".to_string(),
            }],
            state: codegraph.state.clone(),
        }));
    }
    if tool_build.cluster_count > 0 {
        insights.push(priority(PriorityDraft {
            id: "tool-build-clusters-ready".to_string(),
            title: format!(
                "{} repeated-code cluster(s) ready",
                tool_build.cluster_count
            ),
            severity: InsightSeverity::Low,
            score: 300,
            owner: "rust-ci",
            proof_lane: "bash ops/ci/codegraph-tool-build.sh",
            recommended_action:
                "review ranked clusters for tool-building extraction or record ignore feedback",
            evidence: tool_build
                .top_clusters
                .iter()
                .map(|cluster| cluster.insight.clone())
                .collect(),
            source_links: vec![SourceLink {
                label: "tool-build clusters".to_string(),
                url: "/api/v1/codegraph/tool-build/clusters".to_string(),
            }],
            state: EvidenceState::Fresh,
        }));
    }
    if repos.is_empty() {
        insights.push(priority(PriorityDraft {
            id: "no-local-repos".to_string(),
            title: "No local repositories imported".to_string(),
            severity: InsightSeverity::Info,
            score: 120,
            owner: "forge-api",
            proof_lane: "cargo test -p jeryu-api --features web --jobs 40 control_plane",
            recommended_action:
                "import a local repository before expecting PR, CI, or graph evidence",
            evidence: vec!["local ForgeCore repository list is empty".to_string()],
            source_links: vec![SourceLink {
                label: "local runtime".to_string(),
                url: "README.md#local-live-runtime".to_string(),
            }],
            state: EvidenceState::Missing,
        }));
    }
    insights.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    insights
}

fn priority(draft: PriorityDraft<'_>) -> PriorityInsight {
    PriorityInsight {
        id: draft.id,
        title: draft.title,
        severity: draft.severity,
        score: draft.score,
        confidence: 1.0,
        owner: draft.owner.to_string(),
        proof_lane: draft.proof_lane.to_string(),
        recommended_action: draft.recommended_action.to_string(),
        evidence: draft.evidence,
        source_links: draft.source_links,
        state: draft.state,
        rules_version: RULES_VERSION.to_string(),
    }
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
        id: "cluster:stale-mirror".to_string(),
        label: "Mirror evidence".to_string(),
        kind: "stale_mirror".to_string(),
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

fn summarize_checks(checks: &[CheckRun]) -> CheckSummary {
    let queued = checks
        .iter()
        .filter(|check| check.status == CheckRunStatus::Queued)
        .count();
    let running = checks
        .iter()
        .filter(|check| check.status == CheckRunStatus::InProgress)
        .count();
    let failing = checks.iter().filter(|check| failing_check(check)).count();
    let successful = checks
        .iter()
        .filter(|check| {
            check.status == CheckRunStatus::Completed
                && check.conclusion == Some(CheckConclusion::Success)
        })
        .count();
    CheckSummary {
        total: checks.len(),
        queued,
        running,
        failing,
        successful,
        missing: checks.is_empty(),
    }
}

fn check_state(check: &CheckRun) -> EvidenceState {
    match check.status {
        CheckRunStatus::Queued => EvidenceState::Queued,
        CheckRunStatus::InProgress => EvidenceState::Fresh,
        CheckRunStatus::Completed if failing_check(check) => EvidenceState::Failed,
        CheckRunStatus::Completed => EvidenceState::Fresh,
    }
}

fn failing_check(check: &CheckRun) -> bool {
    matches!(
        check.conclusion,
        Some(
            CheckConclusion::ActionRequired
                | CheckConclusion::Cancelled
                | CheckConclusion::Failure
                | CheckConclusion::TimedOut
        )
    )
}

fn check_status(status: &CheckRunStatus) -> &'static str {
    match status {
        CheckRunStatus::Queued => "queued",
        CheckRunStatus::InProgress => "in_progress",
        CheckRunStatus::Completed => "completed",
    }
}

fn check_conclusion(conclusion: &CheckConclusion) -> &'static str {
    match conclusion {
        CheckConclusion::ActionRequired => "action_required",
        CheckConclusion::Cancelled => "cancelled",
        CheckConclusion::Failure => "failure",
        CheckConclusion::Neutral => "neutral",
        CheckConclusion::Success => "success",
        CheckConclusion::Skipped => "skipped",
        CheckConclusion::Superseded => "stale",
        CheckConclusion::TimedOut => "timed_out",
    }
}

pub(super) fn mcp_status(state: &Arc<WebState>) -> Value {
    control_value(snapshot(state), "control-plane snapshot serializes for MCP")
}

pub(super) fn mcp_priorities(state: &Arc<WebState>, args: &Value) -> Value {
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

pub(super) fn mcp_repo_graph_clusters(state: &Arc<WebState>, args: &Value) -> Value {
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

pub(super) fn mcp_repo_graph_query(state: &Arc<WebState>, args: &Value) -> Value {
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

pub(super) fn mcp_remote_status() -> Value {
    control_value(remote_status(), "remote status serializes for MCP")
}

pub(super) fn mcp_artifacts_latest(state: &Arc<WebState>) -> Value {
    control_value(artifacts(state), "artifact status serializes for MCP")
}

pub(super) fn mcp_runner_fabric_status(state: &Arc<WebState>) -> Value {
    control_value(runner_fabric(state), "runner fabric serializes for MCP")
}

pub(super) fn mcp_ci_run_jobs(state: &Arc<WebState>, args: &Value) -> Value {
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

pub(super) fn mcp_ci_bottlenecks(state: &Arc<WebState>, args: &Value) -> Value {
    let snapshot = snapshot(state);
    json!({
        "repo": args.get("repo").cloned().unwrap_or(Value::Null),
        "bottlenecks": snapshot.priorities.iter().filter(|item| {
            matches!(item.severity, InsightSeverity::Critical | InsightSeverity::High)
        }).collect::<Vec<_>>(),
    })
}

pub(super) fn mcp_explain_blockers(state: &Arc<WebState>, args: &Value) -> Value {
    let priorities = snapshot(state).priorities;
    json!({
        "entity_type": args.get("entity_type").cloned().unwrap_or(Value::Null),
        "entity_id": args.get("entity_id").cloned().unwrap_or(Value::Null),
        "mergeable": priorities.iter().all(|p| !matches!(p.severity, InsightSeverity::Critical | InsightSeverity::High)),
        "blockers": priorities,
    })
}

pub(super) fn mcp_plan_validation(state: &Arc<WebState>, args: &Value) -> Value {
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

fn control_value<T: Serialize>(value: T, context: &str) -> Value {
    serde_json::to_value(value).expect(context)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use jeryu_agent_stream::{AgentOutputStream, AgentRunStreamKey, AgentTtyEvent};
    use jeryu_core::{CreateCheckRunRequest, CreatePullRequestRequest, CreateRepositoryRequest};
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
                .any(|cluster| cluster.kind == "stale_mirror")
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
        assert_eq!(check_conclusion(&CheckConclusion::Superseded), "stale");
        assert_eq!(check_conclusion(&CheckConclusion::TimedOut), "timed_out");
    }
}
