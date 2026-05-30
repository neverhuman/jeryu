use serde::{Deserialize, Serialize};

use crate::cas::ContentAddressedStore;
use crate::digest::Digest;
use crate::error::{CrateVaultError, Result};
use crate::key::CacheKey;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FalseHit {
    pub expected: String,
    pub actual: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VerificationReport {
    pub key_digest: Digest,
    pub object_digest: Digest,
    pub verified: bool,
    pub false_hit: Option<FalseHit>,
}

pub fn verify_cache_hit(
    store: &ContentAddressedStore,
    key: &CacheKey,
    object_digest: &Digest,
) -> Result<VerificationReport> {
    key.verify()?;
    let bytes = store.get_bytes(object_digest)?;
    let actual = Digest::from_bytes(&bytes);
    if &actual != object_digest {
        return Err(CrateVaultError::FalseHit(format!(
            "object digest mismatch expected {object_digest} got {actual}"
        )));
    }
    Ok(VerificationReport {
        key_digest: key.digest.clone(),
        object_digest: object_digest.clone(),
        verified: true,
        false_hit: None,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::key::CacheKeyMaterial;
    use crate::tier::TrustTier;

    fn d(label: &str) -> Digest {
        Digest::from_bytes(label.as_bytes())
    }

    fn key() -> CacheKey {
        CacheKeyMaterial {
            cache_schema_version: 1,
            tenant_id: "tenant".into(),
            repo_id_or_explicit_shared_scope: "repo".into(),
            trust_tier: TrustTier::T1ProtectedInternal,
            rustc_version: "rustc".into(),
            cargo_version: "cargo".into(),
            toolchain_channel: "stable".into(),
            host_triple: "host".into(),
            target_triple: "target".into(),
            profile: "release".into(),
            feature_set: vec![],
            rustflags: vec![],
            linker_identity: "linker".into(),
            sysroot_digest: d("sysroot"),
            cargo_lock_subgraph_digest: d("lock"),
            cargo_toml_digest: d("toml"),
            workspace_metadata_digest: d("workspace"),
            crate_source_digest: d("source"),
            build_rs_digest: d("build"),
            build_rs_declared_inputs_digest: d("inputs"),
            proc_macro_digest: d("proc"),
            native_deps_digest: d("native"),
            env_allowlist_digest: d("env"),
            runner_rootfs_digest: d("rootfs"),
            sandbox_policy_digest: d("sandbox"),
        }
        .derive_key()
        .unwrap()
    }

    #[test]
    fn verifies_hit() {
        let tmp = tempdir().unwrap();
        let store = ContentAddressedStore::open(tmp.path()).unwrap();
        let object = store.put_bytes(b"artifact").unwrap();
        let report = verify_cache_hit(&store, &key(), &object.digest).unwrap();
        assert!(report.verified);
    }
}
