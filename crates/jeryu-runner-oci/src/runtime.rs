#![doc = "Runtime seam for OCI execution so the runner is testable without a daemon."]

use crate::OciSpec;
use crate::lifecycle::{ContainerLifecycle, WarmContainer, WarmContainerSpec};
use jeryu_runner_core::error::{RunnerError, RunnerResult};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Outcome of asking a [`ContainerRuntime`] to run a spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOutcome {
    /// Process exit code, when a container actually ran and reported one.
    pub exit_code: Option<i32>,
    /// Whether a container actually executed. `false` means plan-only.
    pub ran: bool,
}

/// Abstraction over the container engine that runs an [`OciSpec`].
///
/// The seam lets the runner be exercised in CI with a fake engine, with no
/// real Docker/Podman daemon present.
pub trait ContainerRuntime: Send + Sync + std::fmt::Debug {
    /// Short name of the runtime, for diagnostics.
    fn name(&self) -> &str;

    /// Run the spec, returning whether a container executed and its exit code.
    fn run(&self, spec: &OciSpec) -> RunnerResult<RuntimeOutcome>;
}

/// Real runtime that shells out to the configured CLI (`podman`/`docker`).
///
/// This is the only runtime that honors the `JERYU_RUN_OCI=1` gate: without it
/// the runtime is plan-only and never touches the daemon. Any other injected
/// runtime (the injection itself is the opt-in) executes unconditionally.
#[derive(Debug, Default, Clone)]
pub struct CliContainerRuntime;

impl CliContainerRuntime {
    fn gate_open() -> bool {
        std::env::var("JERYU_RUN_OCI").ok().as_deref() == Some("1")
    }

    fn engine() -> String {
        std::env::var("JERYU_OCI_RUNTIME").unwrap_or_else(|_| "podman".to_string())
    }

    fn agent_image() -> String {
        std::env::var("JERYU_AGENT_IMAGE")
            .unwrap_or_else(|_| "localhost/jeryu/agent-sandbox:latest".to_string())
    }
}

impl ContainerRuntime for CliContainerRuntime {
    fn name(&self) -> &str {
        "cli"
    }

    fn run(&self, spec: &OciSpec) -> RunnerResult<RuntimeOutcome> {
        if !Self::gate_open() {
            return Ok(RuntimeOutcome {
                exit_code: None,
                ran: false,
            });
        }
        let output = Command::new(&spec.runtime)
            .args(spec.args())
            .stdin(Stdio::null())
            .output()
            .map_err(|err| RunnerError::new("oci_start_failed", err.to_string()))?;
        Ok(RuntimeOutcome {
            exit_code: output.status.code(),
            ran: true,
        })
    }
}

