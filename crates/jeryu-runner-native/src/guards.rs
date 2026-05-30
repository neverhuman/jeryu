#![doc = "Native runner guard checks for host capability exposure."]

use jeryu_runner_core::error::{RunnerError, RunnerResult};
use jeryu_runner_core::fscheck::{DENIED_ENV_VARS, sanitize_env, validate_mount_sources};
use jeryu_runner_core::job::JobRequest;
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_core::trust::RunnerClass;
use std::collections::BTreeMap;
use std::path::PathBuf;

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

    let mount_sources = plan
        .mounts
        .iter()
        .map(|mount| mount.source.clone())
        .collect::<Vec<PathBuf>>();
    validate_mount_sources(&mount_sources)?;

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
    fn sanitized_env_drops_ssh_auth_sock() {
        let mut job = job();
        job.env
            .insert("SSH_AUTH_SOCK".to_string(), "/tmp/agent".to_string());
        job.env.insert("RUST_LOG".to_string(), "debug".to_string());
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);
        let env = sanitized_native_env(&job, &plan);
        assert!(!env.contains_key("SSH_AUTH_SOCK"));
        assert_eq!(env.get("RUST_LOG"), Some(&"debug".to_string()));
    }
}
