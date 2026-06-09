use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::web::WebState;
use crate::web::agent_runs::{AgentRunSourceSnapshot, AgentRunState, AgentRunStatusResponse};
use crate::web::workcells_support::manager;

use super::*;

pub(crate) fn runner_fabric(state: &Arc<WebState>) -> RunnerFabricResponse {
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

pub(crate) fn tty_preview_lines(events: &[jeryu_agent_stream::AgentTtyEvent]) -> Vec<String> {
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

pub(crate) fn task_label(program: &str) -> String {
    Path::new(program)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(program)
        .to_string()
}

pub(crate) fn rfc3339_from_ms(ms: u64) -> String {
    DateTime::<Utc>::from_timestamp_millis(i64::try_from(ms).unwrap_or(i64::MAX))
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| ms.to_string())
}

pub(crate) fn normalize_node_state(state: &str) -> String {
    if state.is_empty() {
        "unknown".to_string()
    } else {
        state.to_string()
    }
}
