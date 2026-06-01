//! The in-memory ForgeCore store.
//!
//! `ForgeCore` is the typed Phase 2 forge store. Its methods are grouped by the
//! resource they operate on into the submodules below; every `impl ForgeCore`
//! block extends the same type, so `crate::core::ForgeCore` and every public
//! method signature resolve exactly as before this file was split out of a
//! single `core.rs`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use parking_lot::{RwLock, RwLockWriteGuard};
use serde_json::Value;
use uuid::Uuid;

use crate::branch_protection::{BranchProtectionEvaluation, EvaluationContext};
use crate::errors::{ForgeError, Result};
use crate::model::*;
use crate::webhooks::{should_deliver, sign_webhook_payload};

mod accounts;
mod branch_protection;
mod check_runs;
mod commit_status;
mod issues;
mod pull_requests;
mod repositories;
mod reviews;
mod storage;
mod webhooks;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Default)]
struct Counters {
    issue: u64,
    pull: u64,
}

#[derive(Debug, Clone, Default)]
struct State {
    users: HashMap<String, User>,
    organizations: HashMap<String, Organization>,
    teams: HashMap<(String, String), Team>,
    repos: HashMap<(String, String), Repository>,
    labels: HashMap<(String, String, String), Label>,
    issues: HashMap<(String, String, u64), Issue>,
    issue_comments: HashMap<(String, String, u64), Vec<IssueComment>>,
    pulls: HashMap<(String, String, u64), PullRequest>,
    reviews: HashMap<(String, String, u64), Vec<Review>>,
    review_comments: HashMap<(String, String, u64), Vec<ReviewComment>>,
    branch_protections: HashMap<(String, String, String), BranchProtectionRule>,
    codeowners: HashMap<(String, String), String>,
    statuses: HashMap<(String, String, String), Vec<CommitStatus>>,
    check_runs: HashMap<(String, String), Vec<CheckRun>>,
    webhooks: HashMap<(String, String), Vec<Webhook>>,
    webhook_deliveries: Vec<WebhookDelivery>,
    counters: HashMap<(String, String), Counters>,
}

/// Materializes a newly created repository's bare git directory on disk.
///
/// Defined in the pure forge core so `create_repository` can trigger on-disk
/// creation without `jeryu-core` depending on the git-daemon crate: the unified
/// `jeryu serve` injects a `jeryu-gitd`-backed implementation via
/// [`ForgeCore::with_repo_materializer`]. With no materializer set (the default,
/// e.g. in unit tests) repository creation stays metadata-only.
pub trait RepoMaterializer: std::fmt::Debug + Send + Sync {
    /// Create the bare repository for `owner/name` with `default_branch` as its
    /// initial `HEAD`. Implementations MUST be idempotent: an already-present
    /// repository is success, not an error.
    fn materialize(&self, owner: &str, name: &str, default_branch: &str) -> Result<()>;
}

#[derive(Debug, Clone, Default)]
pub struct ForgeCore {
    state: Arc<RwLock<State>>,
    storage: Option<Arc<storage::SqliteStore>>,
    repo_materializer: Option<Arc<dyn RepoMaterializer>>,
}

impl ForgeCore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a [`RepoMaterializer`] so repository creation also writes a bare
    /// git repository to disk (used by the unified `jeryu serve`).
    #[must_use]
    pub fn with_repo_materializer(mut self, materializer: Arc<dyn RepoMaterializer>) -> Self {
        self.repo_materializer = Some(materializer);
        self
    }

    pub fn open_sqlite(path: impl AsRef<Path>) -> Result<Self> {
        let (storage, state) = storage::SqliteStore::open(path)?;
        Ok(Self {
            state: Arc::new(RwLock::new(state)),
            storage: Some(Arc::new(storage)),
            repo_materializer: None,
        })
    }

    fn ensure_repo_exists(&self, owner: &str, repo: &str) -> Result<()> {
        self.get_repository(owner, repo).map(|_| ())
    }

    fn persist_after_mutation(
        &self,
        state: &mut RwLockWriteGuard<'_, State>,
        previous: State,
    ) -> Result<()> {
        let Some(storage) = &self.storage else {
            return Ok(());
        };
        if let Err(error) = storage.persist(state) {
            **state = previous;
            return Err(error);
        }
        Ok(())
    }
}

fn require_name(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(ForgeError::Validation(format!("{field} cannot be empty")))
    } else {
        Ok(())
    }
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

fn next_issue_number(state: &mut State, owner: &str, repo: &str) -> u64 {
    let counters = state
        .counters
        .entry((owner.to_string(), repo.to_string()))
        .or_default();
    counters.issue += 1;
    counters.issue
}

