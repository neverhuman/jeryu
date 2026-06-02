#![doc = "Native runner process supervisor."]

use crate::guards::{sanitized_native_env, validate_native_plan, verify_enforcement};
use jeryu_runner_core::error::{RunnerError, RunnerResult};
use jeryu_runner_core::job::JobRequest;
use jeryu_runner_core::policy::PolicyDecision;
use jeryu_runner_core::receipt::{Receipt, ReceiptStatus, now_ms};
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_sandbox_linux::capability::SandboxCapabilities;
use jeryu_sandbox_linux::launch::spawn_sandboxed;
use jeryu_sandbox_linux::watchdog::run_with_watchdog;
use std::fs;
use std::sync::OnceLock;
use std::time::Duration;

/// Probe the host sandbox capabilities exactly once and cache the result; the
/// throwaway-child probes are cheap but pointless to repeat per job.
fn cached_capabilities() -> &'static SandboxCapabilities {
    static CAPS: OnceLock<SandboxCapabilities> = OnceLock::new();
    CAPS.get_or_init(SandboxCapabilities::probe)
}

/// Native runner supervisor.
#[derive(Debug, Default, Clone)]
pub struct NativeRunner;

impl NativeRunner {
    /// Create a native runner.
    pub fn new() -> Self {
        Self
    }

    /// Execute a job under the REAL native syscall sandbox.
    ///
    /// The pipeline is: probe host capabilities once (cached) -> validate the
    /// plan -> resolve the enforcement level -> `spawn_sandboxed` (applies
    /// `PR_SET_NO_NEW_PRIVS`, cgroups, Landlock, seccomp, namespaces via
    /// `pre_exec`, FAIL-CLOSED) -> `run_with_watchdog` (group-kill on timeout)
    /// -> `verify_enforcement` (prove enforcement from `/proc/<pid>/status`).
    ///
    /// Enforcement state is first-class and honest: when the host degrades a
    /// primitive (e.g. unprivileged user namespaces are blocked, or cgroup
    /// delegation is unusable), the receipt message records exactly what was
    /// applied vs. skipped. The runner refuses to run at all when the sandbox is
    /// `Unavailable` (cannot fail closed). All unsafe lives in
    /// `jeryu-sandbox-linux`; this crate stays SAFE.
    pub fn execute(
        &self,
        job: &JobRequest,
        decision: &PolicyDecision,
        plan: &SandboxPlan,
    ) -> RunnerResult<Receipt> {
        job.validate()?;
        validate_native_plan(job, plan)?;
        fs::create_dir_all(&job.workspace)?;

        let caps = cached_capabilities();
        let level = caps.enforcement_level(plan);
        let env = sanitized_native_env(job, plan);

        let started = now_ms();
        let child = match spawn_sandboxed(job, plan, caps, &env) {
            Ok(child) => child,
            Err(err) => {
                let finished = now_ms();
                // Unavailable / setup failures are fail-closed: the job did not
                // run, and the receipt explains why.
                return Ok(Receipt::new(
                    job,
                    decision,
                    plan,
                    ReceiptStatus::Failed,
                    None,
                    started,
                    finished,
                    format!("sandbox_launch_failed[{}]: {}", err.code(), err.message()),
                ));
            }
        };

        // Prove enforcement from /proc/<pid>/status while the child is live.
        let report = verify_enforcement(child.id(), &level);

        let timeout = Duration::from_millis(job.timeout_ms.max(1));
        let finished;
        let result = match run_with_watchdog(child, timeout) {
            Ok(outcome) => {
                finished = now_ms();
                outcome
            }
            Err(err) => {
                let finished = now_ms();
                return Ok(Receipt::new(
                    job,
                    decision,
                    plan,
                    ReceiptStatus::Failed,
                    None,
                    started,
                    finished,
                    format!("watchdog_failed: {err}"),
                ));
            }
        };

        let status = if result.timed_out {
            ReceiptStatus::TimedOut
        } else if result.exit_code == Some(0) {
            ReceiptStatus::Passed
        } else {
            ReceiptStatus::Failed
        };

        let mut message = summarize_output(&result.stdout, &result.stderr);
        message.push_str(&format!(" enforcement={}", enforcement_summary(&report)));
        if result.timed_out {
            message.push_str(&format!(
                " timed_out_after_ms={}",
                result.elapsed.as_millis()
            ));
        }

        Ok(Receipt::new(
            job,
            decision,
            plan,
            status,
            result.exit_code,
            started,
            finished,
            message,
        ))
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

/// Compact, receipt-friendly summary of the proven enforcement state.
fn enforcement_summary(report: &jeryu_sandbox_linux::launch::EnforcementReport) -> String {
    format!(
        "level={} applied=[{}] skipped=[{}] proc_no_new_privs={} proc_seccomp={}",
        report.level,
        report.applied.join(","),
        report.skipped.join(","),
        report
            .proc_no_new_privs
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".to_string()),
        report
            .proc_seccomp
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".to_string()),
    )
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
    use std::sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    static EXECUTION_GUARD: Mutex<()> = Mutex::new(());

    fn temp_dir() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("jeryu-native-test-{stamp}-{unique}"))
    }

    #[test]
    fn executes_echo_and_receipts_pass() {
        let _guard = EXECUTION_GUARD.lock().unwrap();
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
        // The receipt must carry the PROVEN enforcement state, not a claim.
        assert!(
            receipt.message.contains("enforcement=level="),
            "receipt must record enforcement state: {}",
            receipt.message
        );
    }

    #[test]
    fn watchdog_timeout_maps_to_timed_out_status() {
        let _guard = EXECUTION_GUARD.lock().unwrap();
        let workspace = temp_dir();
        let job = JobRequest {
            job_id: "job".to_string(),
            repo_id: "repo".to_string(),
            commit_sha: "abc".to_string(),
            workspace,
            command: "/bin/sleep".to_string(),
            args: vec!["30".to_string()],
            env: Default::default(),
            trust_tier: TrustTier::T1ProtectedInternal,
            requested_runner: None,
            network_policy: NetworkPolicy::Deny,
            secret_policy: SecretPolicy::Default,
            token_policy: TokenPolicy::ReadOnly,
            timeout_ms: 250,
            fork: false,
        };
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);
        let receipt = NativeRunner::new()
            .execute(&job, &decision, &plan)
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(
            receipt.status,
            ReceiptStatus::TimedOut,
            "a runaway must be killed by the watchdog and recorded as TimedOut"
        );
        assert!(receipt.message.contains("timed_out_after_ms="));
    }

    #[test]
    fn native_runner_sanitizes_process_environment() {
        let _guard = EXECUTION_GUARD.lock().unwrap();
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
