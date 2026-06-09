use std::sync::Arc;

use jeryu_core::{CheckRun, CheckRunStatus, ForgeCore, PullRequestState};
use serde::Serialize;
use serde_json::Value;

use crate::web::{WebState, server_time};

use super::*;

pub(crate) fn snapshot(state: &Arc<WebState>) -> ControlPlaneSnapshot {
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
        crate::web::workcells::live_tui(state).workcells,
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

pub(crate) fn artifacts(_state: &Arc<WebState>) -> ArtifactLatestResponse {
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

pub(crate) fn remote_status() -> RemoteStatusResponse {
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

pub(crate) fn collect_repos(core: &ForgeCore) -> Vec<ControlRepo> {
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

pub(crate) fn collect_pull_requests(
    core: &ForgeCore,
    repos: &[ControlRepo],
) -> Vec<ControlPullRequest> {
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

pub(crate) fn collect_check_runs(core: &ForgeCore, repos: &[ControlRepo]) -> Vec<ControlCheckRun> {
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

pub(crate) fn codegraph_summary(state: &Arc<WebState>) -> CodegraphControlSummary {
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

pub(crate) fn tool_build_summary(state: &Arc<WebState>) -> ToolBuildControlSummary {
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

pub(crate) fn control_value<T: Serialize>(value: T, context: &str) -> Value {
    serde_json::to_value(value).expect(context)
}
