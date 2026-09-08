//! Check-run routes (`/repos/{owner}/{repo}/check-runs`) and their
//! GitHub-shaped renderers.

use jeryu_core::{
    CheckConclusion, CheckRun, CheckRunStatus, CreateCheckRunRequest, check_conclusion_wire_value,
};
use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
#[cfg(feature = "web")]
use super::support::docs_url;
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

    pub(super) fn list_check_runs_for_reference(
        &self,
        owner: &str,
        repo: &str,
        reference: &str,
        path: &str,
        page: Pagination,
    ) -> Response {
        let head_sha = match self.resolve_check_run_reference(owner, repo, reference) {
            Ok(head_sha) => head_sha,
            Err(response) => return response,
        };
        match self.core.list_check_runs(owner, repo, Some(&head_sha)) {
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

    fn resolve_check_run_reference(
        &self,
        owner: &str,
        repo: &str,
        reference: &str,
    ) -> Result<String, Response> {
        #[cfg(feature = "web")]
        if let Some(manager) = &self.repo_manager {
            use jeryu_gitd::refs::RefService;

            let resolved = manager.resolve_parts(owner, repo).map_err(|err| {
                json_response(
                    422,
                    &json!({
                        "message": format!("repository does not resolve for check-run lookup: {err}"),
                        "documentation_url": docs_url(),
                    }),
                )
            })?;
            let refs = RefService::new((**manager).clone());
            return match refs.resolve_commit(&resolved, reference) {
                Ok(Some(head_sha)) => Ok(head_sha),
                Ok(None) => Err(json_response(
                    422,
                    &json!({
                        "message": format!(
                            "commit reference {reference:?} is unknown or ambiguous"
                        ),
                        "documentation_url": docs_url(),
                    }),
                )),
                Err(err) => Err(json_response(
                    500,
                    &json!({
                        "message": err.to_string(),
                        "documentation_url": docs_url(),
                    }),
                )),
            };
        }

        let _ = (owner, repo);
        Ok(reference.to_owned())
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
        CheckConclusion::Superseded => check_conclusion_wire_value(conclusion),
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