impl ContainerLifecycle for CliContainerRuntime {
    fn start_warm(&self, workcell_id: &str) -> RunnerResult<String> {
        let spec = WarmContainerSpec::new(
            Self::engine(),
            Self::agent_image(),
            "none",
            workcell_id.to_string(),
        );
        if !Self::gate_open() {
            return Ok(format!("planned-{workcell_id}"));
        }
        let output = Command::new(&spec.runtime)
            .args(spec.args())
            .stdin(Stdio::null())
            .output()
            .map_err(|err| RunnerError::new("oci_warm_failed", err.to_string()))?;
        if !output.status.success() {
            return Err(RunnerError::new(
                "oci_warm_failed",
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn list_workcells(&self) -> RunnerResult<Vec<WarmContainer>> {
        if !Self::gate_open() {
            return Ok(Vec::new());
        }
        let label = crate::lifecycle::WORKCELL_LABEL;
        let output = Command::new(Self::engine())
            .args([
                "ps",
                "--all",
                "--filter",
                &format!("label={label}"),
                "--format",
                &format!("{{{{.ID}}}}\t{{{{.Label \"{label}\"}}}}"),
            ])
            .stdin(Stdio::null())
            .output()
            .map_err(|err| RunnerError::new("oci_list_failed", err.to_string()))?;
        if !output.status.success() {
            return Err(RunnerError::new(
                "oci_list_failed",
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        let listing = String::from_utf8_lossy(&output.stdout);
        let mut found = Vec::new();
        for line in listing.lines() {
            let mut cols = line.splitn(2, '\t');
            let (Some(id), Some(workcell)) = (cols.next(), cols.next()) else {
                continue;
            };
            let (id, workcell) = (id.trim(), workcell.trim());
            if !id.is_empty() && !workcell.is_empty() {
                found.push(WarmContainer::new(id, workcell));
            }
        }
        Ok(found)
    }

    fn remove(&self, container_id: &str) -> RunnerResult<()> {
        if !Self::gate_open() {
            return Ok(());
        }
        let output = Command::new(Self::engine())
            .args(["rm", "--force", container_id])
            .stdin(Stdio::null())
            .output()
            .map_err(|err| RunnerError::new("oci_remove_failed", err.to_string()))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(RunnerError::new(
                "oci_remove_failed",
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ))
        }
    }
}

/// Fake runtime that records the argv it would run and never touches a daemon.
///
/// It consumes the exact same [`OciSpec::args`] builder as the real runtime, so
/// the recorded invocation cannot drift from what would really execute.
#[derive(Debug, Default)]
pub struct FakeContainerRuntime {
    exit_code: i32,
    invocations: Mutex<Vec<Vec<String>>>,
    lifecycle_ops: Mutex<Vec<crate::lifecycle::LifecycleOp>>,
    warm: Mutex<Vec<WarmContainer>>,
    next_cid: AtomicU64,
}

impl FakeContainerRuntime {
    /// Fake that reports the given exit code on every run.
    pub fn with_exit_code(exit_code: i32) -> Self {
        Self {
            exit_code,
            ..Self::default()
        }
    }

    /// Snapshot of every recorded argv (executable followed by `args()`).
    pub fn recorded(&self) -> Vec<Vec<String>> {
        self.invocations
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    /// Snapshot of every recorded lifecycle operation, in order.
    pub fn lifecycle_ops(&self) -> Vec<crate::lifecycle::LifecycleOp> {
        self.lifecycle_ops
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    /// Live (not-yet-removed) warm containers the fake currently tracks.
    pub fn live_warm(&self) -> Vec<WarmContainer> {
        self.warm
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }
}

impl ContainerLifecycle for FakeContainerRuntime {
    fn start_warm(&self, workcell_id: &str) -> RunnerResult<String> {
        let serial = self.next_cid.fetch_add(1, Ordering::SeqCst) + 1;
        let container_id = format!("fake-ctr-{serial:04}");
        let spec = WarmContainerSpec::new(
            self.name(),
            "agent-sandbox:fake",
            "none",
            workcell_id.to_string(),
        );
        self.warm
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(WarmContainer::new(&container_id, workcell_id));
        self.lifecycle_ops
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(crate::lifecycle::LifecycleOp::StartWarm {
                workcell_id: workcell_id.to_string(),
                container_id: container_id.clone(),
                argv: spec.argv(),
            });
        Ok(container_id)
    }

    fn list_workcells(&self) -> RunnerResult<Vec<WarmContainer>> {
        self.lifecycle_ops
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(crate::lifecycle::LifecycleOp::List);
        Ok(self
            .warm
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone())
    }

    fn remove(&self, container_id: &str) -> RunnerResult<()> {
        self.warm
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .retain(|c| c.container_id != container_id);
        self.lifecycle_ops
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(crate::lifecycle::LifecycleOp::Remove {
                container_id: container_id.to_string(),
            });
        Ok(())
    }
}

impl ContainerRuntime for FakeContainerRuntime {
    fn name(&self) -> &str {
        "fake"
    }

    fn run(&self, spec: &OciSpec) -> RunnerResult<RuntimeOutcome> {
        let mut argv = vec![spec.runtime.clone()];
        argv.extend(spec.args());
        self.invocations
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(argv);
        Ok(RuntimeOutcome {
            exit_code: Some(self.exit_code),
            ran: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OciRunner;
    use jeryu_runner_core::job::{JobRequest, NetworkPolicy, SecretPolicy, TokenPolicy};
    use jeryu_runner_core::policy::select_runner;
    use jeryu_runner_core::receipt::ReceiptStatus;
    use jeryu_runner_core::sandbox::SandboxPlan;
    use jeryu_runner_core::trust::{RunnerClass, TrustTier};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn sample_job() -> JobRequest {
        JobRequest {
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
        }
    }

    /// The fake records exactly the argv the real runtime would consume, so the
    /// two cannot drift apart on the shared `OciSpec::args` builder.
    #[test]
    fn fake_records_same_argv_as_spec() {
        let job = sample_job();
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);
        let spec = OciSpec::from_job(&job, &plan).unwrap_or_else(|err| panic!("{err}"));

        let fake = Arc::new(FakeContainerRuntime::default());
        let runner = OciRunner::with_runtime(fake.clone());
        runner
            .execute(&job, &decision, &plan)
            .unwrap_or_else(|err| panic!("{err}"));

        let mut expected = vec![spec.runtime.clone()];
        expected.extend(spec.args());
        assert_eq!(fake.recorded(), vec![expected]);
    }

    /// An injected runtime is the opt-in: the fake-backed runner executes and
    /// reports `Passed` with exit 0 without needing `JERYU_RUN_OCI`.
    #[test]
    fn fake_backed_execute_passes_without_gate() {
        let job = sample_job();
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);

        let runner = OciRunner::with_runtime(Arc::new(FakeContainerRuntime::default()));
        let receipt = runner
            .execute(&job, &decision, &plan)
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(receipt.status, ReceiptStatus::Passed);
        assert_eq!(receipt.exit_code, Some(0));
    }

    /// A non-zero fake exit code maps to a `Failed` receipt.
    #[test]
    fn fake_backed_execute_maps_failure() {
        let job = sample_job();
        let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);

        let runner = OciRunner::with_runtime(Arc::new(FakeContainerRuntime::with_exit_code(7)));
        let receipt = runner
            .execute(&job, &decision, &plan)
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(receipt.status, ReceiptStatus::Failed);
        assert_eq!(receipt.exit_code, Some(7));
    }
}
