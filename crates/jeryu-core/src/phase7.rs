//! Phase 7 proof, queue, and agent model.

pub use crate::error::{JeryuError, JeryuResult};
pub use crate::ids::{AgentId, PullRequestId, QueueEntryId, ReceiptId, RepoId};
pub use crate::receipt::{Receipt, ReceiptKind};

use std::collections::BTreeSet;

/// Changed repository path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangedPath {
    /// Path relative to the repository root.
    pub path: String,
    /// Whether policy should treat the path as sensitive.
    pub sensitive: bool,
}

impl ChangedPath {
    /// Creates a non-sensitive changed path.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            sensitive: false,
        }
    }

    /// Marks the path as sensitive.
    #[must_use]
    pub fn sensitive(mut self) -> Self {
        self.sensitive = true;
        self
    }
}

/// Pull request model used by proof, queue, and agent APIs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PullRequest {
    /// Repository id.
    pub repo: RepoId,
    /// Pull request id.
    pub id: PullRequestId,
    /// Source branch.
    pub source_branch: String,
    /// Target branch.
    pub target_branch: String,
    /// Base SHA.
    pub base_sha: String,
    /// Head SHA.
    pub head_sha: String,
    /// Changed paths.
    pub changed_paths: Vec<ChangedPath>,
}

impl PullRequest {
    /// Creates a pull request model for local proof and queue tests.
    pub fn new(
        repo: RepoId,
        id: PullRequestId,
        source_branch: impl Into<String>,
        target_branch: impl Into<String>,
        base_sha: impl Into<String>,
        head_sha: impl Into<String>,
        changed_paths: Vec<ChangedPath>,
    ) -> Self {
        Self {
            repo,
            id,
            source_branch: source_branch.into(),
            target_branch: target_branch.into(),
            base_sha: base_sha.into(),
            head_sha: head_sha.into(),
            changed_paths,
        }
    }

    /// Changed path set for deterministic queue conflict checks.
    #[must_use]
    pub fn changed_path_set(&self) -> BTreeSet<String> {
        self.changed_paths
            .iter()
            .map(|path| path.path.clone())
            .collect()
    }
}

/// Proof witness minted after all required lanes pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofWitness {
    /// Receipt id.
    pub id: ReceiptId,
    /// Repository id.
    pub repo: RepoId,
    /// Pull request id.
    pub pr: PullRequestId,
    /// Covered head SHA.
    pub head_sha: String,
    /// Covered changed paths.
    pub changed_paths: Vec<String>,
    /// Covered proof lanes.
    pub lanes: Vec<String>,
    /// Required owners.
    pub owners: Vec<String>,
    /// Underlying receipt.
    pub receipt: Receipt,
}

/// Merge queue entry state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueueEntryState {
    /// Waiting for speculative validation.
    Queued,
    /// Synthetic merge candidate is being validated.
    SpeculativeMergeTesting,
    /// Entry passed validation.
    Mergeable,
    /// Entry was dequeued due to a path conflict.
    DequeuedConflict,
    /// Entry was dequeued due to validation failure.
    DequeuedFailedValidation,
}

/// Merge queue entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueueEntry {
    /// Entry id.
    pub id: QueueEntryId,
    /// Pull request.
    pub pr: PullRequest,
    /// Admission proof witness.
    pub proof_witness: ProofWitness,
    /// Synthetic merge SHA, once scheduled.
    pub speculative_sha: Option<String>,
    /// Current state.
    pub state: QueueEntryState,
    /// Receipts emitted for queue actions.
    pub receipts: Vec<Receipt>,
}

/// Agent write scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentScope {
    /// Agent identity.
    pub agent: AgentId,
    /// Repository id.
    pub repo: RepoId,
    /// Allowed path prefixes.
    pub allowed_paths: Vec<String>,
    /// Maximum paths allowed in one mutation request.
    pub max_paths: usize,
}

impl AgentScope {
    /// Returns whether every path is within scope and below the mutation cap.
    #[must_use]
    pub fn permits_all(&self, paths: &[String]) -> bool {
        !paths.is_empty()
            && paths.len() <= self.max_paths
            && paths.iter().all(|path| {
                self.allowed_paths
                    .iter()
                    .any(|prefix| path == prefix || path.starts_with(prefix))
            })
    }
}
