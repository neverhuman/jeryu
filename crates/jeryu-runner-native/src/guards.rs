#![doc = "Native runner guard checks for host capability exposure."]

use jeryu_runner_core::error::{RunnerError, RunnerResult};
use jeryu_runner_core::fscheck::{
    DENIED_ENV_VARS, deny_dangerous_host_path, sanitize_env, validate_mount_sources,
};
use jeryu_runner_core::job::JobRequest;
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_core::trust::RunnerClass;
use jeryu_sandbox_linux::capability::{EnforcementLevel, SandboxCapabilities};
use jeryu_sandbox_linux::launch::EnforcementReport;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Prove (post-spawn) that the kernel actually enforced the sandbox by reading
/// the live child's `/proc/<pid>/status`. This is the SAFE re-export the native
/// runner consumes; all unsafe lives in `jeryu-sandbox-linux`.
pub fn verify_enforcement(pid: u32, level: &EnforcementLevel) -> EnforcementReport {
    jeryu_sandbox_linux::launch::verify_enforcement(pid, level)
}

/// Pre-flight enforcement gate: resolve the level for the plan against the
/// probed host capabilities and REFUSE to run when the sandbox cannot fail
/// closed (`Unavailable`). `Degraded` is allowed but the missing primitives are
/// returned so the caller can record them honestly in the receipt.
pub fn preflight_enforcement(
    plan: &SandboxPlan,
    caps: &SandboxCapabilities,
) -> RunnerResult<EnforcementLevel> {
    let level = caps.enforcement_level(plan);
    if let EnforcementLevel::Unavailable { reason } = &level {
        return Err(RunnerError::new("sandbox_unavailable", reason.clone()));
    }
    Ok(level)
}

/// Validate that the sandbox plan is safe for native execution.
pub fn validate_native_plan(job: &JobRequest, plan: &SandboxPlan) -> RunnerResult<()> {
    if !matches!(
        plan.runner_class,
        RunnerClass::NativeRustHot
            | RunnerClass::NativeRustClean
            | RunnerClass::AgentGuard
            | RunnerClass::ReleaseHermetic
    ) {
        return Err(RunnerError::new(
            "invalid_native_runner",
            format!("{} is not a native runner class", plan.runner_class),
        ));
    }

    if !plan.user_namespace || !plan.mount_namespace || !plan.pid_namespace {
        return Err(RunnerError::new(
            "sandbox_policy_denied",
            "native plan must use user, mount, and pid namespaces",
        ));
    }

    if plan.seccomp.default_action != "kill-process"
        || !plan
            .seccomp
            .allow_groups
            .iter()
            .any(|group| group == "process-basic")
        || !plan
            .seccomp
            .allow_groups
            .iter()
            .any(|group| group == "file-readwrite-workspace")
    {
        return Err(RunnerError::new(
            "sandbox_policy_denied",
            "native plan must use fail-closed seccomp with workspace file access only",
        ));
    }

    if plan.cgroup_limits.memory_max_bytes == 0
        || plan.cgroup_limits.cpu_weight == 0
        || plan.cgroup_limits.pids_max == 0
        || plan.cgroup_limits.io_weight == 0
    {
        return Err(RunnerError::new(
            "sandbox_policy_denied",
            "native plan must include nonzero cgroup limits",
        ));
    }

    let workspace_landlock = plan
        .landlock_rules
        .iter()
        .any(|rule| rule.path == job.workspace && rule.read && rule.write && rule.execute);
    if !workspace_landlock {
        return Err(RunnerError::new(
            "sandbox_policy_denied",
            "native plan must allow read/write/execute only for the job workspace",
        ));
    }

    for rule in &plan.landlock_rules {
        deny_dangerous_host_path(&rule.path)?;
        if matches!(rule.path.to_str(), Some("/usr" | "/nix/store")) && rule.write {
            return Err(RunnerError::new(
                "sandbox_policy_denied",
                format!(
                    "{} must be read-only in native Landlock rules",
                    rule.path.display()
                ),
            ));
        }
    }

    let mount_sources = plan
        .mounts
        .iter()
        .map(|mount| mount.source.clone())
        .collect::<Vec<PathBuf>>();
    validate_mount_sources(&mount_sources)?;
    for mount in &plan.mounts {
        if (matches!(mount.source.to_str(), Some("/usr" | "/nix/store"))
            || matches!(mount.target.to_str(), Some("/usr" | "/nix/store")))
            && !mount.read_only
        {
            return Err(RunnerError::new(
                "sandbox_policy_denied",
                format!(
                    "{} -> {} must be read-only in native mounts",
                    mount.source.display(),
                    mount.target.display()
                ),
            ));
        }
    }
    let usr_mount_read_only = plan.mounts.iter().any(|mount| {
        mount.source == Path::new("/usr") && mount.target == Path::new("/usr") && mount.read_only
    });
    if !usr_mount_read_only {
        return Err(RunnerError::new(
            "sandbox_policy_denied",
            "native plan must mount /usr read-only",
        ));
    }

    if job.fork && plan.runner_class == RunnerClass::NativeRustHot {
        return Err(RunnerError::new(
            "sandbox_policy_denied",
            "fork jobs cannot run on native-rust-hot",
        ));
    }

    Ok(())
}

