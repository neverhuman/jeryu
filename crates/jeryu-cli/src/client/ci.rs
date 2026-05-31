//! CI domain: run/explanation types plus the [`InMemoryClient`]
//! implementation of the CI surface of [`ForgeClient`].
//!
//! CI work is modelled as compile -> IR -> schedule, never a polled remote
//! pipeline.

use serde::{Deserialize, Serialize};

use super::{ClientError, ClientResult, InMemoryClient, lock};

/// CI input dialect. Foreign-forge YAML dialects are intentionally absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiKind {
    /// GitHub Actions workflow YAML.
    GithubActions,
    /// Native jeryu TOML pipeline.
    NativeToml,
}

/// A scheduled CI run, compiled from a workflow file into the jeryu IR.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiRun {
    /// Opaque run identifier.
    pub id: String,
    /// Repository the run belongs to.
    pub repo: String,
    /// Git ref the run was compiled against.
    pub git_ref: String,
    /// Number of jobs in the compiled pipeline IR.
    pub jobs: u32,
    /// Current run status.
    pub status: CiStatus,
}

/// Status of a CI run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CiStatus {
    /// Queued, awaiting a runner lease.
    Queued,
    /// Currently executing on a runner.
    Running,
    /// Completed successfully.
    Passed,
    /// Completed with a failure.
    Failed,
}

/// An explanation of why a CI run is (or is not) blocked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiExplanation {
    /// Run identifier explained.
    pub run_id: String,
    /// Whether the run is blocked from merging.
    pub blocked: bool,
    /// Ordered reasons contributing to the verdict.
    pub reasons: Vec<String>,
}

// ---------------------------------------------------------------------------
// In-memory implementation: CI surface
// ---------------------------------------------------------------------------

impl InMemoryClient {
    pub(super) fn ci_run_inner(
        &self,
        repo: &str,
        git_ref: &str,
        kind: CiKind,
    ) -> ClientResult<CiRun> {
        let mut state = lock(&self.state);
        state.run_seq += 1;
        // The job count is derived deterministically from the input dialect so
        // tests can assert the compile-to-IR path was taken per kind.
        let jobs = match kind {
            CiKind::GithubActions => 2,
            CiKind::NativeToml => 3,
        };
        let run = CiRun {
            id: format!("run-{}", state.run_seq),
            repo: repo.to_string(),
            git_ref: git_ref.to_string(),
            jobs,
            status: CiStatus::Queued,
        };
        state.runs.push(run.clone());
        Ok(run)
    }

    pub(super) fn ci_status_inner(&self, repo: &str) -> ClientResult<Vec<CiRun>> {
        let state = lock(&self.state);
        Ok(state
            .runs
            .iter()
            .filter(|r| r.repo == repo)
            .cloned()
            .collect())
    }

    pub(super) fn ci_explain_inner(&self, run_id: &str) -> ClientResult<CiExplanation> {
        let state = lock(&self.state);
        let run = state
            .runs
            .iter()
            .find(|r| r.id == run_id)
            .ok_or_else(|| ClientError::NotFound(format!("ci run {run_id}")))?;
        let blocked = matches!(run.status, CiStatus::Failed);
        let reasons = if blocked {
            vec![format!("run {run_id} failed on {}", run.git_ref)]
        } else {
            vec![format!(
                "run {run_id} is {:?} with {} jobs",
                run.status, run.jobs
            )]
        };
        Ok(CiExplanation {
            run_id: run_id.to_string(),
            blocked,
            reasons,
        })
    }
}