fn next_pull_number(state: &mut State, owner: &str, repo: &str) -> u64 {
    let counters = state
        .counters
        .entry((owner.to_string(), repo.to_string()))
        .or_default();
    counters.pull += 1;
    counters.pull
}

fn evaluate_locked(
    state: &State,
    pr: &PullRequest,
    requested_sha: Option<&str>,
) -> BranchProtectionEvaluation {
    use crate::branch_protection::evaluate_branch_protection_with;

    let protection = state.branch_protections.get(&(
        pr.owner.clone(),
        pr.repo.clone(),
        pr.base.ref_name.clone(),
    ));
    let reviews = state
        .reviews
        .get(&(pr.owner.clone(), pr.repo.clone(), pr.number))
        .cloned()
        .unwrap_or_default();
    let statuses = state
        .statuses
        .get(&(pr.owner.clone(), pr.repo.clone(), pr.head.sha.clone()))
        .cloned()
        .unwrap_or_default();
    let check_runs = state
        .check_runs
        .get(&(pr.owner.clone(), pr.repo.clone()))
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|check| check.head_sha == pr.head.sha)
        .collect::<Vec<_>>();
    let codeowners = state.codeowners.get(&(pr.owner.clone(), pr.repo.clone()));
    let context = EvaluationContext {
        codeowners: codeowners.map(String::as_str),
        actor_is_admin: false,
    };
    evaluate_branch_protection_with(
        pr,
        protection,
        &reviews,
        &statuses,
        &check_runs,
        requested_sha,
        context,
    )
}

fn apply_evaluation(pr: &mut PullRequest, evaluation: BranchProtectionEvaluation) {
    // Terminal states are sticky. GitHub never resurrects a Merged or Closed PR
    // by recomputing mergeability on read: a merged PR stays merged, and a
    // closed PR stays closed until it is explicitly reopened. Previously only
    // `merged` was sticky, so a Closed PR with no blocking protection was
    // silently reverted to Mergeable on the next read (pinned by the former
    // `closing_a_mergeable_pr_does_not_stick`). This is the deliberate
    // correctness fix.
    if pr.merged || pr.state == PullRequestState::Merged {
        pr.mergeable = false;
        pr.mergeable_state = "merged".to_string();
        return;
    }
    if pr.state == PullRequestState::Closed {
        pr.mergeable = false;
        pr.mergeable_state = "closed".to_string();
        return;
    }
    pr.mergeable = evaluation.mergeable;
    pr.mergeable_state = evaluation.state;
    if pr.draft {
        pr.state = PullRequestState::Draft;
    } else if evaluation.mergeable {
        pr.state = PullRequestState::Mergeable;
    } else {
        pr.state = PullRequestState::BlockedByChecks;
    }
}

fn refresh_pull_mergeability_for_sha(state: &mut State, owner: &str, repo: &str, sha: &str) {
    let keys: Vec<_> = state
        .pulls
        .iter()
        .filter(|((pull_owner, pull_repo, _), pr)| {
            pull_owner == owner && pull_repo == repo && pr.head.sha == sha
        })
        .map(|(key, _)| key.clone())
        .collect();

    for key in keys {
        if let Some(snapshot) = state.pulls.get(&key).cloned() {
            let mut updated = snapshot;
            let evaluation = evaluate_locked(state, &updated, None);
            apply_evaluation(&mut updated, evaluation);
            state.pulls.insert(key, updated);
        }
    }
}

fn emit_event_locked(state: &mut State, owner: &str, repo: &str, event: &str, payload: Value) {
    let hooks = state
        .webhooks
        .get(&(owner.to_string(), repo.to_string()))
        .cloned()
        .unwrap_or_default();
    for hook in hooks.iter().filter(|hook| should_deliver(hook, event)) {
        // A delivery's payload is always an internal `json!(domain_struct)`
        // value, which cannot fail to serialize; encode it explicitly so a
        // hypothetical failure surfaces as a panic at the bug site rather than
        // being silently signed as an empty body.
        let payload_bytes = match serde_json::to_vec(&payload) {
            Ok(bytes) => bytes,
            Err(error) => unreachable!("forge webhook payload is always serializable: {error}"),
        };
        let signature_256 = hook
            .config
            .secret
            .as_ref()
            .map(|secret| sign_webhook_payload(secret, &payload_bytes));
        state.webhook_deliveries.push(WebhookDelivery {
            id: Uuid::new_v4(),
            hook_id: hook.id,
            owner: owner.to_string(),
            repo: repo.to_string(),
            event: event.to_string(),
            target_url: hook.config.url.clone(),
            payload: payload.clone(),
            signature_256,
            delivered: false,
            created_at: Utc::now(),
        });
    }
}
