//! Release objects and signing workflow.

use crate::artifact::Artifact;
use crate::error::{Result, SignRailError};
use crate::identity::OidcJobIdentity;
use crate::json;
use crate::provenance::{ProvenanceStatement, SignedProvenance};
use crate::rollback::RollbackMetadata;
use crate::sbom::SbomDocument;
use crate::signature::Signer;

/// Release object with artifacts and assurance metadata.
#[derive(Clone, Debug)]
pub struct Release {
    /// Release identifier.
    pub id: String,
    /// Human-readable release name.
    pub name: String,
    /// Immutable semantic version or tag.
    pub version: String,
    /// Source repository.
    pub source_repository: String,
    /// Commit SHA.
    pub commit_sha: String,
    /// Tree SHA.
    pub tree_sha: String,
    /// CI IR hash.
    pub jeryu_ci_ir_hash: String,
    /// Runner class.
    pub runner_class: String,
    /// Runner rootfs/image digest.
    pub runner_rootfs_digest: String,
    /// Toolchain digest.
    pub toolchain_digest: String,
    /// Cargo.lock digest.
    pub cargo_lock_digest: String,
    /// Release artifacts.
    pub artifacts: Vec<Artifact>,
    /// SBOM document.
    pub sbom: Option<SbomDocument>,
    /// Signed provenance entries.
    pub provenance: Vec<SignedProvenance>,
    /// Rollback metadata.
    pub rollback: Option<RollbackMetadata>,
    /// OIDC job identity.
    pub oidc: OidcJobIdentity,
    /// Immutable release flag.
    pub immutable: bool,
}

impl Release {
    /// Construct a release object.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        source_repository: impl Into<String>,
        commit_sha: impl Into<String>,
        tree_sha: impl Into<String>,
        jeryu_ci_ir_hash: impl Into<String>,
        runner_class: impl Into<String>,
        runner_rootfs_digest: impl Into<String>,
        toolchain_digest: impl Into<String>,
        cargo_lock_digest: impl Into<String>,
        oidc: OidcJobIdentity,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            source_repository: source_repository.into(),
            commit_sha: commit_sha.into(),
            tree_sha: tree_sha.into(),
            jeryu_ci_ir_hash: jeryu_ci_ir_hash.into(),
            runner_class: runner_class.into(),
            runner_rootfs_digest: runner_rootfs_digest.into(),
            toolchain_digest: toolchain_digest.into(),
            cargo_lock_digest: cargo_lock_digest.into(),
            artifacts: Vec::new(),
            sbom: None,
            provenance: Vec::new(),
            rollback: None,
            oidc,
            immutable: true,
        }
    }

    /// Attach an artifact.
    pub fn add_artifact(&mut self, artifact: Artifact) {
        self.artifacts.push(artifact);
    }

    /// Attach an SBOM.
    pub fn attach_sbom(&mut self, sbom: SbomDocument) {
        self.sbom = Some(sbom);
    }

    /// Attach rollback metadata.
    pub fn attach_rollback(&mut self, rollback: RollbackMetadata) {
        self.rollback = Some(rollback);
    }

    /// Mark the release mutable or immutable.
    pub fn set_immutable(&mut self, immutable: bool) {
        self.immutable = immutable;
    }

    /// Create and sign one provenance statement per artifact.
    pub fn sign_with(&mut self, signer: &dyn Signer, created_at_epoch: u64) -> Result<()> {
        let sbom = self.sbom.as_ref().ok_or_else(|| {
            SignRailError::Policy("missing SBOM; cannot sign release provenance".to_string())
        })?;
        let sbom_digest = sbom.digest();
        self.provenance.clear();
        for artifact in &self.artifacts {
            let statement = ProvenanceStatement {
                source_repository: self.source_repository.clone(),
                commit_sha: self.commit_sha.clone(),
                tree_sha: self.tree_sha.clone(),
                jeryu_ci_ir_hash: self.jeryu_ci_ir_hash.clone(),
                runner_class: self.runner_class.clone(),
                runner_rootfs_digest: self.runner_rootfs_digest.clone(),
                toolchain_digest: self.toolchain_digest.clone(),
                cargo_lock_digest: self.cargo_lock_digest.clone(),
                artifact_digest: artifact.digest.clone(),
                sbom_digest: sbom_digest.clone(),
                signer_identity: signer.signer_id().to_string(),
                oidc_subject: self.oidc.subject.clone(),
                jankurai_release_witness: "phase8-release-witness-required".to_string(),
                created_at_epoch,
            };
            let signature = signer.sign(&statement.canonical_message())?;
            self.provenance.push(SignedProvenance {
                statement,
                signature,
            });
        }
        Ok(())
    }

    /// Render release JSON.
    pub fn to_json(&self) -> String {
        let artifacts = self
            .artifacts
            .iter()
            .map(Artifact::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let provenance = self
            .provenance
            .iter()
            .map(SignedProvenance::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let sbom = self
            .sbom
            .as_ref()
            .map(SbomDocument::to_json)
            .unwrap_or_else(|| "null".to_string());
        let rollback = self
            .rollback
            .as_ref()
            .map(RollbackMetadata::to_json)
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{{},{},{},{},{},{},{},{},{},{},{},\"artifacts\":[{}],\"sbom\":{},\"provenance\":[{}],\"rollback\":{},\"oidc\":{},{} }}",
            json::field("id", &self.id),
            json::field("name", &self.name),
            json::field("version", &self.version),
            json::field("source_repository", &self.source_repository),
            json::field("commit_sha", &self.commit_sha),
            json::field("tree_sha", &self.tree_sha),
            json::field("jeryu_ci_ir_hash", &self.jeryu_ci_ir_hash),
            json::field("runner_class", &self.runner_class),
            json::field("runner_rootfs_digest", &self.runner_rootfs_digest),
            json::field("toolchain_digest", &self.toolchain_digest),
            json::field("cargo_lock_digest", &self.cargo_lock_digest),
            artifacts,
            sbom,
            provenance,
            rollback,
            self.oidc.to_json(),
            json::bool_field("immutable", self.immutable)
        )
    }
}
