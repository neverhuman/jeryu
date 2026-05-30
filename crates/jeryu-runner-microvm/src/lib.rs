#![doc = "MicroVM runner planner/supervisor for untrusted Phase 4 jobs."]

use jeryu_runner_core::error::{RunnerError, RunnerResult};
use jeryu_runner_core::job::JobRequest;
use jeryu_runner_core::policy::PolicyDecision;
use jeryu_runner_core::receipt::{Receipt, ReceiptStatus, now_ms};
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_core::trust::RunnerClass;
use std::process::{Command, Stdio};

/// MicroVM launch specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicroVmSpec {
    /// MicroVM executable, e.g. firecracker.
    pub executable: String,
    /// Kernel image path.
    pub kernel_image: String,
    /// Rootfs image path.
    pub rootfs_image: String,
    /// Workdir exposed through a virtio/vsock-backed handoff.
    pub workspace: String,
    /// Command to execute inside guest.
    pub guest_command: Vec<String>,
    /// Network mode.
    pub network: String,
}

impl MicroVmSpec {
    /// Build a spec from a job and sandbox plan.
    pub fn from_job(job: &JobRequest, plan: &SandboxPlan) -> RunnerResult<Self> {
        if plan.runner_class != RunnerClass::MicroVmRust {
            return Err(RunnerError::new(
                "invalid_microvm_runner",
                format!("{} is not microvm-rust", plan.runner_class),
            ));
        }
        let executable =
            std::env::var("JERYU_MICROVM_BIN").unwrap_or_else(|_| "firecracker".to_string());
        let kernel_image = std::env::var("JERYU_MICROVM_KERNEL")
            .unwrap_or_else(|_| "/var/lib/jeryu/microvm/vmlinux".to_string());
        let rootfs_image = std::env::var("JERYU_MICROVM_ROOTFS")
            .unwrap_or_else(|_| "/var/lib/jeryu/microvm/rust-rootfs.ext4".to_string());
        let mut guest_command = vec![job.command.clone()];
        guest_command.extend(job.args.clone());
        Ok(Self {
            executable,
            kernel_image,
            rootfs_image,
            workspace: job.workspace.display().to_string(),
            guest_command,
            network: plan.network_policy.as_str().to_string(),
        })
    }

    /// Explain this spec without exposing secrets.
    pub fn explain(&self) -> String {
        format!(
            "microvm executable={} kernel={} rootfs={} workspace={} network={} argv={}",
            self.executable,
            self.kernel_image,
            self.rootfs_image,
            self.workspace,
            self.network,
            self.guest_command.join(" ")
        )
    }
}

/// MicroVM runner.
#[derive(Debug, Default, Clone)]
pub struct MicroVmRunner;

impl MicroVmRunner {
    /// Create a microVM runner.
    pub fn new() -> Self {
        Self
    }

    /// Plan or execute a microVM job.
    ///
    /// By default this returns a plan receipt. Set `JERYU_RUN_MICROVM=1` to
    /// invoke the configured launcher. The launcher must consume the generated
    /// spec through its own config adapter in production.
    pub fn execute(
        &self,
        job: &JobRequest,
        decision: &PolicyDecision,
        plan: &SandboxPlan,
    ) -> RunnerResult<Receipt> {
        let spec = MicroVmSpec::from_job(job, plan)?;
        let started = now_ms();
        if std::env::var("JERYU_RUN_MICROVM").ok().as_deref() != Some("1") {
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

        let output = Command::new(&spec.executable)
            .arg("--no-api")
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
                format!("microvm_start_failed: {err}; {}", spec.explain()),
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
    use jeryu_runner_core::trust::TrustTier;
    use std::path::PathBuf;

    #[test]
    fn fork_job_builds_microvm_spec() {
        let job = JobRequest {
            job_id: "job".to_string(),
            repo_id: "repo".to_string(),
            commit_sha: "abc".to_string(),
            workspace: PathBuf::from("/tmp/work"),
            command: "/bin/echo".to_string(),
            args: vec!["ok".to_string()],
            env: Default::default(),
            trust_tier: TrustTier::T4ForkPr,
            requested_runner: None,
            network_policy: NetworkPolicy::Deny,
            secret_policy: SecretPolicy::Default,
            token_policy: TokenPolicy::ReadOnly,
            timeout_ms: 1000,
            fork: true,
        };
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);
        let spec = MicroVmSpec::from_job(&job, &plan).unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(spec.network, "deny");
        assert!(spec.explain().contains("microvm"));
    }
}
