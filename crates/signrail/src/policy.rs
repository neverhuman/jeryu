//! Fail-closed Phase 8 release policy.

use crate::error::{Result, SignRailError};
use crate::release::Release;
use crate::signature::Signer;
use crate::witness::ReleaseWitness;
use std::collections::HashSet;

/// Release validation policy.
#[derive(Clone, Debug)]
pub struct ReleasePolicy {
    /// Policy version.
    pub policy_version: String,
    /// Expected source repository.
    pub expected_source_repository: String,
    /// Expected OIDC issuer.
    pub expected_oidc_issuer: String,
    /// Expected OIDC audience.
    pub expected_oidc_audience: String,
    /// Allowed runner classes.
    pub allowed_runner_classes: Vec<String>,
    /// Current epoch time.
    pub now_epoch: u64,
    /// Require rollback metadata.
    pub require_rollback: bool,
}

impl ReleasePolicy {
    /// Construct a strict release policy.
    pub fn strict(
        expected_source_repository: impl Into<String>,
        expected_oidc_issuer: impl Into<String>,
        expected_oidc_audience: impl Into<String>,
        now_epoch: u64,
    ) -> Self {
        Self {
            policy_version: "phase8.1".to_string(),
            expected_source_repository: expected_source_repository.into(),
            expected_oidc_issuer: expected_oidc_issuer.into(),
            expected_oidc_audience: expected_oidc_audience.into(),
            allowed_runner_classes: vec!["release-hermetic".to_string()],
            now_epoch,
            require_rollback: true,
        }
    }
}

/// Validate a release and emit a witness if every Phase 8 gate passes.
pub fn validate_release(
    release: &Release,
    policy: &ReleasePolicy,
    signer: &dyn Signer,
) -> Result<ReleaseWitness> {
    if release.artifacts.is_empty() {
        return Err(SignRailError::Policy(
            "release has no artifacts".to_string(),
        ));
    }
    if !release.immutable {
        return Err(SignRailError::Policy(
            "mutable release assets are blocked".to_string(),
        ));
    }
    if release.version.trim().is_empty() || release.version == "latest" {
        return Err(SignRailError::Policy(
            "mutable latest-only asset blocked".to_string(),
        ));
    }
    if release.source_repository != policy.expected_source_repository {
        return Err(SignRailError::Policy(format!(
            "source repository mismatch: expected {}, got {}",
            policy.expected_source_repository, release.source_repository
        )));
    }
    if !policy
        .allowed_runner_classes
        .iter()
        .any(|class| class == &release.runner_class)
    {
        return Err(SignRailError::Policy(format!(
            "runner class {} is not allowed for release",
            release.runner_class
        )));
    }
    release.oidc.validate(
        policy.now_epoch,
        &policy.expected_oidc_issuer,
        &policy.expected_oidc_audience,
    )?;

    for artifact in &release.artifacts {
        if artifact.is_mutable_latest() {
            return Err(SignRailError::Policy(
                "mutable latest-only asset blocked".to_string(),
            ));
        }
        if artifact.digest.trim().is_empty() {
            return Err(SignRailError::Policy(format!(
                "artifact {} is missing digest",
                artifact.name
            )));
        }
    }

    let sbom = release
        .sbom
        .as_ref()
        .ok_or_else(|| SignRailError::Policy("missing SBOM blocked".to_string()))?;
    let sbom_digest = sbom.digest();

    if policy.require_rollback {
        release
            .rollback
            .as_ref()
            .ok_or_else(|| SignRailError::Policy("missing rollback metadata".to_string()))?
            .validate()?;
    }

    if release.provenance.len() != release.artifacts.len() {
        return Err(SignRailError::Policy(format!(
            "signature coverage is not 100%: {} provenance entries for {} artifacts",
            release.provenance.len(),
            release.artifacts.len()
        )));
    }

    let artifact_digests = release
        .artifacts
        .iter()
        .map(|artifact| artifact.digest.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();

    for signed in &release.provenance {
        let statement = &signed.statement;
        if statement.source_repository != release.source_repository {
            return Err(SignRailError::Policy(
                "provenance source repository mismatch".to_string(),
            ));
        }
        if statement.commit_sha != release.commit_sha {
            return Err(SignRailError::Policy(
                "wrong source SHA blocked".to_string(),
            ));
        }
        if statement.tree_sha != release.tree_sha {
            return Err(SignRailError::Policy(
                "wrong source tree SHA blocked".to_string(),
            ));
        }
        if statement.ci_ir_hash != release.ci_ir_hash {
            return Err(SignRailError::Policy("CI IR hash mismatch".to_string()));
        }
        if statement.runner_class != release.runner_class {
            return Err(SignRailError::Policy("runner class mismatch".to_string()));
        }
        if statement.runner_rootfs_digest != release.runner_rootfs_digest {
            return Err(SignRailError::Policy(
                "runner rootfs digest mismatch".to_string(),
            ));
        }
        if statement.toolchain_digest != release.toolchain_digest {
            return Err(SignRailError::Policy(
                "toolchain digest mismatch".to_string(),
            ));
        }
        if statement.cargo_lock_digest != release.cargo_lock_digest {
            return Err(SignRailError::Policy(
                "Cargo.lock digest mismatch".to_string(),
            ));
        }
        if statement.sbom_digest != sbom_digest {
            return Err(SignRailError::Policy("SBOM digest mismatch".to_string()));
        }
        if statement.signer_identity != signer.signer_id() {
            return Err(SignRailError::Policy(
                "signer identity mismatch".to_string(),
            ));
        }
        if statement.oidc_subject != release.oidc.subject {
            return Err(SignRailError::Policy("OIDC subject mismatch".to_string()));
        }
        if !artifact_digests.contains(statement.artifact_digest.as_str()) {
            return Err(SignRailError::Policy(
                "provenance references unknown artifact digest".to_string(),
            ));
        }
        if !seen.insert(statement.artifact_digest.clone()) {
            return Err(SignRailError::Policy(
                "duplicate provenance artifact digest".to_string(),
            ));
        }
        signer.verify(&statement.canonical_message(), &signed.signature)?;
    }

    Ok(ReleaseWitness::new(
        release.id.clone(),
        policy.policy_version.clone(),
        release.artifacts.len(),
        release.provenance.len(),
        sbom_digest,
        policy.now_epoch,
    ))
}
