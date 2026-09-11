//! Authenticated source reviews. The observer and SQLite writer are service
//! capabilities, never request fields. Required-check attempts and merge
//! execution are separate prerequisites, and remain unavailable in this cut.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ref_operations::{canonical_json, sha256};
use super::{AuthenticatedActor, ForgeCore, State, apply_evaluation, evaluate_locked};
use crate::*;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewCredentialKind {
    Session,
    PersonalAccessToken,
}

/// The profile UUID is paired with the actual account row and epoch; a display
/// profile or login alone is not an authentication identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewActorBinding {
    pub login: String,
    pub profile_id: Uuid,
    pub account_created_at: DateTime<Utc>,
    pub auth_epoch: u64,
    pub credential_kind: ReviewCredentialKind,
    pub credential_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewGitRepository {
    pub id: Uuid,
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewGitTarget {
    pub source: ReviewGitRepository,
    pub destination: ReviewGitRepository,
    pub source_ref: String,
    pub destination_ref: String,
}

/// Physical custody observed by the managed service. Catalog UUIDs are bound
/// separately in the target; a repository directory is not assigned a UUID by
/// interpreting its name. Installation must exclude unmanaged storage writers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedGitIdentity {
    pub storage_root: String,
    pub root_device: u64,
    pub root_inode: u64,
    pub repository_path: String,
    pub repository_device: u64,
    pub repository_inode: u64,
    pub git_executable: String,
    pub git_executable_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedReviewRef {
    pub reference: String,
    pub commit_sha: String,
    pub tree_sha: String,
    pub identity: ManagedGitIdentity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewGitObservation {
    pub source: ObservedReviewRef,
    pub destination: ObservedReviewRef,
}

/// Actual Git tree delta used by complete merge qualification. Paths are exact
/// UTF-8 repository-relative names; unsupported byte paths refuse observation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MergeGitChange {
    pub path: String,
    pub status: String,
    pub old_mode: String,
    pub new_mode: String,
    pub old_oid: String,
    pub new_oid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MergeGitObservation {
    pub refs: ReviewGitObservation,
    pub object_format: String,
    pub base_is_ancestor: bool,
    pub contains_merge_commits: bool,
    pub source_graph_verified: bool,
    pub destination_has_source_graph: bool,
    pub changes: Vec<MergeGitChange>,
}

/// Trusted service adapter. Core supplies targets from its guarded catalog and
/// PR; adapters must use exact direct refs and reject ambiguous Git/I/O results.
pub trait ReviewGitObserver: std::fmt::Debug + Send + Sync {
    fn storage_root(&self) -> &Path;
    fn observe(&self, target: &ReviewGitTarget) -> Result<ReviewGitObservation>;
    /// Ordinary ref-only observers cannot silently qualify a merge. Core must
    /// hold both UUID guards and the installed writer perimeter for this read.
    fn observe_merge(&self, _: &ReviewGitTarget) -> Result<MergeGitObservation> {
        Err(ForgeError::WriterUnavailable(
            "complete managed Git merge observation is unavailable".into(),
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewSnapshot {
    pub repository_id: Uuid,
    pub pull_request_id: Uuid,
    pub pull_number: u64,
    pub author: String,
    pub target: ReviewGitTarget,
    pub git: ReviewGitObservation,
    pub stored_head_sha: String,
    pub stored_base_sha: String,
    pub draft: bool,
    pub policy: Option<BranchProtectionRule>,
    pub codeowners: Option<String>,
    /// Digest of the complete persisted rule (including revision timestamp) and
    /// CODEOWNERS. This is not an invented policy or check-attempt identity.
    pub policy_revision: String,
    pub reviewer: ReviewActorBinding,
    pub destination_access: RepoAccessLevel,
    pub source_access: RepoAccessLevel,
    pub reviewer_role: UserRole,
    pub destination_grant: Option<RepoAccessGrant>,
    pub source_grant: Option<RepoAccessGrant>,
    pub bound_event_ids: Vec<Uuid>,
    pub advisory_reviews: Vec<Review>,
    pub advisory_statuses: Vec<CommitStatus>,
    pub advisory_check_runs: Vec<CheckRun>,
    pub advisory_commits: Vec<PullRequestCommit>,
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewChallenge {
    pub id: Uuid,
    pub nonce: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub snapshot: ReviewSnapshot,
    pub snapshot_sha256: String,
    pub merge_qualified: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SubmitBoundReviewRequest {
    pub challenge_id: Uuid,
    pub nonce: String,
    pub expected_head_sha: String,
    pub event: ReviewState,
    pub body: Option<String>,
    #[serde(default)]
    pub comments: Vec<ReviewCommentInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DismissBoundReviewRequest {
    pub challenge_id: Uuid,
    pub nonce: String,
    pub expected_head_sha: String,
    pub review_id: Uuid,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundReviewEvent {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub pull_request_id: Uuid,
    pub pull_number: u64,
    pub sequence: u64,
    pub challenge_id: Uuid,
    pub actor: ReviewActorBinding,
    pub snapshot: ReviewSnapshot,
    pub snapshot_sha256: String,
    pub request_sha256: String,
    pub review: Review,
    pub comments: Vec<ReviewComment>,
    pub audit_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewQualification {
    pub repository_id: Uuid,
    pub pull_number: u64,
    pub head_sha: String,
    pub effective_reviews: Vec<Review>,
    pub advisory_reviews: Vec<Review>,
    pub bound_review_ids: Vec<Uuid>,
    pub merge_qualified: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundReviewHistory {
    pub events: Vec<BoundReviewEvent>,
    pub qualification: ReviewQualification,
}

impl ForgeCore {
    /// Attach once to the shared managed runtime. Reopening/cloning the same
    /// backing pair cannot create a competing observer configuration.
    pub fn with_review_git_observer(self, observer: Arc<dyn ReviewGitObserver>) -> Result<Self> {
        self.with_global_mutation(|| {
            let root = self.runtime.storage_root.as_deref().ok_or_else(|| {
                ForgeError::WriterUnavailable(
                    "review observation requires open_managed custody".into(),
                )
            })?;
            if observer.storage_root() != root {
                return Err(ForgeError::Conflict(
                    "review observer storage root differs from managed custody".into(),
                ));
            }
            let mut installed = self.runtime.review_git_observer.write();
            match installed.as_ref() {
                Some(current) if Arc::ptr_eq(current, &observer) => Ok(()),
                Some(_) => Err(ForgeError::Conflict(
                    "a review observer is already bound to this runtime".into(),
                )),
                None => {
                    *installed = Some(observer);
                    Ok(())
                }
            }
        })?;
        Ok(self)
    }

    pub fn create_review_challenge(
        &self,
        actor: &AuthenticatedActor,
        repository_id: Uuid,
        pull_number: u64,
        expected_head_sha: &str,
    ) -> Result<ReviewChallenge> {
        validate_oid(expected_head_sha)?;
        self.with_review_context(
            actor,
            repository_id,
            pull_number,
            true,
            |target, pr, binding| {
                let snapshot = self.observe_review_snapshot(target, pr, binding)?;
                if snapshot.git.source.commit_sha != expected_head_sha {
                    return Err(ForgeError::Conflict(
                        "requested, stored and live pull head must agree".into(),
                    ));
                }
                self.validate_actor_locked(&self.runtime.state.read(), actor, true)?;
                let now = Utc::now();
                let challenge = ReviewChallenge {
                    id: Uuid::new_v4(),
                    nonce: super::auth::random_secret()?,
                    created_at: now,
                    expires_at: now + Duration::minutes(10),
                    snapshot_sha256: sha256(&canonical_json(&snapshot)?),
                    snapshot,
                    merge_qualified: false,
                    blockers: unavailable_blockers(),
                };
                self.review_storage()?.insert_review_challenge(&challenge)?;
                Ok(challenge)
            },
        )
    }

    pub fn submit_bound_review(
        &self,
        actor: &AuthenticatedActor,
        repository_id: Uuid,
        pull_number: u64,
        request: SubmitBoundReviewRequest,
    ) -> Result<BoundReviewEvent> {
        if request.event == ReviewState::Dismissed {
            return Err(ForgeError::Validation(
                "dismissal requires an explicit target review and reason".into(),
            ));
        }
        for comment in &request.comments {
            if comment.path.is_empty()
                || comment.path.starts_with('/')
                || comment
                    .path
                    .split('/')
                    .any(|part| matches!(part, "" | "." | ".."))
                || comment.body.trim().is_empty()
                || comment.line == Some(0)
            {
                return Err(ForgeError::Validation(
                    "review comments require a relative path, nonempty body and positive line"
                        .into(),
                ));
            }
        }
        let digest = sha256(&canonical_json(&request)?);
        self.record_bound_review(actor, repository_id, pull_number, request, None, digest)
    }

    pub fn dismiss_bound_review(
        &self,
        actor: &AuthenticatedActor,
        repository_id: Uuid,
        pull_number: u64,
        request: DismissBoundReviewRequest,
    ) -> Result<BoundReviewEvent> {
        if request.reason.trim().is_empty() {
            return Err(ForgeError::Validation(
                "review dismissal requires a reason".into(),
            ));
        }
        let digest = sha256(&canonical_json(&request)?);
        self.record_bound_review(
            actor,
            repository_id,
            pull_number,
            SubmitBoundReviewRequest {
                challenge_id: request.challenge_id,
                nonce: request.nonce,
                expected_head_sha: request.expected_head_sha,
                event: ReviewState::Dismissed,
                body: Some(request.reason),
                comments: Vec::new(),
            },
            Some(request.review_id),
            digest,
        )
    }

    fn record_bound_review(
        &self,
        actor: &AuthenticatedActor,
        repository_id: Uuid,
        pull_number: u64,
        request: SubmitBoundReviewRequest,
        dismissed_review_id: Option<Uuid>,
        request_sha256: String,
    ) -> Result<BoundReviewEvent> {
        validate_oid(&request.expected_head_sha)?;
        self.with_review_context(actor, repository_id, pull_number, true, |target, pr, binding| {
            let storage = self.review_storage()?;
            let (challenge, accepted) = storage.load_review_challenge(request.challenge_id)?;
            if challenge.snapshot.repository_id != repository_id
                || challenge.snapshot.pull_request_id != pr.id
                || challenge.snapshot.pull_number != pull_number
                || challenge.snapshot.reviewer != *binding
                || challenge.snapshot.git.source.commit_sha != request.expected_head_sha
                || !super::auth::constant_time_eq(challenge.nonce.as_bytes(), request.nonce.as_bytes())
            {
                return Err(ForgeError::Conflict("review challenge binding does not match this request".into()));
            }
            // Lost-return retry is readback of the original immutable decision.
            // Current credentials/grants were revalidated above. Expiry and new
            // refs do not rewrite an already accepted event.
            if let Some(accepted) = accepted {
                return if accepted.request_sha256 == request_sha256 { Ok(accepted) }
                    else { Err(ForgeError::Conflict("review challenge was consumed by a different request".into())) };
            }
            if challenge.expires_at <= Utc::now() {
                return Err(ForgeError::Conflict("review challenge expired; obtain a fresh challenge".into()));
            }
            let observed = self.observe_review_snapshot(target, pr, binding)?;
            if observed != challenge.snapshot
                || sha256(&canonical_json(&observed)?) != challenge.snapshot_sha256
            {
                return Err(ForgeError::Conflict("review refs, policy, authority or evidence changed; obtain a fresh challenge".into()));
            }
            // Wall-clock expiry can pass while Git is observed even though no
            // authority mutation can enter the continuously held guards.
            if challenge.expires_at <= Utc::now() {
                return Err(ForgeError::Conflict("review challenge expired during Git observation".into()));
            }
            self.validate_actor_locked(&self.runtime.state.read(), actor, true)?;
            if request.event == ReviewState::Approved && pr.author.eq_ignore_ascii_case(&binding.login) {
                return Err(ForgeError::Forbidden("pull request authors cannot approve their own changes".into()));
            }
            let mut state = self.runtime.state.write();
            if let Some(target_id) = dismissed_review_id {
                let current = reduced_bound_events(&state, pr);
                let event = current.iter().find(|event| event.review.id == target_id)
                    .ok_or_else(|| ForgeError::Conflict("dismissal target is not a current bound verdict".into()))?;
                if event.actor.profile_id != binding.profile_id || event.actor.login != binding.login {
                    return Err(ForgeError::Forbidden("reviewers may dismiss only their own verdicts".into()));
                }
            }
            let now = Utc::now();
            let id = Uuid::new_v4();
            let review = Review { id, owner: pr.owner.clone(), repo: pr.repo.clone(), pull_number,
                author: binding.login.clone(), state: request.event, body: request.body,
                head_sha: Some(observed.git.source.commit_sha.clone()), dismissed_review_id, submitted_at: now };
            let comments = request.comments.into_iter().map(|comment| ReviewComment {
                id: Uuid::new_v4(), review_id: id, owner: pr.owner.clone(), repo: pr.repo.clone(),
                pull_number, path: comment.path, line: comment.line, author: binding.login.clone(),
                body: comment.body, created_at: now,
            }).collect::<Vec<_>>();
            let sequence = state.bound_reviews.iter().filter(|event|
                event.repository_id == repository_id && event.pull_request_id == pr.id)
                .map(|event| event.sequence).max().unwrap_or(0).checked_add(1)
                .ok_or_else(|| ForgeError::Storage("bound review sequence exhausted".into()))?;
            let event = BoundReviewEvent { id, repository_id, pull_request_id: pr.id, pull_number,
                sequence, challenge_id: challenge.id, actor: binding.clone(), snapshot: observed,
                snapshot_sha256: challenge.snapshot_sha256.clone(), request_sha256,
                review: review.clone(), comments: comments.clone(), audit_id: Uuid::new_v4() };
            let mut proposed = state.clone();
            let key = (pr.owner.clone(), pr.repo.clone(), pull_number);
            proposed.reviews.entry(key.clone()).or_default().push(review);
            proposed.review_comments.entry(key.clone()).or_default().extend(comments);
            proposed.bound_reviews.push(event.clone());
            let mut updated_pr = pr.clone();
            apply_evaluation(&mut updated_pr, evaluate_locked(&proposed, pr, None));
            proposed.pulls.insert(key, updated_pr);
            // Independent event + nonce consumption + compatibility comments +
            // audit commit together. Failed SQL leaves shared State untouched.
            storage.commit_bound_review(&challenge, &event, &proposed)?;
            *state = proposed;
            Ok(event)
        })
    }

    pub fn bound_review_history(
        &self,
        actor: &AuthenticatedActor,
        repository_id: Uuid,
        pull_number: u64,
    ) -> Result<BoundReviewHistory> {
        self.with_review_context(actor, repository_id, pull_number, false, |_, pr, _| {
            let state = self.runtime.state.read();
            Ok(BoundReviewHistory {
                events: state
                    .bound_reviews
                    .iter()
                    .filter(|event| {
                        event.repository_id == repository_id && event.pull_request_id == pr.id
                    })
                    .cloned()
                    .collect(),
                qualification: qualification_locked(&state, pr, repository_id),
            })
        })
    }

    /// Read-model service projection. HTTP adapters still enforce their read
    /// authentication policy; this returns no secret or mintable capability.
    pub fn review_qualification(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<ReviewQualification> {
        self.validate_mutation_process()?;
        self.runtime.coordinator.with_repositories(&[], || {
            let state = self.runtime.state.read();
            let repository = state
                .repos
                .get(&(owner.into(), repo.into()))
                .ok_or_else(|| ForgeError::NotFound(format!("repository {owner}/{repo}")))?;
            let pr = state
                .pulls
                .get(&(owner.into(), repo.into(), number))
                .ok_or_else(|| {
                    ForgeError::NotFound(format!("pull request {owner}/{repo}#{number}"))
                })?;
            Ok(qualification_locked(&state, pr, repository.id))
        })
    }

    fn with_review_context<T>(
        &self,
        actor: &AuthenticatedActor,
        repository_id: Uuid,
        number: u64,
        mutation: bool,
        operation: impl FnOnce(&ReviewGitTarget, &PullRequest, &ReviewActorBinding) -> Result<T>,
    ) -> Result<T> {
        self.validate_mutation_process()?;
        let hint = self.runtime.coordinator.with_repositories(&[], || {
            let state = self.runtime.state.read();
            self.validate_actor_locked(&state, actor, mutation)?;
            review_target(&state, repository_id, number).map(|(target, _)| target)
        })?;
        self.runtime
            .coordinator
            .with_repositories(&[hint.source.id, hint.destination.id], || {
                let (target, pr, binding) = {
                    let state = self.runtime.state.read();
                    let binding = self.validate_actor_locked(&state, actor, mutation)?;
                    let (target, pr) = review_target(&state, repository_id, number)?;
                    if target != hint {
                        return Err(ForgeError::Conflict(
                            "review repository/ref identity changed".into(),
                        ));
                    }
                    super::mutation::require_repository_admissible(
                        &state,
                        target.destination.id,
                        mutation,
                    )?;
                    super::mutation::require_repository_admissible(
                        &state,
                        target.source.id,
                        false,
                    )?;
                    require_access(&state, &binding.login, &target.destination, mutation)?;
                    require_access(&state, &binding.login, &target.source, false)?;
                    (target, pr.clone(), binding)
                };
                operation(&target, &pr, &binding)
            })
    }

    fn observe_review_snapshot(
        &self,
        target: &ReviewGitTarget,
        pr: &PullRequest,
        binding: &ReviewActorBinding,
    ) -> Result<ReviewSnapshot> {
        if pr.merged
            || matches!(
                pr.state,
                PullRequestState::Closed | PullRequestState::Merged
            )
        {
            return Err(ForgeError::Conflict(
                "closed pull request cannot accept a new review".into(),
            ));
        }
        let observer = self
            .runtime
            .review_git_observer
            .read()
            .clone()
            .ok_or_else(|| {
                ForgeError::WriterUnavailable("managed Git review observer is unavailable".into())
            })?;
        let git = observer.observe(target)?;
        for observed in [&git.source, &git.destination] {
            validate_oid(&observed.commit_sha)?;
            validate_oid(&observed.tree_sha)?;
            if Some(Path::new(&observed.identity.storage_root))
                != self.runtime.storage_root.as_deref()
            {
                return Err(ForgeError::WriterUnavailable(
                    "observed Git root does not match managed custody".into(),
                ));
            }
        }
        if git.source.reference != target.source_ref
            || git.destination.reference != target.destination_ref
            || git.source.commit_sha != pr.head.sha
        {
            return Err(ForgeError::Conflict(
                "requested, stored and live pull head/ref must agree".into(),
            ));
        }
        let state = self.runtime.state.read();
        let policy = state
            .branch_protections
            .get(&(pr.owner.clone(), pr.repo.clone(), pr.base.ref_name.clone()))
            .cloned();
        let codeowners = state
            .codeowners
            .get(&(pr.owner.clone(), pr.repo.clone()))
            .cloned();
        let policy_revision = sha256(&canonical_json(&(&policy, &codeowners))?);
        let bound_ids = state
            .bound_reviews
            .iter()
            .filter(|event| {
                event.repository_id == target.destination.id && event.pull_request_id == pr.id
            })
            .map(|event| event.id)
            .collect::<Vec<_>>();
        let bound_set = bound_ids.iter().copied().collect::<BTreeSet<_>>();
        let mut advisory_statuses = state
            .statuses
            .get(&(pr.owner.clone(), pr.repo.clone(), pr.head.sha.clone()))
            .cloned()
            .unwrap_or_default();
        advisory_statuses.sort_by_key(|status| status.id);
        let mut advisory_check_runs = state
            .check_runs
            .get(&(pr.owner.clone(), pr.repo.clone()))
            .into_iter()
            .flatten()
            .filter(|check| check.head_sha == pr.head.sha)
            .cloned()
            .collect::<Vec<_>>();
        advisory_check_runs.sort_by_key(|check| check.id);
        Ok(ReviewSnapshot {
            repository_id: target.destination.id,
            pull_request_id: pr.id,
            pull_number: pr.number,
            author: pr.author.clone(),
            target: target.clone(),
            git,
            stored_head_sha: pr.head.sha.clone(),
            stored_base_sha: pr.base.sha.clone(),
            draft: pr.draft,
            policy,
            codeowners,
            policy_revision,
            reviewer: binding.clone(),
            destination_access: require_access(&state, &binding.login, &target.destination, true)?,
            source_access: require_access(&state, &binding.login, &target.source, false)?,
            reviewer_role: state
                .accounts
                .get(&binding.login)
                .expect("actor validated under authority guard")
                .role
                .clone(),
            destination_grant: state
                .repo_grants
                .get(&(
                    binding.login.clone(),
                    target.destination.owner.clone(),
                    target.destination.name.clone(),
                ))
                .cloned(),
            source_grant: state
                .repo_grants
                .get(&(
                    binding.login.clone(),
                    target.source.owner.clone(),
                    target.source.name.clone(),
                ))
                .cloned(),
            bound_event_ids: bound_ids,
            advisory_reviews: state
                .reviews
                .get(&(pr.owner.clone(), pr.repo.clone(), pr.number))
                .into_iter()
                .flatten()
                .filter(|review| !bound_set.contains(&review.id))
                .cloned()
                .collect(),
            advisory_statuses,
            advisory_check_runs,
            advisory_commits: pr.commits.clone(),
            changed_files: pr.changed_files.clone(),
        })
    }

    fn review_storage(&self) -> Result<&super::storage::SqliteStore> {
        self.runtime.storage.as_ref().ok_or_else(|| {
            ForgeError::WriterUnavailable(
                "authenticated review events require durable storage".into(),
            )
        })
    }
}

fn review_target(
    state: &State,
    repository_id: Uuid,
    number: u64,
) -> Result<(ReviewGitTarget, &PullRequest)> {
    let destination = state
        .repos
        .values()
        .find(|repo| repo.id == repository_id)
        .ok_or_else(|| ForgeError::NotFound(format!("repository {repository_id}")))?;
    let pr = state
        .pulls
        .get(&(destination.owner.clone(), destination.name.clone(), number))
        .ok_or_else(|| ForgeError::NotFound(format!("pull request {repository_id}#{number}")))?;
    let source_name = if pr.source_repository.is_empty() {
        destination.full_name.as_str()
    } else {
        &pr.source_repository
    };
    let (source_owner, source_repo) = source_name
        .split_once('/')
        .ok_or_else(|| ForgeError::Validation("invalid stored source repository".into()))?;
    let source = state
        .repos
        .get(&(source_owner.into(), source_repo.into()))
        .ok_or_else(|| ForgeError::NotFound(format!("source repository {source_name}")))?;
    let repository = |repo: &Repository| ReviewGitRepository {
        id: repo.id,
        owner: repo.owner.clone(),
        name: repo.name.clone(),
    };
    Ok((
        ReviewGitTarget {
            source: repository(source),
            destination: repository(destination),
            source_ref: branch_ref(&pr.head.ref_name)?,
            destination_ref: branch_ref(&pr.base.ref_name)?,
        },
        pr,
    ))
}

fn require_access(
    state: &State,
    login: &str,
    repo: &ReviewGitRepository,
    write: bool,
) -> Result<RepoAccessLevel> {
    let account = state
        .accounts
        .get(login)
        .ok_or_else(|| ForgeError::Unauthenticated("account disappeared".into()))?;
    let access = if account.role == UserRole::Admin {
        Some(RepoAccessLevel::Admin)
    } else {
        state
            .repo_grants
            .get(&(login.into(), repo.owner.clone(), repo.name.clone()))
            .map(|grant| grant.access)
            .or_else(|| {
                (!write
                    && state
                        .repos
                        .get(&(repo.owner.clone(), repo.name.clone()))
                        .is_some_and(|repo| !repo.private))
                .then_some(RepoAccessLevel::Read)
            })
    };
    access
        .filter(|access| {
            if write {
                access.allows_write()
            } else {
                access.allows_read()
            }
        })
        .ok_or_else(|| {
            ForgeError::Forbidden(format!(
                "current repository {} access is required",
                if write { "write" } else { "read" }
            ))
        })
}

pub(super) fn bound_reviews_for_evaluation(state: &State, pr: &PullRequest) -> Vec<Review> {
    let policy = state.branch_protections.get(&(
        pr.owner.clone(),
        pr.repo.clone(),
        pr.base.ref_name.clone(),
    ));
    let codeowners = state.codeowners.get(&(pr.owner.clone(), pr.repo.clone()));
    reduced_bound_events(state, pr)
        .into_iter()
        .filter(|event| {
            // A permission/policy change may withdraw positive qualification, but
            // never erase a rejection or make an older approval reappear.
            if event.review.state == ReviewState::ChangesRequested {
                return true;
            }
            event.snapshot.policy.as_ref() == policy
                && event.snapshot.codeowners.as_ref() == codeowners
                && super::actors::validate_actor_binding(state, &event.actor).is_ok()
                && require_access(
                    state,
                    &event.actor.login,
                    &event.snapshot.target.destination,
                    true,
                )
                .is_ok()
        })
        .map(|event| event.review.clone())
        .collect()
}

fn reduced_bound_events<'a>(state: &'a State, pr: &PullRequest) -> Vec<&'a BoundReviewEvent> {
    let mut latest = BTreeMap::new();
    let mut events = state
        .bound_reviews
        .iter()
        .filter(|event| {
            event.pull_request_id == pr.id
                && event.review.head_sha.as_deref() == Some(pr.head.sha.as_str())
        })
        .collect::<Vec<_>>();
    events.sort_by_key(|event| event.sequence);
    for event in events {
        match event.review.state {
            ReviewState::Approved if pr.author.eq_ignore_ascii_case(&event.actor.login) => {}
            ReviewState::Approved | ReviewState::ChangesRequested => {
                latest.insert(event.actor.profile_id, event);
            }
            ReviewState::Commented => {}
            ReviewState::Dismissed => {
                if latest
                    .get(&event.actor.profile_id)
                    .is_some_and(|current: &&BoundReviewEvent| {
                        Some(current.review.id) == event.review.dismissed_review_id
                    })
                {
                    latest.remove(&event.actor.profile_id);
                }
            }
        }
    }
    latest.into_values().collect()
}

fn qualification_locked(
    state: &State,
    pr: &PullRequest,
    repository_id: Uuid,
) -> ReviewQualification {
    let bound_review_ids = state
        .bound_reviews
        .iter()
        .filter(|event| event.repository_id == repository_id && event.pull_request_id == pr.id)
        .map(|event| event.review.id)
        .collect::<Vec<_>>();
    ReviewQualification {
        repository_id,
        pull_number: pr.number,
        head_sha: pr.head.sha.clone(),
        effective_reviews: bound_reviews_for_evaluation(state, pr),
        advisory_reviews: state
            .reviews
            .get(&(pr.owner.clone(), pr.repo.clone(), pr.number))
            .into_iter()
            .flatten()
            .filter(|review| !bound_review_ids.contains(&review.id))
            .cloned()
            .collect(),
        bound_review_ids,
        merge_qualified: false,
        blockers: unavailable_blockers(),
    }
}

fn unavailable_blockers() -> Vec<String> {
    vec![
        "authoritative required-check attempts are unavailable; legacy checks are advisory".into(),
        "Core-owned merge execution and fresh live-ref qualification are unavailable".into(),
    ]
}

fn validate_oid(oid: &str) -> Result<()> {
    if oid.len() != 40
        || oid
            .bytes()
            .any(|byte| !matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        || oid.bytes().all(|byte| byte == b'0')
    {
        return Err(ForgeError::Validation(
            "a full nonzero lowercase SHA-1 is required".into(),
        ));
    }
    Ok(())
}

fn branch_ref(branch: &str) -> Result<String> {
    let full = if branch.starts_with("refs/") {
        branch.into()
    } else {
        format!("refs/heads/{branch}")
    };
    if !full.starts_with("refs/heads/")
        || full.ends_with('/')
        || full.ends_with('.')
        || full.contains("..")
        || full.contains("@{")
        || full.contains("//")
        || full
            .bytes()
            .any(|byte| byte <= b' ' || byte == 127 || b"~^:?*[\\".contains(&byte))
        || full
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"))
    {
        return Err(ForgeError::Validation(
            "review refs must be full direct branch names".into(),
        ));
    }
    Ok(full)
}
