//! Authenticated publisher controllers are not a substitute
//! for the independently verified root-owned commissioning gate. A deployed
//! runtime without that gate and a persisted enrollment refuses publication.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::publisher_enrollment::{DurableRequiredPublisher, RequiredPublisherScope};
use super::required_attempts::RequiredArtifactBytes;
use super::{
    AuthenticatedActor, DurableRequiredAttempt, ForgeCore, RequiredAttemptConclusion,
    RequiredAttemptStatus, ReviewActorBinding, ReviewGitRepository, ReviewGitTarget, State,
};
use crate::{ForgeError, RepoAccessLevel, Result, UserRole};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredPublisherAction {
    Reserve,
    Complete,
    TerminalReplay,
    Snapshot,
}

/// Trusted installation service, not a request DTO or publisher assertion.
/// The implementation must verify the root-owned detached acceptance/trust
/// records, current epochs/revocation, installed runtime/origin and held custody
/// for every call. No production implementation is supplied in this slice.
pub trait RequiredPublisherAuthority: std::fmt::Debug + Send + Sync {
    fn custody(&self) -> &super::RequiredPublisherCustody;
    fn validate(
        &self,
        custody: &super::RequiredPublisherCustody,
        publisher: &DurableRequiredPublisher,
        action: RequiredPublisherAction,
        now: DateTime<Utc>,
    ) -> Result<()>;
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReserveRequiredAttemptRequest {
    pub publisher_id: Uuid,
    pub expected_head_sha: Option<String>,
    pub expected_tree_sha: Option<String>,
    pub context: String,
    pub idempotency_key: Option<String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredArtifactUpload {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteRequiredAttemptRequest {
    pub expected_head_sha: Option<String>,
    pub expected_reservation_sha256: Option<String>,
    pub conclusion: RequiredAttemptConclusion,
    pub artifacts: Vec<RequiredArtifactUpload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredContextSnapshot {
    pub context: String,
    pub newest_attempt: Option<DurableRequiredAttempt>,
    pub status: Option<RequiredAttemptStatus>,
    pub satisfied: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredAttemptSnapshot {
    pub repository_id: Uuid,
    pub commit_sha: String,
    pub observed_at: DateTime<Utc>,
    pub policy_sha256: String,
    pub contexts: Vec<RequiredContextSnapshot>,
    pub satisfied: bool,
    pub blockers: Vec<String>,
}

impl ForgeCore {
    /// Installation-only capability, like the managed Git observer. An HTTP,
    /// MCP or compatibility caller cannot provide or replace this object.
    pub fn with_required_publisher_authority(
        self,
        authority: Arc<dyn RequiredPublisherAuthority>,
    ) -> Result<Self> {
        self.with_installation_custody(|| {
            self.compare_required_custody(authority.as_ref())?;
            let mut slot = self.runtime.required_publisher_authority.write();
            match slot.as_ref() {
                Some(current) if Arc::ptr_eq(current, &authority) => Ok(()),
                Some(_) => Err(ForgeError::Conflict(
                    "publisher authority is already attached".into(),
                )),
                None => {
                    *slot = Some(authority);
                    Ok(())
                }
            }
        })?;
        Ok(self)
    }

    pub fn reserve_required_attempt(
        &self,
        actor: &AuthenticatedActor,
        repository_id: Uuid,
        request: ReserveRequiredAttemptRequest,
    ) -> Result<DurableRequiredAttempt> {
        let head = required(request.expected_head_sha.as_deref(), "expected_head_sha")?;
        let tree = required(request.expected_tree_sha.as_deref(), "expected_tree_sha")?;
        let key = required(request.idempotency_key.as_deref(), "idempotency_key")?;
        if repository_id.is_nil()
            || request.publisher_id.is_nil()
            || !super::required_attempts::full_oid(head)
            || !super::required_attempts::full_oid(tree)
            || head.len() != tree.len()
            || !super::required_attempts::safe_text(&request.context, 256)
            || !super::required_attempts::safe_text(key, 256)
        {
            return Err(ForgeError::Validation(
                "malformed exact publisher reservation scope".into(),
            ));
        }
        self.validate_mutation_process()?;
        self.runtime.coordinator.with_repositories(&[repository_id], || {
            self.require_ordinary_mutation()?;
            let (repository, binding) = self.required_actor_context(actor, repository_id, true)?;
            let storage = self.runtime.storage.as_ref().ok_or_else(unavailable)?;
            let publisher = storage.latest_required_publisher(request.publisher_id)?.ok_or_else(|| ForgeError::Forbidden("publisher is not enrolled".into()))?;
            let now = Utc::now();
            publisher.require_current(now)?;
            require_publisher_actor(&publisher, &binding)?;
            let scope = publisher.enrollment.scope(repository_id, head, &request.context)?;
            if scope.tree_sha != tree { return Err(ForgeError::Conflict("requested tree differs from enrolled source".into())); }
            if request.expires_at <= now || request.expires_at > publisher.enrollment.expires_at
                || request.expires_at.signed_duration_since(now) > Duration::seconds(7200) {
                return Err(ForgeError::Conflict("required attempt must fit the fixed enrolled window and 7200-second maximum".into()));
            }
            let authority = self.required_authority()?;
            self.validate_required_authority(authority.as_ref(), &publisher, RequiredPublisherAction::Reserve, now)?;
            self.observe_required_source(&repository, scope)?;
            self.required_actor_context(actor, repository_id, true)?;
            let now = Utc::now();
            publisher.require_current(now)?;
            self.validate_required_authority(authority.as_ref(), &publisher, RequiredPublisherAction::Reserve, now)?;
            let reservation = publisher.enrollment.reservation(scope, key, request.expires_at, publisher.enrollment_sha256.clone());
            storage.reserve_required_attempt(&reservation, now)
        })
    }

    pub fn complete_required_attempt(
        &self,
        actor: &AuthenticatedActor,
        repository_id: Uuid,
        attempt_id: Uuid,
        request: CompleteRequiredAttemptRequest,
    ) -> Result<DurableRequiredAttempt> {
        let head = required(request.expected_head_sha.as_deref(), "expected_head_sha")?;
        let digest = required(
            request.expected_reservation_sha256.as_deref(),
            "expected_reservation_sha256",
        )?;
        if repository_id.is_nil()
            || attempt_id.is_nil()
            || !super::required_attempts::full_oid(head)
            || !super::required_attempts::full_sha256(digest)
        {
            return Err(ForgeError::Validation(
                "malformed required completion preconditions".into(),
            ));
        }
        let size = request
            .artifacts
            .iter()
            .try_fold(0usize, |total, artifact| {
                total.checked_add(artifact.bytes.len())
            });
        if request.artifacts.is_empty()
            || request.artifacts.len() > 64
            || size.is_none_or(|size| size > 16 * 1024 * 1024)
        {
            return Err(ForgeError::Validation(
                "required receiving evidence exceeds count or byte limit".into(),
            ));
        }
        self.validate_mutation_process()?;
        self.runtime
            .coordinator
            .with_repositories(&[repository_id], || {
                let (repository, binding) =
                    self.required_actor_context(actor, repository_id, true)?;
                let storage = self.runtime.storage.as_ref().ok_or_else(unavailable)?;
                let attempt = storage.get_required_attempt(attempt_id)?;
                let reservation = &attempt.reservation;
                if reservation.binding.repository_id != repository_id
                    || reservation.binding.commit_sha != head
                    || attempt.reservation_sha256 != digest
                {
                    return Err(ForgeError::Conflict(
                        "completion repository/head/reservation precondition changed".into(),
                    ));
                }
                let publisher = storage
                    .latest_required_publisher(reservation.binding.publisher_id)?
                    .ok_or_else(|| {
                        ForgeError::Forbidden("publisher enrollment disappeared".into())
                    })?;
                require_publisher_actor(&publisher, &binding)?;
                if publisher.enrollment_sha256 != reservation.binding.enrollment_sha256
                    || publisher.revocation.is_some()
                {
                    return Err(ForgeError::Forbidden(
                        "publisher enrollment rotated or revoked".into(),
                    ));
                }
                let scope = validate_attempt_enrollment(&publisher, &attempt)?;
                let action = if attempt.completion.is_some() {
                    RequiredPublisherAction::TerminalReplay
                } else {
                    RequiredPublisherAction::Complete
                };
                if action == RequiredPublisherAction::Complete {
                    self.require_ordinary_mutation()?;
                }
                let authority = self.required_authority()?;
                self.validate_required_authority(
                    authority.as_ref(),
                    &publisher,
                    action,
                    Utc::now(),
                )?;
                // A terminal replay is readback of immutable bytes. It must remain
                // currently authenticated and unrevoked but does not renew expiry,
                // require the old branch still exist or create a new publication.
                if action == RequiredPublisherAction::Complete {
                    publisher.require_current(Utc::now())?;
                    self.observe_required_source(&repository, scope)?;
                }
                self.required_actor_context(actor, repository_id, true)?;
                let now = Utc::now();
                if action == RequiredPublisherAction::Complete {
                    publisher.require_current(now)?;
                }
                self.validate_required_authority(authority.as_ref(), &publisher, action, now)?;
                let bytes = request
                    .artifacts
                    .iter()
                    .map(|artifact| RequiredArtifactBytes {
                        name: artifact.name.clone(),
                        bytes: artifact.bytes.clone(),
                    })
                    .collect::<Vec<_>>();
                storage.complete_required_attempt(
                    attempt_id,
                    reservation,
                    request.conclusion,
                    &bytes,
                    now,
                )
            })
    }

    pub fn required_attempt_snapshot(
        &self,
        actor: &AuthenticatedActor,
        repository_id: Uuid,
        commit_sha: &str,
    ) -> Result<RequiredAttemptSnapshot> {
        if repository_id.is_nil() || !super::required_attempts::full_oid(commit_sha) {
            return Err(ForgeError::Validation(
                "full exact commit is required".into(),
            ));
        }
        self.validate_mutation_process()?;
        self.runtime
            .coordinator
            .with_repositories(&[repository_id], || {
                let (repository, _) = self.required_actor_context(actor, repository_id, false)?;
                let storage = self.runtime.storage.as_ref().ok_or_else(unavailable)?;
                let (branch, policy) = {
                    let state = self.runtime.state.read();
                    let repo = state
                        .repos
                        .get(&(repository.owner.clone(), repository.name.clone()))
                        .ok_or_else(|| ForgeError::Conflict("repository catalog changed".into()))?;
                    (
                        repo.default_branch.clone(),
                        state
                            .branch_protections
                            .get(&(
                                repository.owner.clone(),
                                repository.name.clone(),
                                repo.default_branch.clone(),
                            ))
                            .cloned(),
                    )
                };
                let mut blockers = Vec::new();
                let contexts = match &policy {
                    Some(policy) if !policy.required_status_checks.is_empty() => {
                        policy.required_status_checks.clone()
                    }
                    _ => {
                        blockers.push("required context policy is absent".into());
                        Vec::new()
                    }
                };
                let authority = self.required_authority()?;
                let now = Utc::now();
                let mut results = Vec::new();
                for context in contexts {
                    let newest =
                        storage.latest_required_attempt(repository_id, commit_sha, &context)?;
                    let mut failures = Vec::new();
                    let status = newest.as_ref().map(|attempt| attempt.status_at(now));
                    if let Some(attempt) = &newest {
                        let publisher = storage
                            .latest_required_publisher(attempt.reservation.binding.publisher_id)?;
                        match publisher {
                            Some(publisher)
                                if publisher.revocation.is_none()
                                    && publisher.enrollment_sha256
                                        == attempt.reservation.binding.enrollment_sha256 =>
                            {
                                if let Err(error) = validate_attempt_enrollment(&publisher, attempt)
                                {
                                    failures.push(error.to_string());
                                }
                                if let Err(error) = self.validate_actor_binding_locked(
                                    &self.runtime.state.read(),
                                    &publisher.enrollment.actor,
                                ) {
                                    failures.push(error.to_string());
                                }
                                if let Err(error) = self.validate_required_authority(
                                    authority.as_ref(),
                                    &publisher,
                                    RequiredPublisherAction::Snapshot,
                                    now,
                                ) {
                                    failures.push(error.to_string());
                                }
                            }
                            _ => failures.push(
                                "newest attempt publisher enrollment is absent, revoked or rotated"
                                    .into(),
                            ),
                        }
                    } else {
                        failures.push(
                            "no authoritative attempt exists for this exact commit/context".into(),
                        );
                    }
                    if status != Some(RequiredAttemptStatus::Success) {
                        failures.push("newest authoritative attempt is not successful".into());
                    }
                    results.push(RequiredContextSnapshot {
                        context,
                        newest_attempt: newest,
                        status,
                        satisfied: failures.is_empty(),
                        blockers: failures,
                    });
                }
                Ok(RequiredAttemptSnapshot {
                    repository_id,
                    commit_sha: commit_sha.into(),
                    observed_at: now,
                    policy_sha256: super::ref_operations::sha256(
                        &super::ref_operations::canonical_json(&(branch, policy))?,
                    ),
                    satisfied: blockers.is_empty()
                        && !results.is_empty()
                        && results.iter().all(|context| context.satisfied),
                    contexts: results,
                    blockers,
                })
            })
    }

    // These helpers run inside the existing coordinator operation. Calling the
    // public custody readback here would reacquire a non-reentrant write guard.
    fn compare_required_custody(
        &self,
        authority: &dyn RequiredPublisherAuthority,
    ) -> Result<super::RequiredPublisherCustody> {
        let actual = self
            .runtime
            .storage
            .as_ref()
            .ok_or_else(unavailable)?
            .required_publisher_custody()?;
        if authority.custody() != &actual {
            return Err(ForgeError::WriterUnavailable(
                "publisher authority does not bind this database and storage incarnation".into(),
            ));
        }
        Ok(actual)
    }

    fn validate_required_authority(
        &self,
        authority: &dyn RequiredPublisherAuthority,
        publisher: &DurableRequiredPublisher,
        action: RequiredPublisherAction,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let before = self.compare_required_custody(authority)?;
        authority.validate(&before, publisher, action, now)?;
        let after = self.compare_required_custody(authority)?;
        if before != after {
            return Err(ForgeError::WriterUnavailable(
                "publisher custody changed during validation".into(),
            ));
        }
        Ok(())
    }

    fn required_authority(&self) -> Result<Arc<dyn RequiredPublisherAuthority>> {
        self.runtime
            .required_publisher_authority
            .read()
            .clone()
            .ok_or_else(unavailable)
    }

    pub(super) fn required_actor_context(
        &self,
        actor: &AuthenticatedActor,
        repository_id: Uuid,
        mutation: bool,
    ) -> Result<(ReviewGitRepository, ReviewActorBinding)> {
        let state = self.runtime.state.read();
        let binding = self.validate_actor_locked(&state, actor, mutation)?;
        super::mutation::require_repository_admissible(&state, repository_id, mutation)?;
        let repository = state
            .repos
            .values()
            .find(|repo| repo.id == repository_id)
            .ok_or_else(|| ForgeError::NotFound("repository".into()))?;
        require_access(
            &state,
            &binding.login,
            &repository.owner,
            &repository.name,
            repository.private,
            mutation,
        )?;
        Ok((
            ReviewGitRepository {
                id: repository.id,
                owner: repository.owner.clone(),
                name: repository.name.clone(),
            },
            binding,
        ))
    }

    fn observe_required_source(
        &self,
        repository: &ReviewGitRepository,
        scope: &RequiredPublisherScope,
    ) -> Result<()> {
        let observer = self
            .runtime
            .review_git_observer
            .read()
            .clone()
            .ok_or_else(unavailable)?;
        let target = ReviewGitTarget {
            source: repository.clone(),
            destination: repository.clone(),
            source_ref: scope.source_ref.clone(),
            destination_ref: scope.source_ref.clone(),
        };
        let observed = observer.observe(&target)?;
        if observed.source.reference != scope.source_ref
            || observed.source.commit_sha != scope.commit_sha
            || observed.source.tree_sha != scope.tree_sha
            || observed.destination != observed.source
        {
            return Err(ForgeError::Conflict(
                "actual managed source ref/tree differs from enrolled scope".into(),
            ));
        }
        Ok(())
    }
}

fn validate_attempt_enrollment<'a>(
    publisher: &'a DurableRequiredPublisher,
    attempt: &DurableRequiredAttempt,
) -> Result<&'a RequiredPublisherScope> {
    let reservation = &attempt.reservation;
    let scope = publisher.enrollment.scope(
        reservation.binding.repository_id,
        &reservation.binding.commit_sha,
        &reservation.binding.context,
    )?;
    let expected = publisher.enrollment.reservation(
        scope,
        &reservation.idempotency_key,
        reservation.expires_at,
        publisher.enrollment_sha256.clone(),
    );
    if *reservation != expected
        || attempt.reserved_at < publisher.installed_at
        || attempt.reserved_at < publisher.enrollment.issued_at
        || reservation.expires_at > publisher.enrollment.expires_at
        || reservation
            .expires_at
            .signed_duration_since(attempt.reserved_at)
            > Duration::seconds(7200)
    {
        return Err(ForgeError::Conflict(
            "durable attempt differs from enrolled binding or fixed window".into(),
        ));
    }
    Ok(scope)
}

fn require_publisher_actor(
    publisher: &DurableRequiredPublisher,
    actor: &ReviewActorBinding,
) -> Result<()> {
    if publisher.enrollment.actor != *actor {
        return Err(ForgeError::Forbidden(
            "authenticated principal/credential/epoch differs from enrolled publisher".into(),
        ));
    }
    Ok(())
}

fn require_access(
    state: &State,
    login: &str,
    owner: &str,
    name: &str,
    private: bool,
    mutation: bool,
) -> Result<()> {
    let account = state
        .accounts
        .get(login)
        .ok_or_else(|| ForgeError::Unauthenticated("account disappeared".into()))?;
    let access = if account.role == UserRole::Admin {
        Some(RepoAccessLevel::Admin)
    } else {
        state
            .repo_grants
            .get(&(login.into(), owner.into(), name.into()))
            .map(|grant| grant.access)
            .or_else(|| (!mutation && !private).then_some(RepoAccessLevel::Read))
    };
    if access.is_some_and(|level| {
        if mutation {
            level.allows_write()
        } else {
            level.allows_read()
        }
    }) {
        Ok(())
    } else {
        Err(ForgeError::Forbidden(
            "current repository access is required".into(),
        ))
    }
}

fn required<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str> {
    value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ForgeError::PreconditionRequired(format!("{field} is required")))
}

fn unavailable() -> ForgeError {
    ForgeError::WriterUnavailable("verified publisher installation, managed source or durable enrollment custody is unavailable".into())
}
