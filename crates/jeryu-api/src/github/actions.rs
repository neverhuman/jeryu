//! GitHub Actions-compatible routes
//! (`/repos/{owner}/{repo}/actions/...`) and their GitHub-shaped renderers.
//!
//! Jeryu's forge does not run GitHub Actions; CI is driven by the Codex
//! engine and surfaced as check-runs. So this edge sources the Actions API
//! shape from the repository's check-runs as a proxy: each check-run is
//! projected to a workflow *run*, its `name` is projected to a *workflow*, and
//! the run's single step is projected to a *job*. When a repo has no check-run
//! data, every route returns a VALID, EMPTY GitHub-shaped object (e.g.
//! `{"total_count":0,"workflow_runs":[]}`) so `gh run list` works without
//! erroring rather than 404-ing.
//!
//! Run ids are synthesized as a stable 1-based index over the repo's
//! check-runs so `/actions/runs/{id}` and `/actions/runs/{id}/jobs` resolve
//! deterministically against the same projection.

use std::collections::BTreeMap;

use jeryu_core::{CheckConclusion, CheckRun, CheckRunStatus};
use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
use super::support::{
    Pagination, actions_write_response, error_response, json_response, owner_json, paginate,
};

#[derive(Clone, Debug)]
struct WorkflowRecord {
    id: u64,
    name: String,
    path: String,
    default_branch: String,
    runs: Vec<(u64, CheckRun)>,
    created_at: String,
    updated_at: String,
}

impl GithubRouter {
    /// `GET /repos/{owner}/{repo}/actions/runs` — list workflow runs.
    pub(super) fn list_action_runs(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        page: Pagination,
    ) -> Response {
        let workflows = match self.workflow_records(owner, repo) {
            Ok(runs) => runs,
            Err(response) => return response,
        };
        let rendered: Vec<Value> = Self::workflow_runs(&workflows)
            .into_iter()
            .map(|(workflow, run_id, run)| run_json(owner, workflow, run_id, run))
            .collect();
        paginate(
            path,
            page,
            &rendered,
            |slice, total| json!({ "total_count": total, "workflow_runs": slice }),
        )
    }

    /// `GET /repos/{owner}/{repo}/actions/runs/{id}` — a single workflow run.
    pub(super) fn get_action_run(&self, owner: &str, repo: &str, id: &str) -> Response {
        let workflows = match self.workflow_records(owner, repo) {
            Ok(workflows) => workflows,
            Err(response) => return response,
        };
        match find_run(&workflows, id) {
            Some((workflow, run_id, run)) => {
                json_response(200, &run_json(owner, workflow, run_id, run))
            }
            None => not_found_run(id),
        }
    }

    /// `GET /repos/{owner}/{repo}/actions/runs/{id}/jobs` — jobs for a run.
    pub(super) fn list_action_run_jobs(&self, owner: &str, repo: &str, id: &str) -> Response {
        let workflows = match self.workflow_records(owner, repo) {
            Ok(workflows) => workflows,
            Err(response) => return response,
        };
        match find_run(&workflows, id) {
            Some((workflow, run_id, run)) => {
                let job = job_json(workflow, run_id, run);
                json_response(200, &json!({ "total_count": 1, "jobs": [job] }))
            }
            None => not_found_run(id),
        }
    }

    /// `GET /repos/{owner}/{repo}/actions/workflows` — list workflows.
    pub(super) fn list_action_workflows(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        page: Pagination,
    ) -> Response {
        let workflows = match self.workflow_records(owner, repo) {
            Ok(workflows) => workflows,
            Err(response) => return response,
        };
        let rendered: Vec<Value> = workflows
            .iter()
            .map(|workflow| workflow_json(owner, repo, workflow))
            .collect();
        paginate(
            path,
            page,
            &rendered,
            |slice, total| json!({ "total_count": total, "workflows": slice }),
        )
    }

    /// `GET /repos/{owner}/{repo}/actions/workflows/{workflow_id}` — workflow
    /// details for the supplied workflow id or workflow file name.
    pub(super) fn get_action_workflow(
        &self,
        owner: &str,
        repo: &str,
        workflow_id: &str,
    ) -> Response {
        let workflows = match self.workflow_records(owner, repo) {
            Ok(workflows) => workflows,
            Err(response) => return response,
        };
        match find_workflow(&workflows, workflow_id) {
            Some(workflow) => json_response(200, &workflow_json(owner, repo, workflow)),
            None => not_found_workflow(workflow_id),
        }
    }

