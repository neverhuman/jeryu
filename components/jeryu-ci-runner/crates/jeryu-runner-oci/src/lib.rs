#![doc = "OCI compatibility runner for Docker/Podman-style jobs."]

pub mod lifecycle;
pub mod runtime;
pub mod session;

pub use lifecycle::{
    ContainerLifecycle, LifecycleOp, WORKCELL_LABEL, WarmContainer, WarmContainerSpec,
};
pub use runtime::{CliContainerRuntime, ContainerRuntime, FakeContainerRuntime, RuntimeOutcome};
pub use session::{AgentSessionPlan, plan_agent_session};

use jeryu_runner_core::error::{RunnerError, RunnerResult};
use jeryu_runner_core::fscheck::deny_dangerous_host_path;
use jeryu_runner_core::job::{JobRequest, NetworkPolicy};
use jeryu_runner_core::policy::PolicyDecision;
use jeryu_runner_core::receipt::{Receipt, ReceiptStatus, now_ms};
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_core::trust::RunnerClass;
use std::path::Path;
use std::sync::Arc;

/// Lock-down options for an agent container. When present on an [`OciSpec`], the
/// emitted run args confine the container so an untrusted coding agent cannot reach
/// anything beyond its own writable workspace: a read-only root filesystem, all
/// capabilities dropped, no-new-privileges, a seccomp profile, a non-root user,
/// memory/pid caps, and ONLY the workspace bind-mounted (the toolchain lives in the
/// image, so no host paths are exposed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentHardening {
    /// Non-root uid the agent process runs as inside the container.
    pub uid: u32,
    /// Non-root gid.
    pub gid: u32,
    /// Hard memory ceiling in bytes (0 = unset).
    pub memory_max_bytes: u64,
    /// Max process/thread count (0 = unset).
    pub pids_max: u32,
    /// CPU shares (relative weight; min 2).
    pub cpu_shares: u32,
    /// Writable tmpfs mounts (e.g. `/tmp`), each nosuid,nodev,noexec.
    pub tmpfs: Vec<String>,
    /// In-image path to the seccomp profile JSON for the agent.
    pub seccomp_profile_path: String,
}

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
    /// Non-secret container environment, emitted as `-e KEY=VALUE`. Credentials are
    /// NEVER carried here: secrets reach the container only via mount or the broker,
    /// so this vector holds plain, non-sensitive values (e.g. the pinned branch name).
    pub env: Vec<(String, String)>,
    /// Lock-down options for confined agent containers; `None` for the OCI-compat lane.
    pub hardening: Option<AgentHardening>,
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
            env: Vec::new(),
            hardening: None,
        })
    }

    /// Build a LOCKED-DOWN agent container spec. Unlike [`OciSpec::from_job`], this
    /// confines an untrusted coding agent: `--network none`, read-only root, all caps
    /// dropped, no-new-privileges, a seccomp profile, a non-root user, memory/pid caps,
    /// and ONLY the workspace mounted (the Rust/Vite/TS/React toolchain + repo deps +
    /// the agent CLIs live in the image, so no host paths are exposed). The image comes
    /// from `JERYU_AGENT_IMAGE`. Agents must be network-deny here; model egress is via
    /// the separate proxy bridge, never the container.
    pub fn from_agent_job(job: &JobRequest, plan: &SandboxPlan) -> RunnerResult<Self> {
        if plan.runner_class != RunnerClass::OciDocker {
            return Err(RunnerError::new(
                "invalid_oci_runner",
                format!("{} is not oci-docker", plan.runner_class),
            ));
        }
        deny_dangerous_host_path(Path::new(&job.workspace))?;
        if job.network_policy != NetworkPolicy::Deny || plan.network_policy != NetworkPolicy::Deny {
            return Err(RunnerError::new(
                "invalid_agent_network_policy",
                format!(
                    "agent containers require both requested and effective network policy deny; requested={}, effective={}; model egress requires the separate proxy bridge",
                    job.network_policy.as_str(),
                    plan.network_policy.as_str()
                ),
            ));
        }
        let runtime = std::env::var("JERYU_OCI_RUNTIME").unwrap_or_else(|_| "podman".to_string());
        let image = std::env::var("JERYU_AGENT_IMAGE")
            .unwrap_or_else(|_| "localhost/jeryu/agent-sandbox:latest".to_string());
        let mut command = vec![job.command.clone()];
        command.extend(job.args.clone());
        let cg = &plan.cgroup_limits;
        Ok(Self {
            runtime,
            image,
            workspace: job.workspace.display().to_string(),
            command,
            network: "none".to_string(),
            env: Vec::new(),
            hardening: Some(AgentHardening {
                uid: 1000,
                gid: 1000,
                memory_max_bytes: cg.memory_max_bytes,
                pids_max: cg.pids_max,
                cpu_shares: u32::from(cg.cpu_weight).max(2),
                tmpfs: vec!["/tmp".to_string()],
                // The docker daemon reads `--security-opt seccomp=<path>` from the HOST,
                // not from inside the image, so the profile dir must be a host path. It
                // defaults to the in-image location but is overridable via
                // JERYU_AGENT_SECCOMP_DIR for hosts where /opt is not writable.
                seccomp_profile_path: format!(
                    "{}/{}.json",
                    std::env::var("JERYU_AGENT_SECCOMP_DIR")
                        .unwrap_or_else(|_| "/opt/jeryu/seccomp".to_string()),
                    plan.seccomp.name
                ),
            }),
        })
    }

    /// Runtime args without the executable. When [`OciSpec::hardening`] is set, the
    /// lock-down flags are emitted FIRST so an untrusted agent is confined to its
    /// workspace; the OCI-compat lane (`hardening: None`) keeps the original loose args.
    pub fn args(&self) -> Vec<String> {
        let mut args = vec!["run".to_string(), "--rm".to_string()];
        if let Some(h) = &self.hardening {
            args.push("--read-only".to_string());
            for tmpfs in &h.tmpfs {
                args.push("--tmpfs".to_string());
                args.push(format!("{tmpfs}:rw,nosuid,nodev,noexec"));
            }
            args.push("--cap-drop=ALL".to_string());
            args.push("--security-opt".to_string());
            args.push("no-new-privileges".to_string());
            args.push("--security-opt".to_string());
            args.push(format!("seccomp={}", h.seccomp_profile_path));
            args.push("--user".to_string());
            args.push(format!("{}:{}", h.uid, h.gid));
            if h.memory_max_bytes > 0 {
                args.push("--memory".to_string());
                args.push(h.memory_max_bytes.to_string());
            }
            if h.pids_max > 0 {
                args.push("--pids-limit".to_string());
                args.push(h.pids_max.to_string());
            }
            args.push("--cpu-shares".to_string());
            args.push(h.cpu_shares.to_string());
        }
        for (key, value) in &self.env {
            args.push("-e".to_string());
            args.push(format!("{key}={value}"));
        }
        args.push("--network".to_string());
        args.push(self.network.clone());
        args.push("-v".to_string());
        args.push(format!("{}:/workspace:Z", self.workspace));
        args.push("-w".to_string());
        args.push("/workspace".to_string());
        args.push(self.image.clone());
        args.extend(self.command.clone());
        args
    }

    /// Runtime args for a LIVE, PTY-attached agent container run.
    ///
    /// Like [`OciSpec::args`] but tuned for the docker-backed live-terminal path:
    /// it injects `-i` (keep stdin open so the web terminal can forward input into
    /// the container) and a stable `--name jeryu-agent-<run_id>` (so an interrupt
    /// can `docker kill` exactly this container) right after `run --rm`, then keeps
    /// the full hardening + the in-image agent argv unchanged. The runtime
    /// executable itself (docker) is NOT included — the caller prepends it.
    ///
    /// `--network none` is preserved: the agent still streams its banner / first
    /// output (proving the pipeline) without egress. Model egress is a separate
    /// later layer (the proxy bridge), never the container.
    pub fn live_pty_args(&self, run_id: &str) -> Vec<String> {
        let mut args = self.args();
        // args[0] == "run", args[1] == "--rm"; splice the live-terminal flags in
        // right after so they precede the hardening block and the image.
        let insert_at = 2.min(args.len());
        // `-it`: keep stdin open AND allocate a TTY inside the container, so an
        // interactive agent CLI (codex/claude) gets a controlling terminal — the host
        // side is already the PtyAgentDriver's PTY. Without `-t` codex aborts with
        // "stdin is not a terminal".
        let live = vec![
            "-i".to_string(),
            "-t".to_string(),
            "--name".to_string(),
            format!("jeryu-agent-{run_id}"),
        ];
        args.splice(insert_at..insert_at, live);
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

    /// Launch a planned agent session's already-hardened container through the
    /// configured runtime.
    ///
    /// Unlike [`OciRunner::execute`], the spec is NOT rebuilt here: the session
    /// planner produced [`AgentSessionPlan::container`] with the full lock-down
    /// (read-only root, all caps dropped, `--network none`, only the workspace
    /// mounted), so the recorded argv is exactly what runs — no drift, and no way
    /// for the launch path to silently widen the confinement. With the default
    /// [`CliContainerRuntime`] this is plan-only unless `JERYU_RUN_OCI=1`; an
    /// injected runtime (e.g. [`FakeContainerRuntime`]) executes unconditionally.
    pub fn launch_session(&self, plan: &AgentSessionPlan) -> RunnerResult<RuntimeOutcome> {
        self.runtime.run(&plan.container)
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
mod tests;
