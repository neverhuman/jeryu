//! SBOM generation hooks for release artifacts.

use crate::artifact::Artifact;
use crate::checksum::sha256_hex;
use crate::json;

/// SBOM component entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SbomComponent {
    /// Component name.
    pub name: String,
    /// Component version.
    pub version: String,
    /// Component kind.
    pub kind: String,
    /// Component digest.
    pub digest: String,
    /// Source path.
    pub path: String,
}

impl SbomComponent {
    /// Render component JSON.
    pub fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{}}}",
            json::field("name", &self.name),
            json::field("version", &self.version),
            json::field("kind", &self.kind),
            json::field("digest", &self.digest),
            json::field("path", &self.path)
        )
    }
}

/// Minimal release SBOM document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SbomDocument {
    /// SBOM format.
    pub format: String,
    /// SBOM spec version.
    pub spec_version: String,
    /// Deterministic serial number.
    pub serial_number: String,
    /// Artifact components.
    pub components: Vec<SbomComponent>,
    /// Generation time as Unix epoch seconds.
    pub generated_at_epoch: u64,
}

impl SbomDocument {
    /// Build an SBOM from release artifacts.
    pub fn from_artifacts(
        version: impl Into<String>,
        artifacts: &[Artifact],
        generated_at_epoch: u64,
    ) -> Self {
        let version = version.into();
        let components = artifacts
            .iter()
            .map(|artifact| SbomComponent {
                name: artifact.name.clone(),
                version: version.clone(),
                kind: "file".to_string(),
                digest: artifact.digest.clone(),
                path: artifact.path.display().to_string(),
            })
            .collect::<Vec<_>>();
        let seed = components
            .iter()
            .map(|component| component.digest.as_str())
            .collect::<Vec<_>>()
            .join("|");
        Self {
            format: "CycloneDX-compatible".to_string(),
            spec_version: "1.5-minimal".to_string(),
            serial_number: format!("urn:uuid:jitforge-{}", &sha256_hex(seed.as_bytes())[..32]),
            components,
            generated_at_epoch,
        }
    }

    /// Digest the canonical SBOM JSON.
    pub fn digest(&self) -> String {
        sha256_hex(self.to_json().as_bytes())
    }

    /// Render SBOM JSON.
    pub fn to_json(&self) -> String {
        let components = self
            .components
            .iter()
            .map(SbomComponent::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{{},{},{},{},\"components\":[{}]}}",
            json::field("format", &self.format),
            json::field("spec_version", &self.spec_version),
            json::field("serial_number", &self.serial_number),
            json::number_field("generated_at_epoch", self.generated_at_epoch),
            components
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Artifact;
    use std::fs;

    #[test]
    fn sbom_digest_is_stable() {
        let path = std::env::temp_dir().join("signrail-sbom-test.bin");
        fs::write(&path, b"artifact").unwrap_or_else(|err| panic!("write failed: {err}"));
        let artifact = Artifact::from_file("artifact", &path, "application/octet-stream")
            .unwrap_or_else(|err| panic!("artifact failed: {err}"));
        let sbom1 = SbomDocument::from_artifacts("1.0.0", std::slice::from_ref(&artifact), 1);
        let sbom2 = SbomDocument::from_artifacts("1.0.0", &[artifact], 1);
        assert_eq!(sbom1.digest(), sbom2.digest());
        let _ = fs::remove_file(path);
    }
}