    /// `GET /repos/{owner}/{repo}/actions/workflows/{workflow_id}/runs` —
    /// workflow runs for a specific workflow.
    pub(super) fn list_action_workflow_runs(
        &self,
        owner: &str,
        repo: &str,
        workflow_id: &str,
        path: &str,
        page: Pagination,
    ) -> Response {
        let workflows = match self.workflow_records(owner, repo) {
            Ok(workflows) => workflows,
            Err(response) => return response,
        };
        match find_workflow(&workflows, workflow_id) {
            Some(workflow) => {
                let rendered: Vec<Value> = workflow
                    .runs
                    .iter()
                    .map(|(run_id, run)| run_json(owner, workflow, *run_id, run))
                    .collect();
                paginate(
                    path,
                    page,
                    &rendered,
                    |slice, total| json!({ "total_count": total, "workflow_runs": slice }),
                )
            }
            None => not_found_workflow(workflow_id),
        }
    }

    /// `POST /repos/{owner}/{repo}/actions/...` — unsupported hosted Actions
    /// writes surface a guided local CI/MCP error instead of a silent 404.
    pub(super) fn unsupported_action_write(&self, owner: &str, repo: &str) -> Response {
        actions_write_response(owner, repo)
    }

    /// Projects the repo's check-runs to synthetic workflow records derived
    /// from the check-run names.
    /// Returns the forge error (404 for an unknown repo) so a missing repo is
    /// distinguishable from an empty-but-valid run list.
    fn workflow_records(
        &self,
        owner: &str,
        repo: &str,
    ) -> std::result::Result<Vec<WorkflowRecord>, Response> {
        match self.core.list_check_runs(owner, repo, None) {
            Ok(list) => {
                let default_branch = self
                    .core
                    .get_repository(owner, repo)
                    .map(|repo| repo.default_branch)
                    .unwrap_or_else(|_| "main".to_owned());
                let mut grouped: BTreeMap<String, Vec<(u64, CheckRun)>> = BTreeMap::new();
                for (index, run) in list.check_runs.into_iter().enumerate() {
                    grouped
                        .entry(run.name.clone())
                        .or_default()
                        .push((index as u64 + 1, run));
                }
                Ok(grouped
                    .into_iter()
                    .enumerate()
                    .map(|(index, (name, runs))| WorkflowRecord {
                        id: index as u64 + 1,
                        path: workflow_path(&name),
                        default_branch: default_branch.clone(),
                        created_at: runs
                            .first()
                            .map(|(_, run)| run.started_at.to_rfc3339())
                            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_owned()),
                        updated_at: runs
                            .last()
                            .map(|(_, run)| run.completed_at.unwrap_or(run.started_at).to_rfc3339())
                            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_owned()),
                        name,
                        runs,
                    })
                    .collect())
            }
            Err(err) => Err(error_response(err)),
        }
    }

    fn workflow_runs(workflows: &[WorkflowRecord]) -> Vec<(&WorkflowRecord, u64, &CheckRun)> {
        let mut runs = Vec::new();
        for workflow in workflows {
            for (run_id, run) in &workflow.runs {
                runs.push((workflow, *run_id, run));
            }
        }
        runs.sort_by_key(|(_, run_id, _)| *run_id);
        runs
    }
}

/// Resolves a synthetic run id (`?id=N`) to its workflow/run pair.
fn find_run<'a>(
    workflows: &'a [WorkflowRecord],
    id: &str,
) -> Option<(&'a WorkflowRecord, u64, &'a CheckRun)> {
    let wanted: u64 = id.parse().ok()?;
    workflows.iter().find_map(|workflow| {
        workflow
            .runs
            .iter()
            .find(|(run_id, _)| *run_id == wanted)
            .map(|(run_id, run)| (workflow, *run_id, run))
    })
}

fn find_workflow<'a>(
    workflows: &'a [WorkflowRecord],
    workflow_id: &str,
) -> Option<&'a WorkflowRecord> {
    if let Ok(wanted) = workflow_id.parse::<u64>() {
        return workflows.iter().find(|workflow| workflow.id == wanted);
    }
    let trimmed = workflow_id.trim();
    workflows.iter().find(|workflow| {
        workflow.name == trimmed
            || workflow.path == trimmed
            || workflow.path.rsplit('/').next() == Some(trimmed)
            || workflow.path.rsplit('/').next() == Some(&workflow_id.replace(".yaml", ".yml"))
            || workflow.path.rsplit('/').next() == Some(&workflow_id.replace(".yml", ".yaml"))
            || workflow
                .path
                .rsplit('/')
                .next()
                .map(|file| file.trim_end_matches(".yml").trim_end_matches(".yaml"))
                == Some(trimmed)
    })
}

fn not_found_run(id: &str) -> Response {
    error_response(jeryu_core::ForgeError::NotFound(format!(
        "workflow run {id} not found"
    )))
}

fn not_found_workflow(id: &str) -> Response {
    error_response(jeryu_core::ForgeError::NotFound(format!(
        "workflow {id} not found"
    )))
}

