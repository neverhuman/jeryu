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

use std::collections::BTreeSet;

use jeryu_core::{CheckConclusion, CheckRun, CheckRunStatus};
use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
use super::support::{Pagination, error_response, json_response, paginate};

impl GithubRouter {
    /// `GET /repos/{owner}/{repo}/actions/runs` — list workflow runs.
    pub(super) fn list_action_runs(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        page: Pagination,
    ) -> Response {
        let runs = match self.action_runs(owner, repo) {
            Ok(runs) => runs,
            Err(response) => return response,
        };
        let rendered: Vec<Value> = runs.iter().map(|(id, run)| run_json(*id, run)).collect();
        paginate(
            path,
            page,
            &rendered,
            |slice, total| json!({ "total_count": total, "workflow_runs": slice }),
        )
    }

    /// `GET /repos/{owner}/{repo}/actions/runs/{id}` — a single workflow run.
    pub(super) fn get_action_run(&self, owner: &str, repo: &str, id: &str) -> Response {
        let runs = match self.action_runs(owner, repo) {
            Ok(runs) => runs,
            Err(response) => return response,
        };
        match find_run(&runs, id) {
            Some((run_id, run)) => json_response(200, &run_json(run_id, run)),
            None => not_found_run(id),
        }
    }

    /// `GET /repos/{owner}/{repo}/actions/runs/{id}/jobs` — jobs for a run.
    pub(super) fn list_action_run_jobs(&self, owner: &str, repo: &str, id: &str) -> Response {
        let runs = match self.action_runs(owner, repo) {
            Ok(runs) => runs,
            Err(response) => return response,
        };
        match find_run(&runs, id) {
            Some((run_id, run)) => {
                let job = job_json(run_id, run);
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
        let runs = match self.action_runs(owner, repo) {
            Ok(runs) => runs,
            Err(response) => return response,
        };
        // Each distinct check-run name is one workflow. Stable, deduplicated.
        let mut names: BTreeSet<&str> = BTreeSet::new();
        for (_, run) in &runs {
            names.insert(run.name.as_str());
        }
        let rendered: Vec<Value> = names
            .iter()
            .enumerate()
            .map(|(index, name)| workflow_json(owner, repo, index as u64 + 1, name))
            .collect();
        paginate(
            path,
            page,
            &rendered,
            |slice, total| json!({ "total_count": total, "workflows": slice }),
        )
    }

    /// Projects the repo's check-runs to indexed `(run_id, CheckRun)` pairs.
    /// Returns the forge error (404 for an unknown repo) so a missing repo is
    /// distinguishable from an empty-but-valid run list.
    fn action_runs(
        &self,
        owner: &str,
        repo: &str,
    ) -> std::result::Result<Vec<(u64, CheckRun)>, Response> {
        match self.core.list_check_runs(owner, repo, None) {
            Ok(list) => Ok(list
                .check_runs
                .into_iter()
                .enumerate()
                .map(|(index, run)| (index as u64 + 1, run))
                .collect()),
            Err(err) => Err(error_response(err)),
        }
    }
}

/// Resolves a synthetic run id (`?id=N`) to its `(run_id, &CheckRun)` pair.
fn find_run<'a>(runs: &'a [(u64, CheckRun)], id: &str) -> Option<(u64, &'a CheckRun)> {
    let wanted: u64 = id.parse().ok()?;
    runs.iter()
        .find(|(run_id, _)| *run_id == wanted)
        .map(|(run_id, run)| (*run_id, run))
}

fn not_found_run(id: &str) -> Response {
    error_response(jeryu_core::ForgeError::NotFound(format!(
        "workflow run {id} not found"
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

fn run_json(run_id: u64, run: &CheckRun) -> Value {
    json!({
        "id": run_id,
        "name": run.name,
        "head_sha": run.head_sha,
        "status": run_status(&run.status),
        "conclusion": run.conclusion.as_ref().map(run_conclusion),
        "run_number": run_id,
        "event": "push",
        "workflow_id": run_id,
        "html_url": format!("/{}/{}/actions/runs/{run_id}", run.owner, run.repo),
        "url": format!("/repos/{}/{}/actions/runs/{run_id}", run.owner, run.repo),
        "created_at": run.started_at,
        "updated_at": run.completed_at.unwrap_or(run.started_at),
    })
}

fn job_json(run_id: u64, run: &CheckRun) -> Value {
    json!({
        "id": run_id,
        "run_id": run_id,
        "name": run.name,
        "head_sha": run.head_sha,
        "status": run_status(&run.status),
        "conclusion": run.conclusion.as_ref().map(run_conclusion),
        "started_at": run.started_at,
        "completed_at": run.completed_at,
        "steps": [{
            "name": run.name,
            "status": run_status(&run.status),
            "conclusion": run.conclusion.as_ref().map(run_conclusion),
            "number": 1,
        }],
        "url": format!("/repos/{}/{}/actions/jobs/{run_id}", run.owner, run.repo),
    })
}

fn workflow_json(owner: &str, repo: &str, workflow_id: u64, name: &str) -> Value {
    json!({
        "id": workflow_id,
        "name": name,
        "path": format!(".github/workflows/{name}.yml"),
        "state": "active",
        "html_url": format!("/{owner}/{repo}/actions/workflows/{workflow_id}"),
        "url": format!("/repos/{owner}/{repo}/actions/workflows/{workflow_id}"),
    })
}
