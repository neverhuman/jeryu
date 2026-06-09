//! Capability-gated real-engine smoke for the hardened agent container.
//!
//! This is the Rust companion to `ops/agent-sandbox/smoke.sh`: it builds the exact
//! [`OciSpec::from_agent_job`] the runner launches and executes it through the real
//! [`CliContainerRuntime`], proving the hardened argv runs on a live engine and that the
//! recorded flags are the locked-down set.
//!
//! It is gated twice so the daemonless CI stays green: it runs ONLY when `JERYU_RUN_OCI=1`
//! AND the configured engine (`JERYU_OCI_RUNTIME`, default `podman`) is on `PATH`. Without
//! both it prints a skip line and returns, so `cargo test -p jeryu-runner-oci` is a no-op
//! pass on hosts with no engine. The dedicated runner that owns this lane sets the gate and
//! builds the image (`JERYU_AGENT_IMAGE`, default `localhost/jeryu/agent-sandbox:smoke`).

use std::path::{Path, PathBuf};

use jeryu_runner_core::job::{JobRequest, NetworkPolicy, SecretPolicy, TokenPolicy};
use jeryu_runner_core::policy::select_runner;
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_core::trust::{RunnerClass, TrustTier};
use jeryu_runner_oci::{CliContainerRuntime, ContainerRuntime, OciSpec};

/// True when the named executable is resolvable on `PATH`.
fn on_path(exe: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(exe).is_file())
}

/// The configured engine, matching the runner's selection logic.
fn configured_runtime() -> String {
    std::env::var("JERYU_OCI_RUNTIME").unwrap_or_else(|_| "podman".to_string())
}

fn agent_job(workspace: &Path) -> JobRequest {
    JobRequest {
        job_id: "real-docker-smoke".to_string(),
        repo_id: "jeryu/jeryu".to_string(),
        commit_sha: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string(),
        workspace: workspace.to_path_buf(),
        command: "cargo".to_string(),
        args: vec!["--version".to_string()],
        env: Default::default(),
        trust_tier: TrustTier::T4ForkPr,
        requested_runner: Some(RunnerClass::OciDocker),
        network_policy: NetworkPolicy::Deny,
        secret_policy: SecretPolicy::None,
        token_policy: TokenPolicy::None,
        timeout_ms: 60_000,
        fork: true,
    }
}

/// The hardened argv that [`OciSpec::args`] emits must carry the full lock-down set; this
/// asserts the flags that confine an untrusted agent to its workspace.
fn assert_hardened(spec: &OciSpec) {
    let args = spec.args();
    assert!(args.contains(&"--read-only".to_string()), "args: {args:?}");
    assert!(
        args.contains(&"--cap-drop=ALL".to_string()),
        "args: {args:?}"
    );
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--security-opt" && w[1] == "no-new-privileges"),
        "args: {args:?}"
    );
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--security-opt" && w[1].starts_with("seccomp=")),
        "args: {args:?}"
    );
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--user" && w[1] == "1000:1000"),
        "args: {args:?}"
    );
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--network" && w[1] == "none"),
        "args: {args:?}"
    );
    // ONLY the workspace is bind-mounted — no host paths leak into the cell.
    let binds: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| *a == "-v")
        .map(|(i, _)| &args[i + 1])
        .collect();
    assert_eq!(
        binds.len(),
        1,
        "exactly one mount (the workspace): {binds:?}"
    );
    assert!(binds[0].ends_with(":/workspace:Z"), "binds: {binds:?}");
}

#[test]
fn hardened_agent_container_runs_on_a_real_engine() {
    if std::env::var("JERYU_RUN_OCI").ok().as_deref() != Some("1") {
        eprintln!(
            "real_docker_smoke: SKIP (set JERYU_RUN_OCI=1 on a runner with a container engine)"
        );
        return;
    }
    let runtime = configured_runtime();
    if !on_path(&runtime) {
        eprintln!("real_docker_smoke: SKIP (engine '{runtime}' not on PATH)");
        return;
    }

    // A real, isolated workspace directory the hardened spec mounts at /workspace.
    let workspace: PathBuf =
        std::env::temp_dir().join(format!("jeryu-real-docker-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&workspace)
        .unwrap_or_else(|err| panic!("create workspace {}: {err}", workspace.display()));
    let _guard = WorkspaceGuard(workspace.clone());

    let job = agent_job(&workspace);
    let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
    let plan = SandboxPlan::from_decision(&job.workspace, &decision);
    let spec = OciSpec::from_agent_job(&job, &plan).unwrap_or_else(|err| panic!("{err}"));

    // The recorded argv carries the hardened flags BEFORE the engine ever runs.
    assert_hardened(&spec);

    // CliContainerRuntime honors JERYU_RUN_OCI=1 and shells out to the real engine.
    let outcome = CliContainerRuntime
        .run(&spec)
        .unwrap_or_else(|err| panic!("engine run failed: {err}"));
    assert!(
        outcome.ran,
        "engine must execute the container when JERYU_RUN_OCI=1"
    );
    assert!(
        outcome.exit_code.is_some(),
        "the container must record an exit code: {outcome:?}"
    );
}

/// Removes the temporary workspace when the test ends, pass or panic.
struct WorkspaceGuard(PathBuf);

impl Drop for WorkspaceGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
