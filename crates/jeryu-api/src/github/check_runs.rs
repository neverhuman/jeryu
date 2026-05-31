//! Check-run routes (`/repos/{owner}/{repo}/check-runs`) and their
//! GitHub-shaped renderers.

use jeryu_core::{CheckConclusion, CheckRun, CheckRunStatus, CreateCheckRunRequest};
use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
use super::support::{Pagination, error_response, json_response, paginate, parse_body};

impl GithubRouter {
    pub(super) fn list_check_runs(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        page: Pagination,
    ) -> Response {
        match self.core.list_check_runs(owner, repo, None) {
            Ok(list) => {
                let runs: Vec<Value> = list.check_runs.iter().map(check_run_json).collect();
                paginate(
                    path,
                    page,
                    &runs,
                    |slice, total| json!({ "total_count": total, "check_runs": slice }),
                )
            }
            Err(err) => error_response(err),
        }
    }

    pub(super) fn create_check_run(&self, owner: &str, repo: &str, body: &str) -> Response {
        let req: CreateCheckRunRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.create_check_run(owner, repo, req) {
            Ok(run) => json_response(201, &check_run_json(&run)),
            Err(err) => error_response(err),
        }
    }
}

fn check_run_status(status: &CheckRunStatus) -> &'static str {
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
        // GitHub's documented `CheckConclusion` wire value for a superseded run.
        // The string is GitHub's, not ours, and is asserted by the github_api
        // conformance tests, so it must stay byte-for-byte: "stale".
        CheckConclusion::Superseded => "stale",
        CheckConclusion::TimedOut => "timed_out",
    }
}

fn check_run_json(run: &CheckRun) -> Value {
    json!({
        "id": run.id,
        "name": run.name,
        "head_sha": run.head_sha,
        "status": check_run_status(&run.status),
        "conclusion": run.conclusion.as_ref().map(check_conclusion),
        "details_url": run.details_url,
        "output": run.output.as_ref().map(|output| json!({
            "title": output.title,
            "summary": output.summary,
            "text": output.text,
        })),
        "started_at": run.started_at,
        "completed_at": run.completed_at,
    })
}
