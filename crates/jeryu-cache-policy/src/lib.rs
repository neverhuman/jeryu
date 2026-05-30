use jeryu_ci_ir::{Job, TrustTier, deterministic_hash};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CacheScope {
    JobTmpfs,
    RunnerLocalProject,
    RunnerLocalRegistry,
    RepoCompiledCas,
    TenantSourceCas,
    ExplicitSharedCompiledCas(String),
    ReleaseHermeticVendorSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheDecision {
    pub scope: CacheScope,
    pub read_allowed: bool,
    pub write_allowed: bool,
    pub promote_after_green: bool,
    pub quarantine: bool,
    pub mutable_compiled_cache_allowed: bool,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachePlan {
    pub job_id: String,
    pub decisions: Vec<CacheDecision>,
    pub fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CachePolicyError {
    CrossProjectCompiledDenied,
    ReleaseMutableCacheDenied,
    MissingFingerprintInput(String),
}

impl fmt::Display for CachePolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CrossProjectCompiledDenied => {
                f.write_str("cross-project compiled cache is denied by default")
            }
            Self::ReleaseMutableCacheDenied => {
                f.write_str("release lane cannot consume mutable compiled cache")
            }
            Self::MissingFingerprintInput(input) => {
                write!(f, "missing cache fingerprint input: {input}")
            }
        }
    }
}

impl std::error::Error for CachePolicyError {}

pub fn plan_cache_for_job(job: &Job, trust_tier: &TrustTier) -> CachePlan {
    let mut decisions = Vec::new();
    decisions.push(CacheDecision {
        scope: CacheScope::JobTmpfs,
        read_allowed: true,
        write_allowed: true,
        promote_after_green: false,
        quarantine: false,
        mutable_compiled_cache_allowed: true,
        reason: "job-local tmpfs is ephemeral".to_string(),
    });
    decisions.push(CacheDecision {
        scope: CacheScope::RunnerLocalRegistry,
        read_allowed: true,
        write_allowed: matches!(
            trust_tier,
            TrustTier::ProtectedInternal | TrustTier::InternalBranch
        ),
        promote_after_green: false,
        quarantine: matches!(trust_tier, TrustTier::AgentAuthored),
        mutable_compiled_cache_allowed: false,
        reason: "registry/source cache may be shared, compiled outputs are not implied".to_string(),
    });

    match trust_tier {
        TrustTier::ReleaseHermetic => decisions.push(CacheDecision {
            scope: CacheScope::ReleaseHermeticVendorSnapshot,
            read_allowed: true,
            write_allowed: false,
            promote_after_green: false,
            quarantine: false,
            mutable_compiled_cache_allowed: false,
            reason: "release lanes use locked hermetic inputs only".to_string(),
        }),
        TrustTier::ProtectedInternal | TrustTier::InternalBranch => decisions.push(CacheDecision {
            scope: CacheScope::RepoCompiledCas,
            read_allowed: true,
            write_allowed: true,
            promote_after_green: true,
            quarantine: false,
            mutable_compiled_cache_allowed: true,
            reason: "same repo and trusted tier may read/write project compiled cache after green"
                .to_string(),
        }),
        TrustTier::AgentAuthored => decisions.push(CacheDecision {
            scope: CacheScope::RepoCompiledCas,
            read_allowed: true,
            write_allowed: true,
            promote_after_green: false,
            quarantine: true,
            mutable_compiled_cache_allowed: true,
            reason: "agent writes go to quarantine until proof receipts pass".to_string(),
        }),
        TrustTier::ForkPr | TrustTier::PublicUntrusted => decisions.push(CacheDecision {
            scope: CacheScope::TenantSourceCas,
            read_allowed: true,
            write_allowed: false,
            promote_after_green: false,
            quarantine: false,
            mutable_compiled_cache_allowed: false,
            reason: "untrusted jobs may read source cache but cannot write trusted compiled cache"
                .to_string(),
        }),
    }

    CachePlan {
        job_id: job.id.clone(),
        fingerprint: fingerprint_job(job, trust_tier, &FingerprintInputs::default_for_dev()),
        decisions,
    }
}

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

pub fn assert_cross_project_compiled_allowed(allow: bool) -> Result<(), CachePolicyError> {
    if allow {
        Ok(())
    } else {
        Err(CachePolicyError::CrossProjectCompiledDenied)
    }
}

pub fn assert_release_cache_safe(plan: &CachePlan) -> Result<(), CachePolicyError> {
    for decision in &plan.decisions {
        let compiled_cache = matches!(
            decision.scope,
            CacheScope::RepoCompiledCas | CacheScope::ExplicitSharedCompiledCas(_)
        );
        if compiled_cache && decision.mutable_compiled_cache_allowed && decision.read_allowed {
            return Err(CachePolicyError::ReleaseMutableCacheDenied);
        }
    }
    Ok(())
}

fn push(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push('=');
    out.push_str(value);
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_ci_ir::{CacheMode, CacheMount, Job, RunnerClass, Step};

    fn job() -> Job {
        let mut job = Job::new("test", "test", RunnerClass::NativeRustClean);
        job.steps.push(Step::run("s", "test", "cargo test"));
        job.cache_mounts.push(CacheMount {
            name: "target".to_string(),
            path: "target/".to_string(),
            mode: CacheMode::ReadOnly,
            fingerprint: "fp".to_string(),
        });
        job
    }

    #[test]
    fn fork_cannot_write_trusted_compiled_cache() {
        let plan = plan_cache_for_job(&job(), &TrustTier::ForkPr);
        assert!(
            plan.decisions
                .iter()
                .all(|d| !matches!(d.scope, CacheScope::RepoCompiledCas) || !d.write_allowed)
        );
    }

    #[test]
    fn release_rejects_mutable_compiled_cache() -> Result<(), Box<dyn std::error::Error>> {
        let plan = plan_cache_for_job(&job(), &TrustTier::ReleaseHermetic);
        assert_release_cache_safe(&plan)?;
        Ok(())
    }

    #[test]
    fn release_rejects_injected_mutable_compiled_cache() {
        let mut plan = plan_cache_for_job(&job(), &TrustTier::ReleaseHermetic);
        plan.decisions.push(CacheDecision {
            scope: CacheScope::RepoCompiledCas,
            read_allowed: true,
            write_allowed: false,
            promote_after_green: false,
            quarantine: false,
            mutable_compiled_cache_allowed: true,
            reason: "test injected unsafe compiled cache".to_string(),
        });
        assert!(matches!(
            assert_release_cache_safe(&plan),
            Err(CachePolicyError::ReleaseMutableCacheDenied)
        ));
    }

    #[test]
    fn fingerprint_changes_when_source_changes() {
        let mut inputs = FingerprintInputs::default_for_dev();
        let a = fingerprint_job(&job(), &TrustTier::InternalBranch, &inputs);
        inputs.crate_source_digest = "new-source".to_string();
        let b = fingerprint_job(&job(), &TrustTier::InternalBranch, &inputs);
        assert_ne!(a, b);
    }

    #[test]
    fn cross_project_compiled_denied_by_default() {
        assert!(matches!(
            assert_cross_project_compiled_allowed(false),
            Err(CachePolicyError::CrossProjectCompiledDenied)
        ));
    }
}
