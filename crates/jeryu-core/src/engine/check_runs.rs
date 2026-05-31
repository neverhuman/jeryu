//! Check runs, check suites, and workflow runs.

use std::collections::HashMap;

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::{ForgeCore, emit_event_locked, refresh_pull_mergeability_for_sha, require_name};
use crate::errors::Result;
use crate::model::*;
use crate::webhooks::event_payload;

impl ForgeCore {
    pub fn create_check_run(
        &self,
        owner: &str,
        repo: &str,
        request: CreateCheckRunRequest,
    ) -> Result<CheckRun> {
        require_name("check run name", &request.name)?;
        require_name("head sha", &request.head_sha)?;
        self.ensure_repo_exists(owner, repo)?;
        let completed = matches!(request.status, Some(CheckRunStatus::Completed))
            || request.conclusion.is_some();
        let check_run = CheckRun {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            name: request.name,
            head_sha: request.head_sha,
            status: request.status.unwrap_or(if completed {
                CheckRunStatus::Completed
            } else {
                CheckRunStatus::Queued
            }),
            conclusion: request.conclusion,
            details_url: request.details_url,
            output: request.output,
            started_at: Utc::now(),
            completed_at: if completed { Some(Utc::now()) } else { None },
        };
        let mut state = self.state.write();
        let previous = state.clone();
        state
            .check_runs
            .entry((owner.to_string(), repo.to_string()))
            .or_default()
            .push(check_run.clone());
        refresh_pull_mergeability_for_sha(&mut state, owner, repo, &check_run.head_sha);
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "check_run",
            event_payload("created", "check_run", json!(check_run.clone())),
        );
        self.persist_after_mutation(&mut state, previous)?;
        Ok(check_run)
    }

    pub fn list_check_runs(
        &self,
        owner: &str,
        repo: &str,
        head_sha: Option<&str>,
    ) -> Result<CheckRunList> {
        self.ensure_repo_exists(owner, repo)?;
        // No check-runs entry for the repo means none have been created; an
        // empty list is the intended value.
        let runs: Vec<_> = self
            .state
            .read()
            .check_runs
            .get(&(owner.to_string(), repo.to_string()))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|run| head_sha.is_none_or(|sha| run.head_sha == sha))
            .collect();
        Ok(CheckRunList {
            total_count: runs.len(),
            check_runs: runs,
        })
    }
    pub fn check_suites(
        &self,
        owner: &str,
        repo: &str,
        head_sha: Option<&str>,
    ) -> Result<Vec<CheckSuite>> {
        let runs = self.list_check_runs(owner, repo, head_sha)?.check_runs;
        let mut grouped: HashMap<String, Vec<CheckRun>> = HashMap::new();
        for run in runs {
            grouped.entry(run.head_sha.clone()).or_default().push(run);
        }
        Ok(grouped
            .into_iter()
            .map(|(sha, runs)| {
                let status = if runs
                    .iter()
                    .all(|run| run.status == CheckRunStatus::Completed)
                {
                    CheckRunStatus::Completed
                } else if runs
                    .iter()
                    .any(|run| run.status == CheckRunStatus::InProgress)
                {
                    CheckRunStatus::InProgress
                } else {
                    CheckRunStatus::Queued
                };
                let conclusion = if status == CheckRunStatus::Completed {
                    if runs
                        .iter()
                        .all(|run| run.conclusion == Some(CheckConclusion::Success))
                    {
                        Some(CheckConclusion::Success)
                    } else {
                        Some(CheckConclusion::Failure)
                    }
                } else {
                    None
                };
                CheckSuite {
                    id: Uuid::new_v4(),
                    owner: owner.to_string(),
                    repo: repo.to_string(),
                    head_sha: sha,
                    status,
                    conclusion,
                    check_runs: runs,
                }
            })
            .collect())
    }

    pub fn workflow_runs(&self, owner: &str, repo: &str) -> Result<WorkflowRunList> {
        let suites = self.check_suites(owner, repo, None)?;
        let workflow_runs: Vec<_> = suites
            .into_iter()
            .map(|suite| WorkflowRun {
                id: suite.id,
                name: "Phase 2 checks".to_string(),
                head_sha: suite.head_sha,
                status: suite.status,
                conclusion: suite.conclusion,
                check_runs: suite.check_runs,
            })
            .collect();
        Ok(WorkflowRunList {
            total_count: workflow_runs.len(),
            workflow_runs,
        })
    }
}
