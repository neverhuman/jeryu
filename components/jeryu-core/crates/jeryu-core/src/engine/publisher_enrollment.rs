//! NEXT-SLICE PROPOSAL, not integrated or executable authority.
//! Dedicated enrollment truth. Only the commissioning verifier may install a
//! row; request transports never supply an enrollment or issuer identity.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::required_attempts::{
    MAX_SAFE_INTEGER, RequiredAttemptBinding, RequiredAttemptReservation, RequiredAuthorityOrigin,
};
use super::{ReviewActorBinding, ReviewCredentialKind};
use crate::{ForgeError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredPublisherScope {
    pub repository_id: Uuid,
    pub source_ref: String,
    pub commit_sha: String,
    pub tree_sha: String,
    pub context: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredPublisherEnrollment {
    pub schema: String,
    pub publisher_id: Uuid,
    pub revision: u64,
    pub actor: ReviewActorBinding,
    pub scopes: Vec<RequiredPublisherScope>,
    pub runtime_sha256: String,
    pub authority_origin: RequiredAuthorityOrigin,
    pub authority_contract_sha256: String,
    pub evidence_contract_sha256: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub issuer: ReviewActorBinding,
    pub reviewer: ReviewActorBinding,
    pub issuer_acceptance_sha256: String,
    pub reviewer_acceptance_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredPublisherRevocation {
    pub operation_id: Uuid,
    pub actor: ReviewActorBinding,
    pub revoked_at: DateTime<Utc>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableRequiredPublisher {
    pub enrollment: RequiredPublisherEnrollment,
    pub enrollment_sha256: String,
    pub installation_operation_id: Uuid,
    pub expected_previous_sha256: Option<String>,
    pub installed_at: DateTime<Utc>,
    pub audit_id: Uuid,
    pub revocation: Option<RequiredPublisherRevocation>,
}

impl RequiredPublisherEnrollment {
    pub(super) fn validate(&self) -> Result<()> {
        if self.schema != "jeryu.required-publisher-enrollment/v1"
            || self.publisher_id.is_nil()
            || self.revision == 0
            || self.revision > MAX_SAFE_INTEGER
            || self.scopes.is_empty()
            || self.scopes.len() > 256
            || self.expires_at <= self.issued_at
            || self.actor.credential_kind != ReviewCredentialKind::PersonalAccessToken
            || !sha256(&self.authority_contract_sha256)
            || !sha256(&self.issuer_acceptance_sha256)
            || !sha256(&self.reviewer_acceptance_sha256)
        {
            return Err(malformed("invalid required publisher enrollment"));
        }
        actor_shape(&self.actor)?;
        actor_shape(&self.issuer)?;
        actor_shape(&self.reviewer)?;
        // Opaque profile identity is decisive; login aliases do not substitute
        // for independent holders. Authentication is rechecked by the Core
        // controller and detached acceptance verifier, not by this shape check.
        let ids = [
            self.actor.profile_id,
            self.issuer.profile_id,
            self.reviewer.profile_id,
        ];
        if ids[0] == ids[1] || ids[0] == ids[2] || ids[1] == ids[2] {
            return Err(malformed(
                "publisher, enrollment issuer and reviewer must differ",
            ));
        }
        let credentials = [
            self.actor.credential_id,
            self.issuer.credential_id,
            self.reviewer.credential_id,
        ];
        if credentials[0] == credentials[1]
            || credentials[0] == credentials[2]
            || credentials[1] == credentials[2]
        {
            return Err(malformed(
                "independent enrollment holders require distinct credentials",
            ));
        }
        let mut scopes = std::collections::BTreeSet::new();
        for scope in &self.scopes {
            if !direct_source_ref(&scope.source_ref)
                || !scopes.insert((scope.repository_id, &scope.context, &scope.commit_sha))
            {
                return Err(malformed("invalid or duplicate publisher source scope"));
            }
            // Reuse the persistence contract's exact full OIDs, context and
            // input hashes. No second weaker request validator is introduced.
            self.reservation(
                scope,
                "enrollment-shape-check",
                self.expires_at,
                "f".repeat(64),
            )
            .validate()?;
        }
        Ok(())
    }

    pub(super) fn scope(
        &self,
        repository_id: Uuid,
        commit_sha: &str,
        context: &str,
    ) -> Result<&RequiredPublisherScope> {
        self.scopes
            .iter()
            .find(|scope| {
                scope.repository_id == repository_id
                    && scope.commit_sha == commit_sha
                    && scope.context == context
            })
            .ok_or_else(|| {
                ForgeError::Forbidden(
                    "publisher enrollment does not cover the exact source/context".into(),
                )
            })
    }

    pub(super) fn reservation(
        &self,
        scope: &RequiredPublisherScope,
        idempotency_key: &str,
        expires_at: DateTime<Utc>,
        enrollment_sha256: String,
    ) -> RequiredAttemptReservation {
        RequiredAttemptReservation {
            binding: RequiredAttemptBinding {
                repository_id: scope.repository_id,
                commit_sha: scope.commit_sha.clone(),
                tree_sha: scope.tree_sha.clone(),
                context: scope.context.clone(),
                publisher_id: self.publisher_id,
                actor: self.actor.clone(),
                runtime_sha256: self.runtime_sha256.clone(),
                authority_origin: self.authority_origin.clone(),
                enrollment_sha256,
                evidence_contract_sha256: self.evidence_contract_sha256.clone(),
            },
            idempotency_key: idempotency_key.into(),
            expires_at,
        }
    }
}

impl DurableRequiredPublisher {
    pub(super) fn require_current(&self, now: DateTime<Utc>) -> Result<()> {
        self.enrollment.validate()?;
        if self.revocation.is_some() {
            return Err(ForgeError::Forbidden(
                "publisher enrollment is revoked".into(),
            ));
        }
        if now < self.installed_at
            || now < self.enrollment.issued_at
            || now >= self.enrollment.expires_at
        {
            return Err(ForgeError::Conflict(
                "publisher enrollment is outside its fixed window".into(),
            ));
        }
        Ok(())
    }
}

pub(super) fn actor_shape(actor: &ReviewActorBinding) -> Result<()> {
    if actor.profile_id.is_nil()
        || actor.credential_id.is_nil()
        || actor.auth_epoch > MAX_SAFE_INTEGER
        || actor.login.is_empty()
        || actor.login.len() > 256
        || actor.login.trim() != actor.login
        || actor.login.chars().any(char::is_control)
    {
        return Err(malformed("invalid enrollment principal binding"));
    }
    Ok(())
}

fn direct_source_ref(value: &str) -> bool {
    value.len() <= 1024
        && value.starts_with("refs/heads/")
        && !value.ends_with('/')
        && !value.ends_with('.')
        && !value.contains("..")
        && !value.contains("@{")
        && value
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
}

fn sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn malformed(message: &str) -> ForgeError {
    ForgeError::Validation(message.into())
}
