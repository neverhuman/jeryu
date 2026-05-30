#![doc = "Native runner process supervisor."]

use crate::guards::{sanitized_native_env, validate_native_plan};
use jeryu_runner_core::error::{RunnerError, RunnerResult};
use jeryu_runner_core::job::JobRequest;
use jeryu_runner_core::policy::PolicyDecision;
use jeryu_runner_core::receipt::{Receipt, ReceiptStatus, now_ms};
use jeryu_runner_core::sandbox::SandboxPlan;
use std::fs;
use std::process::{Command, Stdio};

/// Native runner supervisor.
#[derive(Debug, Default, Clone)]
pub struct NativeRunner;

impl NativeRunner {
    /// Create a native runner.
    pub fn new() -> Self {
        Self
    }

    /// Execute a job under the native runner guard model.
    ///
    /// The Phase 4 code models namespace, seccomp, Landlock, and cgroups in the
    /// plan and enforces the portable safety-critical pieces directly: clean env,
    /// workspace cwd, denied host sockets/agents, no ambient secrets, and receipt
    /// generation. Production deployments should wire the plan into the host
    /// namespace/seccomp/Landlock launcher before enabling multi-tenant native use.
    pub fn execute(
        &self,
        job: &JobRequest,
        decision: &PolicyDecision,
        plan: &SandboxPlan,
    ) -> RunnerResult<Receipt> {
        job.validate()?;
        validate_native_plan(job, plan)?;
        fs::create_dir_all(&job.workspace)?;

        let started = now_ms();
        let env = sanitized_native_env(job, plan);
        let output = Command::new(&job.command)
            .args(&job.args)
            .current_dir(&job.workspace)
            .env_clear()
            .envs(env)
            .stdin(Stdio::null())
            .output();
        let finished = now_ms();

        match output {
            Ok(output) => {
                let exit_code = output.status.code();
                let status = if output.status.success() {
                    ReceiptStatus::Passed
                } else {
                    ReceiptStatus::Failed
                };
                let message = summarize_output(&output.stdout, &output.stderr);
                Ok(Receipt::new(
                    job, decision, plan, status, exit_code, started, finished, message,
                ))
            }
            Err(err) => Ok(Receipt::new(
                job,
                decision,
                plan,
                ReceiptStatus::Failed,
                None,
                started,
                finished,
                format!("process_start_failed: {err}"),
            )),
        }
    }

    /// Build a plan-only receipt for explain mode.
    pub fn plan_only(
        &self,
        job: &JobRequest,
        decision: &PolicyDecision,
        plan: &SandboxPlan,
    ) -> RunnerResult<Receipt> {
        job.validate()?;
        validate_native_plan(job, plan)?;
        let now = now_ms();
        Ok(Receipt::new(
            job,
            decision,
            plan,
            ReceiptStatus::Planned,
            None,
            now,
            now,
            "native runner plan created",
        ))
    }
}

fn summarize_output(stdout: &[u8], stderr: &[u8]) -> String {
    let mut message = String::new();
    if !stdout.is_empty() {
        message.push_str("stdout=");
        message.push_str(&lossy_limit(stdout, 4096));
    }
    if !stderr.is_empty() {
        if !message.is_empty() {
            message.push(' ');
        }
        message.push_str("stderr=");
        message.push_str(&lossy_limit(stderr, 4096));
    }
    if message.is_empty() {
        "process completed without output".to_string()
    } else {
        message
    }
}

fn lossy_limit(bytes: &[u8], limit: usize) -> String {
    let mut value = String::from_utf8_lossy(bytes).to_string();
    if value.len() > limit {
        value.truncate(limit);
        value.push_str("...[truncated]");
    }
    value
}

/// Convert policy denial into a typed error when a native class is missing.
pub fn native_class_required(plan: &SandboxPlan) -> RunnerResult<()> {
    if plan.runner_class.is_native() {
        Ok(())
    } else {
        Err(RunnerError::new(
            "invalid_native_runner",
            format!("{} is not native", plan.runner_class),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_runner_core::job::{NetworkPolicy, SecretPolicy, TokenPolicy};
    use jeryu_runner_core::policy::select_runner;
    use jeryu_runner_core::sandbox::SandboxPlan;
    use jeryu_runner_core::trust::TrustTier;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("jeryu-native-test-{stamp}"))
    }

    #[test]
    fn executes_echo_and_receipts_pass() {
        let workspace = temp_dir();
        let job = JobRequest {
            job_id: "job".to_string(),
            repo_id: "repo".to_string(),
            commit_sha: "abc".to_string(),
            workspace,
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
        };
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);
        let receipt = NativeRunner::new()
            .execute(&job, &decision, &plan)
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(receipt.status, ReceiptStatus::Passed);
        assert_eq!(receipt.exit_code, Some(0));
    }

    #[test]
    fn native_runner_sanitizes_process_environment() {
        let workspace = temp_dir();
        let mut env = std::collections::BTreeMap::new();
        env.insert("SSH_AUTH_SOCK".to_string(), "/tmp/leaked-agent".to_string());
        env.insert("AWS_ACCESS_KEY_ID".to_string(), "leaked-key".to_string());
        env.insert("RUST_LOG".to_string(), "debug".to_string());
        let job = JobRequest {
            job_id: "job".to_string(),
            repo_id: "repo".to_string(),
            commit_sha: "abc".to_string(),
            workspace,
            command: "/usr/bin/env".to_string(),
            args: vec!["-0".to_string()],
            env,
            trust_tier: TrustTier::T1ProtectedInternal,
            requested_runner: None,
            network_policy: NetworkPolicy::Deny,
            secret_policy: SecretPolicy::None,
            token_policy: TokenPolicy::ReadOnly,
            timeout_ms: 1000,
            fork: false,
        };
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);
        let receipt = NativeRunner::new()
            .execute(&job, &decision, &plan)
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(receipt.status, ReceiptStatus::Passed);
        assert!(receipt.message.contains("RUST_LOG=debug"));
        assert!(receipt.message.contains("JERYU_SECRETS=disabled"));
        assert!(!receipt.message.contains("leaked-agent"));
        assert!(!receipt.message.contains("leaked-key"));
        assert!(!receipt.message.contains("SSH_AUTH_SOCK="));
        assert!(!receipt.message.contains("AWS_ACCESS_KEY_ID="));
    }

    #[test]
    fn execute_rejects_invalid_job_before_spawn() {
        let mut job = JobRequest {
            job_id: "job".to_string(),
            repo_id: "repo".to_string(),
            commit_sha: "abc".to_string(),
            workspace: temp_dir(),
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
        };
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);
        job.workspace = PathBuf::from("relative");
        let err = NativeRunner::new()
            .execute(&job, &decision, &plan)
            .err()
            .unwrap_or_else(|| panic!("expected validation failure"));
        assert_eq!(err.code(), "invalid_workspace");
    }

    #[test]
    fn plan_only_rejects_invalid_job_before_receipt() {
        let mut job = JobRequest {
            job_id: "job".to_string(),
            repo_id: "repo".to_string(),
            commit_sha: "abc".to_string(),
            workspace: temp_dir(),
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
        };
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);
        job.timeout_ms = 0;
        let err = NativeRunner::new()
            .plan_only(&job, &decision, &plan)
            .err()
            .unwrap_or_else(|| panic!("expected validation failure"));
        assert_eq!(err.code(), "invalid_job");
    }
}
