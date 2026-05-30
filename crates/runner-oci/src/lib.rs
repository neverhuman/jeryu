#![doc = "OCI compatibility runner for Docker/Podman-style jobs."]

use runner_core::error::{RunnerError, RunnerResult};
use runner_core::fscheck::deny_dangerous_host_path;
use runner_core::job::JobRequest;
use runner_core::policy::PolicyDecision;
use runner_core::receipt::{now_ms, Receipt, ReceiptStatus};
use runner_core::sandbox::SandboxPlan;
use runner_core::trust::RunnerClass;
use std::path::Path;
use std::process::{Command, Stdio};

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
        let runtime =
            std::env::var("JITFORGE_OCI_RUNTIME").unwrap_or_else(|_| "podman".to_string());
        let image = std::env::var("JITFORGE_OCI_IMAGE")
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
#[derive(Debug, Default, Clone)]
pub struct OciRunner;

impl OciRunner {
    /// Create an OCI runner.
    pub fn new() -> Self {
        Self
    }

    /// Plan or execute OCI job.
    ///
    /// Default is plan-only. Set `JITFORGE_RUN_OCI=1` to execute.
    pub fn execute(
        &self,
        job: &JobRequest,
        decision: &PolicyDecision,
        plan: &SandboxPlan,
    ) -> RunnerResult<Receipt> {
        let spec = OciSpec::from_job(job, plan)?;
        let started = now_ms();
        if std::env::var("JITFORGE_RUN_OCI").ok().as_deref() != Some("1") {
            return Ok(Receipt::new(
                job,
                decision,
                plan,
                ReceiptStatus::Planned,
                None,
                started,
                started,
                spec.explain(),
            ));
        }
        let output = Command::new(&spec.runtime)
            .args(spec.args())
            .stdin(Stdio::null())
            .output();
        let finished = now_ms();
        match output {
            Ok(output) => Ok(Receipt::new(
                job,
                decision,
                plan,
                if output.status.success() {
                    ReceiptStatus::Passed
                } else {
                    ReceiptStatus::Failed
                },
                output.status.code(),
                started,
                finished,
                spec.explain(),
            )),
            Err(err) => Ok(Receipt::new(
                job,
                decision,
                plan,
                ReceiptStatus::Failed,
                None,
                started,
                finished,
                format!("oci_start_failed: {err}; {}", spec.explain()),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runner_core::job::{NetworkPolicy, SecretPolicy, TokenPolicy};
    use runner_core::policy::select_runner;
    use runner_core::sandbox::SandboxPlan;
    use runner_core::trust::{RunnerClass, TrustTier};
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
}
