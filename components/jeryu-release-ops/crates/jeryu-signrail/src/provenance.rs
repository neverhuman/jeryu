//! Provenance statements and signed provenance bundles.

use crate::json;
use crate::signature::Signature;

/// Required provenance marker for SignRail release witnesses.
pub const RELEASE_WITNESS_MARKER: &str = "phase8-release-witness-required";

/// Provenance statement required for every release artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvenanceStatement {
    /// Source repository URL or canonical slug.
    pub source_repository: String,
    /// Source commit SHA.
    pub commit_sha: String,
    /// Source tree SHA.
    pub tree_sha: String,
    /// CI IR hash.
    pub jeryu_ci_ir_hash: String,
    /// Runner class used to build the artifact.
    pub runner_class: String,
    /// Runner image/rootfs digest.
    pub runner_rootfs_digest: String,
    /// Toolchain digest.
    pub toolchain_digest: String,
    /// Cargo.lock digest.
    pub cargo_lock_digest: String,
    /// Artifact digest.
    pub artifact_digest: String,
    /// SBOM digest.
    pub sbom_digest: String,
    /// Signer identity.
    pub signer_identity: String,
    /// OIDC subject.
    pub oidc_subject: String,
    /// Jankurai release witness marker.
    pub jankurai_release_witness: String,
    /// Creation time as Unix epoch seconds.
    pub created_at_epoch: u64,
}

impl ProvenanceStatement {
    /// Canonical bytes for signing and verification.
    pub fn canonical_message(&self) -> Vec<u8> {
        format!(
            concat!(
                "source_repository={}\n",
                "commit_sha={}\n",
                "tree_sha={}\n",
                "jeryu_ci_ir_hash={}\n",
                "runner_class={}\n",
                "runner_rootfs_digest={}\n",
                "toolchain_digest={}\n",
                "cargo_lock_digest={}\n",
                "artifact_digest={}\n",
                "sbom_digest={}\n",
                "signer_identity={}\n",
                "oidc_subject={}\n",
                "jankurai_release_witness={}\n",
                "created_at_epoch={}\n"
            ),
            self.source_repository,
            self.commit_sha,
            self.tree_sha,
            self.jeryu_ci_ir_hash,
            self.runner_class,
            self.runner_rootfs_digest,
            self.toolchain_digest,
            self.cargo_lock_digest,
            self.artifact_digest,
            self.sbom_digest,
            self.signer_identity,
            self.oidc_subject,
            self.jankurai_release_witness,
            self.created_at_epoch
        )
        .into_bytes()
    }

    /// Render statement JSON.
    pub fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{},{},{},{},{},{},{},{},{},{}}}",
            json::field("source_repository", &self.source_repository),
            json::field("commit_sha", &self.commit_sha),
            json::field("tree_sha", &self.tree_sha),
            json::field("jeryu_ci_ir_hash", &self.jeryu_ci_ir_hash),
            json::field("runner_class", &self.runner_class),
            json::field("runner_rootfs_digest", &self.runner_rootfs_digest),
            json::field("toolchain_digest", &self.toolchain_digest),
            json::field("cargo_lock_digest", &self.cargo_lock_digest),
            json::field("artifact_digest", &self.artifact_digest),
            json::field("sbom_digest", &self.sbom_digest),
            json::field("signer_identity", &self.signer_identity),
            json::field("oidc_subject", &self.oidc_subject),
            json::field("jankurai_release_witness", &self.jankurai_release_witness),
            json::number_field("created_at_epoch", self.created_at_epoch)
        )
    }
}

/// Signed provenance bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedProvenance {
    /// Provenance statement.
    pub statement: ProvenanceStatement,
    /// Signature over canonical statement bytes.
    pub signature: Signature,
}

impl SignedProvenance {
    /// Render signed provenance JSON.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"statement\":{},\"signature\":{}}}",
            self.statement.to_json(),
            self.signature.to_json()
        )
    }
}