/// GitHub-shaped `status` for a workflow run, projected from the check-run.
fn run_status(status: &CheckRunStatus) -> &'static str {
    match status {
        CheckRunStatus::Queued => "queued",
        CheckRunStatus::InProgress => "in_progress",
        CheckRunStatus::Completed => "completed",
    }
}

/// GitHub-shaped `conclusion` for a workflow run, projected from the check-run.
fn run_conclusion(conclusion: &CheckConclusion) -> &'static str {
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

fn run_json(owner: &str, workflow: &WorkflowRecord, run_id: u64, run: &CheckRun) -> Value {
    json!({
        "id": run_id,
        "name": run.name,
        "node_id": format!("workflow-run-{run_id}"),
        "head_sha": run.head_sha,
        "head_branch": workflow.default_branch,
        "path": format!("{}@{}", workflow.path, workflow.default_branch),
        "status": run_status(&run.status),
        "conclusion": run.conclusion.as_ref().map(run_conclusion),
        "run_number": run_id,
        "event": "push",
        "display_title": run.name,
        "workflow_id": workflow.id,
        "workflow_name": workflow.name,
        "check_suite_id": run_id,
        "check_suite_node_id": format!("workflow-suite-{run_id}"),
        "html_url": format!("/{}/{}/actions/runs/{run_id}", run.owner, run.repo),
        "url": format!("/repos/{}/{}/actions/runs/{run_id}", run.owner, run.repo),
        "created_at": run.started_at,
        "updated_at": run.completed_at.unwrap_or(run.started_at),
        "run_started_at": run.started_at,
        "jobs_url": format!("/repos/{}/{}/actions/runs/{run_id}/jobs", run.owner, run.repo),
        "logs_url": format!("/repos/{}/{}/actions/runs/{run_id}/logs", run.owner, run.repo),
        "check_suite_url": format!("/repos/{}/{}/check-suites/{run_id}", run.owner, run.repo),
        "artifacts_url": format!("/repos/{}/{}/actions/runs/{run_id}/artifacts", run.owner, run.repo),
        "cancel_url": format!("/repos/{}/{}/actions/runs/{run_id}/cancel", run.owner, run.repo),
        "rerun_url": format!("/repos/{}/{}/actions/runs/{run_id}/rerun", run.owner, run.repo),
        "workflow_url": format!("/repos/{}/{}/actions/workflows/{}", run.owner, run.repo, workflow.id),
        "pull_requests": [],
        "actor": owner_json(owner),
        "triggering_actor": owner_json(owner),
        "run_attempt": 1,
    })
}

fn job_json(workflow: &WorkflowRecord, run_id: u64, run: &CheckRun) -> Value {
    json!({
        "id": run_id,
        "run_id": run_id,
        "run_url": format!("/repos/{}/{}/actions/runs/{run_id}", run.owner, run.repo),
        "node_id": format!("workflow-job-{run_id}"),
        "name": run.name,
        "head_sha": run.head_sha,
        "head_branch": workflow.default_branch,
        "status": run_status(&run.status),
        "conclusion": run.conclusion.as_ref().map(run_conclusion),
        "started_at": run.started_at,
        "completed_at": run.completed_at,
        "steps": [{
            "name": run.name,
            "status": run_status(&run.status),
            "conclusion": run.conclusion.as_ref().map(run_conclusion),
            "number": 1,
            "started_at": run.started_at,
            "completed_at": run.completed_at,
        }],
        "url": format!("/repos/{}/{}/actions/jobs/{run_id}", run.owner, run.repo),
        "html_url": format!("/{}/{}/actions/runs/{run_id}/jobs/{run_id}", run.owner, run.repo),
        "check_run_url": format!("/repos/{}/{}/check-runs/{run_id}", run.owner, run.repo),
        "labels": [],
        "runner_id": 1,
        "runner_name": "jeryu-runner",
        "runner_group_id": 1,
        "runner_group_name": "default",
        "workflow_name": workflow.name,
    })
}

fn workflow_json(owner: &str, repo: &str, workflow: &WorkflowRecord) -> Value {
    json!({
        "id": workflow.id,
        "name": workflow.name,
        "node_id": format!("workflow-{}", workflow.id),
        "path": workflow.path,
        "state": "active",
        "created_at": workflow.created_at,
        "updated_at": workflow.updated_at,
        "html_url": format!("/{owner}/{repo}/blob/{}/{}", workflow.default_branch, workflow.path),
        "url": format!("/repos/{owner}/{repo}/actions/workflows/{}", workflow.id),
        "badge_url": format!("/{owner}/{repo}/workflows/{}/badge.svg", slugify(&workflow.name)),
    })
}

fn workflow_path(name: &str) -> String {
    format!(".github/workflows/{}.yml", slugify(name))
}

fn slugify(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
