//! OIDC job identity for release lanes.

use crate::error::{Result, SignRailError};
use crate::json;

/// OIDC identity bound to a release job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OidcJobIdentity {
    /// Token issuer.
    pub issuer: String,
    /// Token audience.
    pub audience: String,
    /// Subject claim.
    pub subject: String,
    /// Repository claim.
    pub repository: String,
    /// Workflow reference claim.
    pub workflow_ref: String,
    /// CI job identifier.
    pub job_id: String,
    /// Runner identity.
    pub runner_id: String,
    /// Expiration time as Unix epoch seconds.
    pub expires_at_epoch: u64,
}

impl OidcJobIdentity {
    /// Construct a new OIDC job identity.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        issuer: impl Into<String>,
        audience: impl Into<String>,
        subject: impl Into<String>,
        repository: impl Into<String>,
        workflow_ref: impl Into<String>,
        job_id: impl Into<String>,
        runner_id: impl Into<String>,
        expires_at_epoch: u64,
    ) -> Self {
        Self {
            issuer: issuer.into(),
            audience: audience.into(),
            subject: subject.into(),
            repository: repository.into(),
            workflow_ref: workflow_ref.into(),
            job_id: job_id.into(),
            runner_id: runner_id.into(),
            expires_at_epoch,
        }
    }

    /// Validate issuer, audience, expiration, and required release-lane claims.
    pub fn validate(
        &self,
        now_epoch: u64,
        expected_issuer: &str,
        expected_audience: &str,
    ) -> Result<()> {
        if self.issuer != expected_issuer {
            return Err(SignRailError::Policy(format!(
                "OIDC issuer mismatch: expected {expected_issuer}, got {}",
                self.issuer
            )));
        }
        if self.audience != expected_audience {
            return Err(SignRailError::Policy(format!(
                "OIDC audience mismatch: expected {expected_audience}, got {}",
                self.audience
            )));
        }
        if self.expires_at_epoch <= now_epoch {
            return Err(SignRailError::Policy(
                "OIDC job identity expired".to_string(),
            ));
        }
        for (name, value) in [
            ("subject", &self.subject),
            ("repository", &self.repository),
            ("workflow_ref", &self.workflow_ref),
            ("job_id", &self.job_id),
            ("runner_id", &self.runner_id),
        ] {
            if value.trim().is_empty() {
                return Err(SignRailError::Policy(format!("OIDC {name} claim is empty")));
            }
        }
        Ok(())
    }

    /// Canonical JSON representation.
    pub fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{},{},{},{}}}",
            json::field("issuer", &self.issuer),
            json::field("audience", &self.audience),
            json::field("subject", &self.subject),
            json::field("repository", &self.repository),
            json::field("workflow_ref", &self.workflow_ref),
            json::field("job_id", &self.job_id),
            json::field("runner_id", &self.runner_id),
            json::number_field("expires_at_epoch", self.expires_at_epoch)
        )
    }
}
