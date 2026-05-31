use super::*;
use jeryu_ci_ir::{CacheMode, CacheMount, Job, RunnerClass, Step, TrustTier};

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

#[test]
fn validate_rejects_missing_cache_law_inputs() {
    for required in [
        "cargo_lock_subgraph_digest",
        "workspace_metadata_digest",
        "crate_source_digest",
        "build_rs_digest",
        "proc_macro_digest",
        "native_deps_digest",
        "env_allowlist_digest",
        "runner_rootfs_digest",
        "sandbox_policy_digest",
    ] {
        let mut inputs = FingerprintInputs::default_for_dev();
        blank_input(&mut inputs, required);

        assert!(matches!(
            inputs.validate(),
            Err(CachePolicyError::MissingFingerprintInput(name)) if name == required
        ));
    }
}

fn blank_input(inputs: &mut FingerprintInputs, name: &str) {
    match name {
        "cargo_lock_subgraph_digest" => inputs.cargo_lock_subgraph_digest.clear(),
        "workspace_metadata_digest" => inputs.workspace_metadata_digest.clear(),
        "crate_source_digest" => inputs.crate_source_digest.clear(),
        "build_rs_digest" => inputs.build_rs_digest.clear(),
        "proc_macro_digest" => inputs.proc_macro_digest.clear(),
        "native_deps_digest" => inputs.native_deps_digest.clear(),
        "env_allowlist_digest" => inputs.env_allowlist_digest.clear(),
        "runner_rootfs_digest" => inputs.runner_rootfs_digest.clear(),
        "sandbox_policy_digest" => inputs.sandbox_policy_digest.clear(),
        other => panic!("unhandled required input {other}"),
    }
}
