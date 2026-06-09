#![doc = "OCI compatibility runner for Docker/Podman-style jobs."]

pub mod runtime;

pub use runtime::{CliContainerRuntime, ContainerRuntime, FakeContainerRuntime, RuntimeOutcome};

use jeryu_runner_core::error::{RunnerError, RunnerResult};
use jeryu_runner_core::fscheck::deny_dangerous_host_path;
use jeryu_runner_core::job::JobRequest;
use jeryu_runner_core::policy::PolicyDecision;
use jeryu_runner_core::receipt::{Receipt, ReceiptStatus, now_ms};
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_core::trust::RunnerClass;
use std::path::Path;
use std::sync::Arc;

/// OCI launch spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OciSpec {
    /// Runtime executable, e.g. podman or docker.
    pub runtime: String,
    /// Image reference.
    pub image: String,
    /// Workspace bind mount.
    pub workspace: String,
    /// Command argv.
    pub command: Vec<String>,
    /// Network mode passed to runtime.
    pub network: String,
}

impl OciSpec {
    /// Build OCI spec from job and sandbox plan.
    pub fn from_job(job: &JobRequest, plan: &SandboxPlan) -> RunnerResult<Self> {
        if plan.runner_class != RunnerClass::OciDocker {
            return Err(RunnerError::new(
                "invalid_oci_runner",
                format!("{} is not oci-docker", plan.runner_class),
            ));
        }
        deny_dangerous_host_path(Path::new(&job.workspace))?;
        let runtime = std::env::var("JERYU_OCI_RUNTIME").unwrap_or_else(|_| "podman".to_string());
        let image = std::env::var("JERYU_OCI_IMAGE")
            .unwrap_or_else(|_| "docker.io/library/rust:latest".to_string());
        let mut command = vec![job.command.clone()];
        command.extend(job.args.clone());
        Ok(Self {
            runtime,
            image,
            workspace: job.workspace.display().to_string(),
            command,
            network: match plan.network_policy.as_str() {
                "deny" => "none".to_string(),
                other => other.to_string(),
            },
        })
    }

    /// Runtime args without the executable.
    pub fn args(&self) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--rm".to_string(),
            "--network".to_string(),
            self.network.clone(),
            "-v".to_string(),
            format!("{}:/workspace:Z", self.workspace),
            "-w".to_string(),
            "/workspace".to_string(),
            self.image.clone(),
        ];
        args.extend(self.command.clone());
        args
    }

    /// Explain this spec without secrets.
    pub fn explain(&self) -> String {
        format!("oci runtime={} {}", self.runtime, self.args().join(" "))
    }
}

/// OCI runner.
///
/// Execution is delegated to a [`ContainerRuntime`]. The default
/// [`CliContainerRuntime`] keeps plan-only behavior unless `JERYU_RUN_OCI=1` is
/// set; injecting any other runtime (e.g. [`FakeContainerRuntime`]) is itself
/// the opt-in to execute, so injected runtimes do not consult that gate.
#[derive(Debug, Clone)]
pub struct OciRunner {
    runtime: Arc<dyn ContainerRuntime>,
}

impl Default for OciRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl OciRunner {
    /// Create an OCI runner backed by the real CLI runtime.
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(CliContainerRuntime),
        }
    }

    /// Create an OCI runner backed by an injected runtime.
    pub fn with_runtime(rt: Arc<dyn ContainerRuntime>) -> Self {
        Self { runtime: rt }
    }

    /// Plan or execute OCI job.
    ///
    /// The spec is built unchanged, then handed to the configured runtime. A
    /// plan-only outcome (`ran=false`) yields a `Planned` receipt; an executed
    /// outcome yields `Passed` on exit 0 and `Failed` otherwise. With the
    /// default [`CliContainerRuntime`], plan-only is the default unless
    /// `JERYU_RUN_OCI=1` is set.
    pub fn execute(
        &self,
        job: &JobRequest,
        decision: &PolicyDecision,
        plan: &SandboxPlan,
    ) -> RunnerResult<Receipt> {
        let spec = OciSpec::from_job(job, plan)?;
        let started = now_ms();
        match self.runtime.run(&spec) {
            Ok(outcome) => {
                let finished = now_ms();
                let status = if !outcome.ran {
                    ReceiptStatus::Planned
                } else if outcome.exit_code == Some(0) {
                    ReceiptStatus::Passed
                } else {
                    ReceiptStatus::Failed
                };
                Ok(Receipt::new(
                    job,
                    decision,
                    plan,
                    status,
                    outcome.exit_code,
                    started,
                    finished,
                    spec.explain(),
                ))
            }
            Err(err) => Ok(Receipt::new(
                job,
                decision,
                plan,
                ReceiptStatus::Failed,
                None,
                started,
                now_ms(),
                format!("{err}; {}", spec.explain()),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_runner_core::job::{NetworkPolicy, SecretPolicy, TokenPolicy};
    use jeryu_runner_core::policy::select_runner;
    use jeryu_runner_core::sandbox::SandboxPlan;
    use jeryu_runner_core::trust::{RunnerClass, TrustTier};
    use std::path::PathBuf;

    #[test]
    fn oci_spec_uses_network_none_for_deny() {
        let job = JobRequest {
            job_id: "job".to_string(),
            repo_id: "repo".to_string(),
            commit_sha: "abc".to_string(),
            workspace: PathBuf::from("/tmp/work"),
            command: "/bin/echo".to_string(),
            args: vec!["ok".to_string()],
            env: Default::default(),
            trust_tier: TrustTier::T4ForkPr,
            requested_runner: Some(RunnerClass::OciDocker),
            network_policy: NetworkPolicy::Deny,
            secret_policy: SecretPolicy::Default,
            token_policy: TokenPolicy::ReadOnly,
            timeout_ms: 1000,
            fork: true,
        };
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);
        let spec = OciSpec::from_job(&job, &plan).unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(spec.network, "none");
        assert!(spec.args().iter().any(|arg| arg == "--network"));
    }

    #[test]
    fn oci_spec_rejects_dangerous_workspace() {
        let job = JobRequest {
            job_id: "job".to_string(),
            repo_id: "repo".to_string(),
            commit_sha: "abc".to_string(),
            workspace: PathBuf::from("/var/run/docker.sock"),
            command: "/bin/echo".to_string(),
            args: vec!["ok".to_string()],
            env: Default::default(),
            trust_tier: TrustTier::T4ForkPr,
            requested_runner: Some(RunnerClass::OciDocker),
            network_policy: NetworkPolicy::Deny,
            secret_policy: SecretPolicy::Default,
            token_policy: TokenPolicy::ReadOnly,
            timeout_ms: 1000,
            fork: true,
        };
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);
        let err = OciSpec::from_job(&job, &plan)
            .err()
            .unwrap_or_else(|| panic!("expected host path denial"));
        assert_eq!(err.code(), "host_path_denied");
    }
}
