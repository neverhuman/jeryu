//! Release witness receipts.

use crate::checksum::sha256_hex;
use crate::json;

/// Release witness generated after all Phase 8 gates pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseWitness {
    /// Release identifier.
    pub release_id: String,
    /// Policy version used for validation.
    pub policy_version: String,
    /// Artifact count.
    pub artifact_count: usize,
    /// Signature count.
    pub signature_count: usize,
    /// Signature coverage percentage.
    pub signature_coverage_percent: u8,
    /// SBOM digest.
    pub sbom_digest: String,
    /// Check time as Unix epoch seconds.
    pub checked_at_epoch: u64,
    /// Receipt digest over witness fields.
    pub receipt_digest: String,
}

impl ReleaseWitness {
    /// Construct and digest a witness.
    pub fn new(
        release_id: impl Into<String>,
        policy_version: impl Into<String>,
        artifact_count: usize,
        signature_count: usize,
        sbom_digest: impl Into<String>,
        checked_at_epoch: u64,
    ) -> Self {
        let release_id = release_id.into();
        let policy_version = policy_version.into();
        let sbom_digest = sbom_digest.into();
        let coverage = (signature_count * 100)
            .checked_div(artifact_count)
            .unwrap_or(0) as u8;
        let material = format!(
            "release_id={release_id}\npolicy_version={policy_version}\nartifact_count={artifact_count}\nsignature_count={signature_count}\ncoverage={coverage}\nsbom_digest={sbom_digest}\nchecked_at_epoch={checked_at_epoch}\n"
        );
        let receipt_digest = sha256_hex(material.as_bytes());
        Self {
            release_id,
            policy_version,
            artifact_count,
            signature_count,
            signature_coverage_percent: coverage,
            sbom_digest,
            checked_at_epoch,
            receipt_digest,
        }
    }

    /// Render witness JSON.
    pub fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{},{},{},{}}}",
            json::field("release_id", &self.release_id),
            json::field("policy_version", &self.policy_version),
            json::number_field("artifact_count", self.artifact_count as u64),
            json::number_field("signature_count", self.signature_count as u64),
            json::number_field(
                "signature_coverage_percent",
                self.signature_coverage_percent as u64
            ),
            json::field("sbom_digest", &self.sbom_digest),
            json::number_field("checked_at_epoch", self.checked_at_epoch),
            json::field("receipt_digest", &self.receipt_digest)
        )
    }
}
