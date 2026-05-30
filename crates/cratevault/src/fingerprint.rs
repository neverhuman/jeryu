//! Cargo-aware cache fingerprint construction.

use crate::digest::{digest_bytes, Digest};
use crate::ids::{RepoId, TenantId};
use crate::policy::TrustTier;

/// Complete cache fingerprint inputs from the Phase 6 law.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FingerprintInput {
    /// Cache schema version.
    pub cache_schema_version: String,
    /// Tenant identity.
    pub tenant_id: TenantId,
    /// Repo or explicit shared scope identity.
    pub repo_id_or_explicit_shared_scope: String,
    /// Trust tier.
    pub trust_tier: TrustTier,
    /// rustc version.
    pub rustc_version: String,
    /// Cargo version.
    pub cargo_version: String,
    /// Toolchain channel.
    pub toolchain_channel: String,
    /// Host triple.
    pub host_triple: String,
    /// Target triple.
    pub target_triple: String,
    /// Cargo profile.
    pub profile: String,
    /// Feature set.
    pub feature_set: Vec<String>,
    /// RUSTFLAGS digest/value.
    pub rustflags: String,
    /// Linker identity digest/value.
    pub linker_identity: String,
    /// Sysroot digest.
    pub sysroot_digest: String,
    /// Cargo.lock affected subgraph digest.
    pub cargo_lock_subgraph_digest: String,
    /// Cargo.toml digest.
    pub cargo_toml_digest: String,
    /// Workspace metadata digest.
    pub workspace_metadata_digest: String,
    /// Crate source digest.
    pub crate_source_digest: String,
    /// build.rs digest.
    pub build_rs_digest: String,
    /// Declared build.rs input digest.
    pub build_rs_declared_inputs_digest: String,
    /// Proc macro digest.
    pub proc_macro_digest: String,
    /// Native dependencies digest.
    pub native_deps_digest: String,
    /// Environment allowlist digest.
    pub env_allowlist_digest: String,
    /// Runner rootfs digest.
    pub runner_rootfs_digest: String,
    /// Sandbox policy digest.
    pub sandbox_policy_digest: String,
}

impl FingerprintInput {
    /// Creates a conservative test/default fingerprint for one repo.
    pub fn for_repo(tenant_id: TenantId, repo_id: RepoId, trust_tier: TrustTier) -> Self {
        Self {
            cache_schema_version: "phase6-v1".to_string(),
            tenant_id,
            repo_id_or_explicit_shared_scope: repo_id.as_str().to_string(),
            trust_tier,
            rustc_version: "rustc-stable".to_string(),
            cargo_version: "cargo-stable".to_string(),
            toolchain_channel: "stable".to_string(),
            host_triple: "x86_64-unknown-linux-gnu".to_string(),
            target_triple: "x86_64-unknown-linux-gnu".to_string(),
            profile: "dev".to_string(),
            feature_set: Vec::new(),
            rustflags: "none".to_string(),
            linker_identity: "system-default".to_string(),
            sysroot_digest: "sysroot".to_string(),
            cargo_lock_subgraph_digest: "lock".to_string(),
            cargo_toml_digest: "manifest".to_string(),
            workspace_metadata_digest: "workspace".to_string(),
            crate_source_digest: "source".to_string(),
            build_rs_digest: "no-build-rs".to_string(),
            build_rs_declared_inputs_digest: "no-build-rs-inputs".to_string(),
            proc_macro_digest: "no-proc-macro".to_string(),
            native_deps_digest: "no-native-deps".to_string(),
            env_allowlist_digest: "empty-env".to_string(),
            runner_rootfs_digest: "rootfs".to_string(),
            sandbox_policy_digest: "sandbox".to_string(),
        }
    }

    /// Sets `build.rs` digests.
    pub fn with_build_rs(mut self, digest: impl Into<String>, inputs: impl Into<String>) -> Self {
        self.build_rs_digest = digest.into();
        self.build_rs_declared_inputs_digest = inputs.into();
        self
    }

