//! Cache fingerprint inputs and canonical job fingerprinting.

use crate::types::CachePolicyError;
use jeryu_ci_ir::{Job, TrustTier, deterministic_hash};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FingerprintInputs {
    pub cache_schema_version: String,
    pub tenant_id: String,
    pub repo_id_or_scope: String,
    pub rustc_version: String,
    pub cargo_version: String,
    pub toolchain_channel: String,
    pub host_triple: String,
    pub target_triple: String,
    pub profile: String,
    pub feature_set: String,
    pub rustflags: String,
    pub linker_identity: String,
    pub sysroot_digest: String,
    pub cargo_lock_subgraph_digest: String,
    pub workspace_metadata_digest: String,
    pub crate_source_digest: String,
    pub build_rs_digest: String,
    pub proc_macro_digest: String,
    pub native_deps_digest: String,
    pub env_allowlist_digest: String,
    pub runner_rootfs_digest: String,
    pub sandbox_policy_digest: String,
    pub extra: BTreeMap<String, String>,
}

impl FingerprintInputs {
    pub fn default_for_dev() -> Self {
        Self {
            cache_schema_version: "phase3.v1".to_string(),
            tenant_id: "dev".to_string(),
            repo_id_or_scope: "repo".to_string(),
            rustc_version: "unknown-rustc".to_string(),
            cargo_version: "unknown-cargo".to_string(),
            toolchain_channel: "locked".to_string(),
            host_triple: "unknown-host".to_string(),
            target_triple: "unknown-target".to_string(),
            profile: "dev".to_string(),
            feature_set: "default".to_string(),
            rustflags: "".to_string(),
            linker_identity: "default".to_string(),
            sysroot_digest: "unknown-sysroot".to_string(),
            cargo_lock_subgraph_digest: "unknown-lock".to_string(),
            workspace_metadata_digest: "unknown-workspace".to_string(),
            crate_source_digest: "unknown-source".to_string(),
            build_rs_digest: "none".to_string(),
            proc_macro_digest: "none".to_string(),
            native_deps_digest: "none".to_string(),
            env_allowlist_digest: "empty".to_string(),
            runner_rootfs_digest: "native".to_string(),
            sandbox_policy_digest: "phase3-sandbox".to_string(),
            extra: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> Result<(), CachePolicyError> {
        let required = [
            ("cache_schema_version", &self.cache_schema_version),
            ("tenant_id", &self.tenant_id),
            ("repo_id_or_scope", &self.repo_id_or_scope),
            ("rustc_version", &self.rustc_version),
            ("cargo_version", &self.cargo_version),
            ("host_triple", &self.host_triple),
            ("target_triple", &self.target_triple),
            (
                "cargo_lock_subgraph_digest",
                &self.cargo_lock_subgraph_digest,
            ),
            ("workspace_metadata_digest", &self.workspace_metadata_digest),
            ("crate_source_digest", &self.crate_source_digest),
            ("build_rs_digest", &self.build_rs_digest),
            ("proc_macro_digest", &self.proc_macro_digest),
            ("native_deps_digest", &self.native_deps_digest),
            ("env_allowlist_digest", &self.env_allowlist_digest),
            ("runner_rootfs_digest", &self.runner_rootfs_digest),
            ("sandbox_policy_digest", &self.sandbox_policy_digest),
        ];
        for (name, value) in required {
            if value.trim().is_empty() {
                return Err(CachePolicyError::MissingFingerprintInput(name.to_string()));
            }
        }
        Ok(())
    }
}

pub fn fingerprint_job(job: &Job, trust_tier: &TrustTier, inputs: &FingerprintInputs) -> String {
    let mut canonical = String::new();
    push(
        &mut canonical,
        "cache_schema_version",
        &inputs.cache_schema_version,
    );
    push(&mut canonical, "tenant_id", &inputs.tenant_id);
    push(&mut canonical, "repo_id_or_scope", &inputs.repo_id_or_scope);
    push(&mut canonical, "trust_tier", trust_tier.as_str());
    push(&mut canonical, "job_id", &job.id);
    push(&mut canonical, "runner_class", job.runner_class.as_str());
    push(&mut canonical, "rustc_version", &inputs.rustc_version);
    push(&mut canonical, "cargo_version", &inputs.cargo_version);
    push(
        &mut canonical,
        "toolchain_channel",
        &inputs.toolchain_channel,
    );
    push(&mut canonical, "host_triple", &inputs.host_triple);
    push(&mut canonical, "target_triple", &inputs.target_triple);
    push(&mut canonical, "profile", &inputs.profile);
    push(&mut canonical, "feature_set", &inputs.feature_set);
    push(&mut canonical, "rustflags", &inputs.rustflags);
    push(&mut canonical, "linker_identity", &inputs.linker_identity);
    push(&mut canonical, "sysroot_digest", &inputs.sysroot_digest);
    push(
        &mut canonical,
        "cargo_lock_subgraph_digest",
        &inputs.cargo_lock_subgraph_digest,
    );
    push(
        &mut canonical,
        "workspace_metadata_digest",
        &inputs.workspace_metadata_digest,
    );
    push(
        &mut canonical,
        "crate_source_digest",
        &inputs.crate_source_digest,
    );
    push(&mut canonical, "build_rs_digest", &inputs.build_rs_digest);
    push(
        &mut canonical,
        "proc_macro_digest",
        &inputs.proc_macro_digest,
    );
    push(
        &mut canonical,
        "native_deps_digest",
        &inputs.native_deps_digest,
    );
    push(
        &mut canonical,
        "env_allowlist_digest",
        &inputs.env_allowlist_digest,
    );
    push(
        &mut canonical,
        "runner_rootfs_digest",
        &inputs.runner_rootfs_digest,
    );
    push(
        &mut canonical,
        "sandbox_policy_digest",
        &inputs.sandbox_policy_digest,
    );
    for mount in &job.cache_mounts {
        push(
            &mut canonical,
            "mount",
            &format!("{}|{}|{}", mount.name, mount.path, mount.fingerprint),
        );
    }
    for (key, value) in &inputs.extra {
        push(&mut canonical, key, value);
    }
    deterministic_hash(&canonical)
}

fn push(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push('=');
    out.push_str(value);
    out.push('\n');
}
