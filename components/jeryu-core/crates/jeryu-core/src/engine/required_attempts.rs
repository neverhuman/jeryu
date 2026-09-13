//! Required-attempt persistence contracts. Enrollment and sealed execution must
//! authenticate these inputs before the private storage API can be activated.
//! Existing legacy check rows and split-phase merge APIs gain no authority here.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::ReviewActorBinding;
use crate::{ForgeError, Result};

pub(super) const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequiredAuthorityOrigin {
    Ordinary,
    ReviewedStagingCandidate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredAttemptBinding {
    pub repository_id: Uuid,
    pub commit_sha: String,
    pub tree_sha: String,
    pub context: String,
    pub publisher_id: Uuid,
    pub actor: ReviewActorBinding,
    pub runtime_sha256: String,
    pub authority_origin: RequiredAuthorityOrigin,
    pub enrollment_sha256: String,
    pub evidence_contract_sha256: String,
}

/// Private Core input, never an authenticated publisher capability. The future
/// enrollment adapter must build it under the same guards as reservation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredAttemptReservation {
    pub binding: RequiredAttemptBinding,
    pub idempotency_key: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequiredAttemptConclusion {
    Success,
    Failure,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceivedRequiredArtifact {
    pub name: String,
    pub sha256: String,
    pub size_bytes: u64,
}

/// Receiving bytes are hashed and durably stored by Core. This deliberately
/// accepts no caller-supplied digest or filesystem path as receiving evidence.
pub(super) struct RequiredArtifactBytes {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredAttemptCompletion {
    pub conclusion: RequiredAttemptConclusion,
    pub artifacts: Vec<ReceivedRequiredArtifact>,
    pub completed_at: DateTime<Utc>,
    pub audit_id: Uuid,
    pub event_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableRequiredAttempt {
    pub id: Uuid,
    pub ordinal: u64,
    pub reservation: RequiredAttemptReservation,
    pub reservation_sha256: String,
    pub reserved_at: DateTime<Utc>,
    pub audit_id: Uuid,
    pub completion: Option<RequiredAttemptCompletion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequiredAttemptStatus {
    Pending,
    Success,
    Failure,
    Cancelled,
    Expired,
}

impl DurableRequiredAttempt {
    /// Apply only to the newest reservation for the exact repository, commit
    /// and context. A completed receipt retains its original meaning at expiry.
    pub fn status_at(&self, now: DateTime<Utc>) -> RequiredAttemptStatus {
        match self.completion.as_ref().map(|result| result.conclusion) {
            Some(RequiredAttemptConclusion::Success) => RequiredAttemptStatus::Success,
            Some(RequiredAttemptConclusion::Failure) => RequiredAttemptStatus::Failure,
            Some(RequiredAttemptConclusion::Cancelled) => RequiredAttemptStatus::Cancelled,
            None if now >= self.reservation.expires_at => RequiredAttemptStatus::Expired,
            None => RequiredAttemptStatus::Pending,
        }
    }
}

impl RequiredAttemptReservation {
    pub(super) fn validate(&self) -> Result<()> {
        let binding = &self.binding;
        if binding.repository_id.is_nil()
            || binding.publisher_id.is_nil()
            || binding.actor.profile_id.is_nil()
            || binding.actor.credential_id.is_nil()
            || binding.actor.login.trim().is_empty()
            || binding.actor.auth_epoch > MAX_SAFE_INTEGER
            || !full_oid(&binding.commit_sha)
            || !full_oid(&binding.tree_sha)
            || binding.commit_sha.len() != binding.tree_sha.len()
            || !safe_text(&binding.context, 256)
            || !safe_text(&self.idempotency_key, 256)
            || !full_sha256(&binding.runtime_sha256)
            || !full_sha256(&binding.enrollment_sha256)
            || !full_sha256(&binding.evidence_contract_sha256)
        {
            return Err(invalid("incomplete or malformed required-attempt binding"));
        }
        Ok(())
    }
}

pub(super) fn receive_artifacts(
    artifacts: &[RequiredArtifactBytes],
) -> Result<Vec<ReceivedRequiredArtifact>> {
    if artifacts.is_empty() || artifacts.len() > 64 {
        return Err(invalid(
            "required completion needs between 1 and 64 artifacts",
        ));
    }
    let mut names = BTreeSet::new();
    let mut total = 0usize;
    let mut received = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        if artifact.name.is_empty()
            || artifact.name.len() > 128
            || !artifact
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
            || matches!(artifact.name.as_str(), "." | "..")
            || !names.insert(&artifact.name)
            || artifact.bytes.is_empty()
        {
            return Err(invalid("invalid or duplicate required artifact name/bytes"));
        }
        total = total
            .checked_add(artifact.bytes.len())
            .filter(|size| *size <= 16 * 1024 * 1024)
            .ok_or_else(|| invalid("required evidence exceeds admitted 16 MiB limit"))?;
        received.push(ReceivedRequiredArtifact {
            name: artifact.name.clone(),
            sha256: hex::encode(Sha256::digest(&artifact.bytes)),
            size_bytes: artifact.bytes.len() as u64,
        });
    }
    received.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(received)
}

pub(super) fn safe_text(value: &str, limit: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= limit
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

pub(super) fn full_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && !value.bytes().all(|byte| byte == b'0')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub(super) fn full_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn invalid(message: &str) -> ForgeError {
    ForgeError::Validation(message.into())
}