/// Build the sanitized native environment.
pub fn sanitized_native_env(job: &JobRequest, plan: &SandboxPlan) -> BTreeMap<String, String> {
    let mut env = sanitize_env(&job.env);
    env.insert("CI".to_string(), "true".to_string());
    env.insert("JERYU_JOB_ID".to_string(), job.job_id.clone());
    env.insert("JERYU_REPO_ID".to_string(), job.repo_id.clone());
    env.insert("JERYU_COMMIT_SHA".to_string(), job.commit_sha.clone());
    env.insert(
        "JERYU_RUNNER_CLASS".to_string(),
        plan.runner_class.as_str().to_string(),
    );
    env.insert(
        "JERYU_NETWORK_POLICY".to_string(),
        plan.network_policy.as_str().to_string(),
    );
    env.insert("HOME".to_string(), "/tmp/jeryu-home".to_string());
    env.insert("TMPDIR".to_string(), "/tmp".to_string());
    env.insert(
        "PATH".to_string(),
        "/usr/local/bin:/usr/bin:/bin".to_string(),
    );
    if !plan.allow_secrets {
        env.insert("JERYU_SECRETS".to_string(), "disabled".to_string());
    }
    for denied in DENIED_ENV_VARS {
        env.remove(*denied);
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_runner_core::job::{NetworkPolicy, SecretPolicy, TokenPolicy};
    use jeryu_runner_core::policy::select_runner;
    use jeryu_runner_core::sandbox::SandboxPlan;
    use jeryu_runner_core::trust::TrustTier;
    use std::path::PathBuf;

    fn job() -> JobRequest {
        JobRequest {
            job_id: "job".to_string(),
            repo_id: "repo".to_string(),
            commit_sha: "abc".to_string(),
            workspace: PathBuf::from("/tmp/work"),
            command: "/bin/echo".to_string(),
            args: vec!["ok".to_string()],
            env: Default::default(),
            trust_tier: TrustTier::T1ProtectedInternal,
            requested_runner: None,
            network_policy: NetworkPolicy::Deny,
            secret_policy: SecretPolicy::Default,
            token_policy: TokenPolicy::ReadOnly,
            timeout_ms: 1000,
            fork: false,
        }
    }

    #[test]
    fn native_plan_validates() {
        let job = job();
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);
        validate_native_plan(&job, &plan).unwrap_or_else(|err| panic!("{err}"));
    }

    #[test]
    fn native_plan_rejects_missing_namespace() {
        let job = job();
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let mut plan = SandboxPlan::from_decision(&job.workspace, &decision);
        plan.pid_namespace = false;
        let err = validate_native_plan(&job, &plan)
            .err()
            .unwrap_or_else(|| panic!("expected namespace denial"));
        assert_eq!(err.code(), "sandbox_policy_denied");
        assert!(err.message().contains("namespaces"));
    }

    #[test]
    fn native_plan_rejects_non_fail_closed_seccomp() {
        let job = job();
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let mut plan = SandboxPlan::from_decision(&job.workspace, &decision);
        plan.seccomp.default_action = "allow".to_string();
        let err = validate_native_plan(&job, &plan)
            .err()
            .unwrap_or_else(|| panic!("expected seccomp denial"));
        assert_eq!(err.code(), "sandbox_policy_denied");
        assert!(err.message().contains("seccomp"));
    }

    #[test]
    fn native_plan_rejects_missing_cgroup_limits() {
        let job = job();
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let mut plan = SandboxPlan::from_decision(&job.workspace, &decision);
        plan.cgroup_limits.pids_max = 0;
        let err = validate_native_plan(&job, &plan)
            .err()
            .unwrap_or_else(|| panic!("expected cgroup denial"));
        assert_eq!(err.code(), "sandbox_policy_denied");
        assert!(err.message().contains("cgroup"));
    }

    #[test]
    fn native_plan_rejects_missing_workspace_landlock() {
        let job = job();
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let mut plan = SandboxPlan::from_decision(&job.workspace, &decision);
        plan.landlock_rules
            .retain(|rule| rule.path != job.workspace);
        let err = validate_native_plan(&job, &plan)
            .err()
            .unwrap_or_else(|| panic!("expected Landlock denial"));
        assert_eq!(err.code(), "sandbox_policy_denied");
        assert!(err.message().contains("workspace"));
    }

    #[test]
    fn native_plan_rejects_writable_system_landlock() {
        let job = job();
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let mut plan = SandboxPlan::from_decision(&job.workspace, &decision);
        let usr_rule = plan
            .landlock_rules
            .iter_mut()
            .find(|rule| rule.path == Path::new("/usr"))
            .unwrap_or_else(|| panic!("expected /usr rule"));
        usr_rule.write = true;
        let err = validate_native_plan(&job, &plan)
            .err()
            .unwrap_or_else(|| panic!("expected read-only denial"));
        assert_eq!(err.code(), "sandbox_policy_denied");
        assert!(err.message().contains("read-only"));
    }

    #[test]
    fn native_plan_rejects_dangerous_mount_source() {
        let job = job();
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let mut plan = SandboxPlan::from_decision(&job.workspace, &decision);
        plan.mounts[0].source = PathBuf::from("/var/run/docker.sock");
        let err = validate_native_plan(&job, &plan)
            .err()
            .unwrap_or_else(|| panic!("expected host path denial"));
        assert_eq!(err.code(), "host_path_denied");
    }

    #[test]
    fn native_plan_rejects_writable_system_mount() {
        let job = job();
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let mut plan = SandboxPlan::from_decision(&job.workspace, &decision);
        let usr_mount = plan
            .mounts
            .iter_mut()
            .find(|mount| mount.source == Path::new("/usr"))
            .unwrap_or_else(|| panic!("expected /usr mount"));
        usr_mount.read_only = false;
        let err = validate_native_plan(&job, &plan)
            .err()
            .unwrap_or_else(|| panic!("expected read-only denial"));
        assert_eq!(err.code(), "sandbox_policy_denied");
        assert!(err.message().contains("read-only"));
    }

    #[test]
    fn sanitized_env_drops_ssh_auth_sock() {
        let mut job = job();
        job.secret_policy = SecretPolicy::None;
        for denied in DENIED_ENV_VARS {
            job.env
                .insert((*denied).to_string(), format!("leaked-{denied}"));
        }
        job.env.insert("RUST_LOG".to_string(), "debug".to_string());
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);
        let env = sanitized_native_env(&job, &plan);
        for denied in DENIED_ENV_VARS {
            assert!(!env.contains_key(*denied), "{denied} must be scrubbed");
        }
        assert_eq!(env.get("RUST_LOG"), Some(&"debug".to_string()));
        assert_eq!(env.get("JERYU_SECRETS"), Some(&"disabled".to_string()));
    }
}
