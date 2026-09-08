//! Commit-status routes (combined status read and status creation) and their
//! GitHub-shaped renderers.

use jeryu_core::{CombinedStatus, CommitStatus, CommitStatusState, CreateCommitStatusRequest};
use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
use super::support::{actor, error_response, json_response, owner_json, parse_body};

impl GithubRouter {
    pub(super) fn commit_status(&self, owner: &str, repo: &str, reference: &str) -> Response {
        match self.core.combined_status(owner, repo, reference) {
            Ok(combined) => json_response(200, &combined_status_json(reference, &combined)),
            Err(err) => error_response(err),
        }
    }

    pub(super) fn create_status(&self, owner: &str, repo: &str, sha: &str, body: &str) -> Response {
        let req: CreateCommitStatusRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let creator = actor(body);
        match self
            .core
            .create_commit_status(owner, repo, sha, &creator, req)
        {
            Ok(status) => json_response(201, &commit_status_json(&status)),
            Err(err) => error_response(err),
        }
    }
}

fn commit_status_state(state: &CommitStatusState) -> &'static str {
    match state {
        CommitStatusState::Error => "error",
        CommitStatusState::Failure => "failure",
        CommitStatusState::Pending => "pending",
        CommitStatusState::Success => "success",
    }
}

fn commit_status_json(status: &CommitStatus) -> Value {
    json!({
        "id": status.id,
        "state": commit_status_state(&status.state),
        "context": status.context,
        "description": status.description,
        "target_url": status.target_url,
        "creator": owner_json(&status.creator),
        "created_at": status.created_at,
        "updated_at": status.updated_at,
    })
}

fn combined_status_json(reference: &str, combined: &CombinedStatus) -> Value {
    let statuses: Vec<Value> = combined.statuses.iter().map(commit_status_json).collect();
    json!({
        "state": commit_status_state(&combined.state),
        "sha": reference,
        "total_count": combined.total_count,
        "statuses": statuses,
    })
}
