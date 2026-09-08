//! Runner domain: runner/executor types plus the [`InMemoryClient`]
//! implementation of the runner surface of [`ForgeClient`].
//!
//! These are jeryu runners; never foreign-forge runners or pools.

use serde::{Deserialize, Serialize};

use super::{ClientError, ClientResult, InMemoryClient, lock};

/// A registered build runner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Runner {
    /// Runner identifier.
    pub id: String,
    /// Executor backing the runner.
    pub executor: RunnerExecutor,
    /// Whether the runner currently accepts leases.
    pub accepting: bool,
}

/// Runner executor backend. Native Rust is the default fast path; OCI is
/// explicit for jobs that require container isolation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunnerExecutor {
    /// OCI container executor.
    Oci,
    /// Native host executor.
    Native,
}

// ---------------------------------------------------------------------------
// In-memory implementation: runner surface
// ---------------------------------------------------------------------------

impl InMemoryClient {
    pub(super) fn runner_list_inner(&self) -> ClientResult<Vec<Runner>> {
        Ok(lock(&self.state).runners.clone())
    }

    pub(super) fn runner_enroll_inner(
        &self,
        node: &str,
        executor: RunnerExecutor,
    ) -> ClientResult<Runner> {
        if node.trim().is_empty() {
            return Err(ClientError::Invalid("node name is empty".into()));
        }
        let mut state = lock(&self.state);
        if state.runners.iter().any(|r| r.id == node) {
            return Err(ClientError::Conflict(format!(
                "runner {node} already enrolled"
            )));
        }
        let runner = Runner {
            id: node.to_string(),
            executor,
            accepting: true,
        };
        state.runners.push(runner.clone());
        Ok(runner)
    }

    pub(super) fn runner_drain_inner(&self, id: &str) -> ClientResult<Runner> {
        let mut state = lock(&self.state);
        let runner = state
            .runners
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| ClientError::NotFound(format!("runner {id}")))?;
        runner.accepting = false;
        Ok(runner.clone())
    }

    pub(super) fn runner_rotate_inner(&self, id: &str) -> ClientResult<String> {
        let state = lock(&self.state);
        if !state.runners.iter().any(|r| r.id == id) {
            return Err(ClientError::NotFound(format!("runner {id}")));
        }
        // Credential issuance lives in jeryu-runnerd's registry; the in-memory
        // client mints a deterministic credential id derived from the runner id
        // so the rotate dispatch path is exercisable and replay-stable.
        Ok(format!("cred-{id}-rotated"))
    }
}