    /// Sets proc-macro digest.
    pub fn with_proc_macro(mut self, digest: impl Into<String>) -> Self {
        self.proc_macro_digest = digest.into();
        self
    }

    /// Sets crate source digest.
    pub fn with_crate_source(mut self, digest: impl Into<String>) -> Self {
        self.crate_source_digest = digest.into();
        self
    }

    /// Sets feature set and sorts it for canonical stability.
    pub fn with_features(mut self, features: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.feature_set = features.into_iter().map(Into::into).collect();
        self.feature_set.sort();
        self.feature_set.dedup();
        self
    }

    /// Canonical line representation for hashing and explain output.
    pub fn canonical_lines(&self) -> String {
        let mut features = self.feature_set.clone();
        features.sort();
        features.dedup();
        let fields: Vec<(&str, String)> = vec![
            ("cache_schema_version", self.cache_schema_version.clone()),
            ("tenant_id", self.tenant_id.as_str().to_string()),
            (
                "repo_id_or_explicit_shared_scope",
                self.repo_id_or_explicit_shared_scope.clone(),
            ),
            ("trust_tier", self.trust_tier.to_string()),
            ("rustc_version", self.rustc_version.clone()),
            ("cargo_version", self.cargo_version.clone()),
            ("toolchain_channel", self.toolchain_channel.clone()),
            ("host_triple", self.host_triple.clone()),
            ("target_triple", self.target_triple.clone()),
            ("profile", self.profile.clone()),
            ("feature_set", features.join(",")),
            ("rustflags", self.rustflags.clone()),
            ("linker_identity", self.linker_identity.clone()),
            ("sysroot_digest", self.sysroot_digest.clone()),
            (
                "Cargo.lock_subgraph_digest",
                self.cargo_lock_subgraph_digest.clone(),
            ),
            ("Cargo.toml_digest", self.cargo_toml_digest.clone()),
            (
                "workspace_metadata_digest",
                self.workspace_metadata_digest.clone(),
            ),
            ("crate_source_digest", self.crate_source_digest.clone()),
            ("build_rs_digest", self.build_rs_digest.clone()),
            (
                "build_rs_declared_inputs_digest",
                self.build_rs_declared_inputs_digest.clone(),
            ),
            ("proc_macro_digest", self.proc_macro_digest.clone()),
            ("native_deps_digest", self.native_deps_digest.clone()),
            ("env_allowlist_digest", self.env_allowlist_digest.clone()),
            ("runner_rootfs_digest", self.runner_rootfs_digest.clone()),
            ("sandbox_policy_digest", self.sandbox_policy_digest.clone()),
        ];

        let mut out = String::new();
        for (name, value) in fields {
            out.push_str(name);
            out.push('=');
            out.push_str(&value);
            out.push('\n');
        }
        out
    }

    /// Computes the cache fingerprint digest.
    pub fn digest(&self) -> Digest {
        digest_bytes(self.canonical_lines().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> FingerprintInput {
        FingerprintInput::for_repo(
            TenantId::new("tenant").expect("valid tenant"),
            RepoId::new("repo").expect("valid repo"),
            TrustTier::InternalBranch,
        )
    }

    #[test]
    fn fingerprint_changes_on_build_rs() {
        assert_ne!(
            base().with_build_rs("a", "inputs").digest(),
            base().with_build_rs("b", "inputs").digest()
        );
    }

    #[test]
    fn fingerprint_changes_on_proc_macro() {
        assert_ne!(
            base().with_proc_macro("macro-a").digest(),
            base().with_proc_macro("macro-b").digest()
        );
    }

    #[test]
    fn fingerprint_sorts_features() {
        assert_eq!(
            base().with_features(["b", "a"]).digest(),
            base().with_features(["a", "b"]).digest()
        );
    }
}
