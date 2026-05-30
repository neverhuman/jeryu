use serde::{Deserialize, Serialize};

use crate::digest::Digest;
use crate::error::{CrateVaultError, Result};
use crate::tier::TrustTier;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CacheKeyMaterial {
    pub cache_schema_version: u32,
    pub tenant_id: String,
    pub repo_id_or_explicit_shared_scope: String,
    pub trust_tier: TrustTier,
    pub rustc_version: String,
    pub cargo_version: String,
    pub toolchain_channel: String,
    pub host_triple: String,
    pub target_triple: String,
    pub profile: String,
    pub feature_set: Vec<String>,
    pub rustflags: Vec<String>,
    pub linker_identity: String,
    pub sysroot_digest: Digest,
    pub cargo_lock_subgraph_digest: Digest,
    pub cargo_toml_digest: Digest,
    pub workspace_metadata_digest: Digest,
    pub crate_source_digest: Digest,
    pub build_rs_digest: Digest,
    pub build_rs_declared_inputs_digest: Digest,
    pub proc_macro_digest: Digest,
    pub native_deps_digest: Digest,
    pub env_allowlist_digest: Digest,
    pub runner_rootfs_digest: Digest,
    pub sandbox_policy_digest: Digest,
}

impl CacheKeyMaterial {
    pub fn validate(&self) -> Result<()> {
        if self.cache_schema_version == 0 {
            return Err(CrateVaultError::InvalidKeyMaterial(
                "schema version must be non-zero".into(),
            ));
        }
        for (name, value) in [
            ("tenant_id", &self.tenant_id),
            (
                "repo_id_or_explicit_shared_scope",
                &self.repo_id_or_explicit_shared_scope,
            ),
            ("rustc_version", &self.rustc_version),
            ("cargo_version", &self.cargo_version),
            ("host_triple", &self.host_triple),
            ("target_triple", &self.target_triple),
            ("profile", &self.profile),
        ] {
            if value.trim().is_empty() {
                return Err(CrateVaultError::InvalidKeyMaterial(format!(
                    "{name} is required"
                )));
            }
        }
        Ok(())
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut cloned = self.clone();
        cloned.feature_set.sort();
        cloned.feature_set.dedup();
        cloned.rustflags.sort();
        cloned.rustflags.dedup();
        serde_json::to_vec(&cloned).map_err(Into::into)
    }

    pub fn derive_key(&self) -> Result<CacheKey> {
        Ok(CacheKey {
            digest: Digest::from_bytes(&self.canonical_json()?),
            material: self.clone(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CacheKey {
    pub digest: Digest,
    pub material: CacheKeyMaterial,
}

impl CacheKey {
    pub fn verify(&self) -> Result<()> {
        let expected = self.material.derive_key()?.digest;
        if expected != self.digest {
            return Err(CrateVaultError::FalseHit(format!(
                "cache key digest mismatch expected {expected} got {}",
                self.digest
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(label: &str) -> Digest {
        Digest::from_bytes(label.as_bytes())
    }

    fn material() -> CacheKeyMaterial {
        CacheKeyMaterial {
            cache_schema_version: 1,
            tenant_id: "tenant".into(),
            repo_id_or_explicit_shared_scope: "repo".into(),
            trust_tier: TrustTier::T2InternalBranch,
            rustc_version: "rustc 1.78.0".into(),
            cargo_version: "cargo 1.78.0".into(),
            toolchain_channel: "stable".into(),
            host_triple: "x86_64-unknown-linux-gnu".into(),
            target_triple: "x86_64-unknown-linux-gnu".into(),
            profile: "dev".into(),
            feature_set: vec!["serde".into(), "default".into()],
            rustflags: vec!["-Cdebuginfo=1".into()],
            linker_identity: "lld".into(),
            sysroot_digest: d("sysroot"),
            cargo_lock_subgraph_digest: d("lock"),
            cargo_toml_digest: d("toml"),
            workspace_metadata_digest: d("workspace"),
            crate_source_digest: d("source"),
            build_rs_digest: d("build-rs"),
            build_rs_declared_inputs_digest: d("build-inputs"),
            proc_macro_digest: d("proc-macro"),
            native_deps_digest: d("native"),
            env_allowlist_digest: d("env"),
            runner_rootfs_digest: d("rootfs"),
            sandbox_policy_digest: d("sandbox"),
        }
    }

    #[test]
    fn cache_key_is_deterministic_under_feature_ordering() {
        let a = material().derive_key().unwrap();
        let mut m = material();
        m.feature_set.reverse();
        let b = m.derive_key().unwrap();
        assert_eq!(a.digest, b.digest);
    }

    #[test]
    fn cache_key_changes_when_build_rs_changes() {
        let a = material().derive_key().unwrap();
        let mut m = material();
        m.build_rs_digest = d("changed build.rs");
        let b = m.derive_key().unwrap();
        assert_ne!(a.digest, b.digest);
    }

    #[test]
    fn cache_key_changes_when_proc_macro_changes() {
        let a = material().derive_key().unwrap();
        let mut m = material();
        m.proc_macro_digest = d("changed proc macro");
        let b = m.derive_key().unwrap();
        assert_ne!(a.digest, b.digest);
    }
}
