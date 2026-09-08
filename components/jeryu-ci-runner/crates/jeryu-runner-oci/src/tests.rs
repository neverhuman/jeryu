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
    // OCI-compat lane is NOT hardened (keeps the original loose args).
    assert!(spec.hardening.is_none());
    assert!(!spec.args().contains(&"--read-only".to_string()));
}

fn oci_agent_job() -> JobRequest {
    JobRequest {
        job_id: "agent".to_string(),
        repo_id: "jeryu/jeryu".to_string(),
        commit_sha: "abc".to_string(),
        workspace: PathBuf::from("/tmp/agent-ws"),
        command: "/opt/jeryu/bin/codex".to_string(),
        args: vec!["exec".to_string()],
        env: Default::default(),
        trust_tier: TrustTier::T4ForkPr,
        requested_runner: Some(RunnerClass::OciDocker),
        network_policy: NetworkPolicy::Deny,
        secret_policy: SecretPolicy::None,
        token_policy: TokenPolicy::None,
        timeout_ms: 1000,
        fork: true,
    }
}

#[test]
fn agent_hardened_spec_locks_down_the_container() {
    let job = oci_agent_job();
    let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
    let plan = SandboxPlan::from_decision(&job.workspace, &decision);
    let spec = OciSpec::from_agent_job(&job, &plan).unwrap_or_else(|err| panic!("{err}"));
    assert!(spec.hardening.is_some());
    assert_eq!(spec.network, "none");
    let args = spec.args();
    assert!(args.contains(&"--read-only".to_string()), "args: {args:?}");
    assert!(args.contains(&"--cap-drop=ALL".to_string()));
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--security-opt" && w[1] == "no-new-privileges")
    );
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--security-opt" && w[1].starts_with("seccomp="))
    );
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--user" && w[1] == "1000:1000")
    );
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--network" && w[1] == "none")
    );
    // ONLY the workspace is bind-mounted — no host paths leak in.
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
    assert!(binds[0].ends_with(":/workspace:Z"));
    assert!(!args.iter().any(|a| {
        a.contains("docker.sock") || a.contains("/root") || a == "/usr" || a.contains("/nix")
    }));
}

#[test]
fn agent_hardened_rejects_dangerous_workspace() {
    let mut job = oci_agent_job();
    job.workspace = PathBuf::from("/var/run/docker.sock");
    let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
    let plan = SandboxPlan::from_decision(&job.workspace, &decision);
    assert_eq!(
        OciSpec::from_agent_job(&job, &plan)
            .err()
            .unwrap_or_else(|| panic!("expected host path denial"))
            .code(),
        "host_path_denied"
    );
}

#[test]
fn fake_runtime_records_exactly_the_hardened_args() {
    use crate::runtime::{ContainerRuntime, FakeContainerRuntime};
    let job = oci_agent_job();
    let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
    let plan = SandboxPlan::from_decision(&job.workspace, &decision);
    let spec = OciSpec::from_agent_job(&job, &plan).unwrap_or_else(|err| panic!("{err}"));
    let fake = FakeContainerRuntime::default();
    fake.run(&spec).unwrap_or_else(|err| panic!("{err}"));
    let mut expected = vec![spec.runtime.clone()];
    expected.extend(spec.args());
    assert_eq!(
        fake.recorded(),
        vec![expected],
        "fake must record the EXACT hardened argv (no drift from the real runtime)"
    );
}

#[test]
fn launch_session_runs_the_hardened_container_spec() {
    use crate::runtime::FakeContainerRuntime;
    use crate::session::plan_agent_session;
    let job = oci_agent_job();
    let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
    let plan = SandboxPlan::from_decision(&job.workspace, &decision);
    let session = plan_agent_session(
        "jeryu",
        "jeryu",
        "agent-7",
        "run-42",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        "https://forge.invalid/jeryu/jeryu.git",
        &job,
        &plan,
    )
    .unwrap_or_else(|err| panic!("{err}"));
    let fake = Arc::new(FakeContainerRuntime::default());
    let runner = OciRunner::with_runtime(fake.clone());
    let outcome = runner
        .launch_session(&session)
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(outcome.ran, "injected runtime must execute");
    let recorded = fake.recorded();
    assert_eq!(recorded.len(), 1, "exactly one launch: {recorded:?}");
    let argv = &recorded[0];
    // The launched argv carries the FULL lock-down, not the loose OCI-compat args.
    assert!(argv.contains(&"--read-only".to_string()), "argv: {argv:?}");
    assert!(argv.contains(&"--cap-drop=ALL".to_string()));
    assert!(
        argv.windows(2)
            .any(|w| w[0] == "--network" && w[1] == "bridge")
    );
    let binds: Vec<&String> = argv
        .iter()
        .enumerate()
        .filter(|(_, a)| *a == "-v")
        .map(|(i, _)| &argv[i + 1])
        .collect();
    assert_eq!(binds.len(), 1, "only the workspace is mounted: {binds:?}");
    assert!(binds[0].ends_with(":/workspace:Z"));
}

#[test]
fn live_pty_args_inject_interactive_name_and_keep_hardening() {
    let job = oci_agent_job();
    let decision = select_runner(&job).unwrap_or_else(|err| panic!("{err}"));
    let plan = SandboxPlan::from_decision(&job.workspace, &decision);
    let spec = OciSpec::from_agent_job(&job, &plan).unwrap_or_else(|err| panic!("{err}"));
    let args = spec.live_pty_args("run-9");
    // The live-terminal flags are spliced in right after `run --rm`.
    assert_eq!(args[0], "run");
    assert_eq!(args[1], "--rm");
    assert_eq!(args[2], "-i");
    assert_eq!(args[3], "-t");
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--name" && w[1] == "jeryu-agent-run-9"),
        "args: {args:?}"
    );
    // The full hardening + network-none + workspace mount survive unchanged.
    assert!(args.contains(&"--read-only".to_string()), "args: {args:?}");
    assert!(args.contains(&"--cap-drop=ALL".to_string()));
    assert!(
        args.windows(2)
            .any(|w| w[0] == "--network" && w[1] == "none")
    );
    assert!(
        args.iter()
            .enumerate()
            .filter(|(_, a)| *a == "-v")
            .any(|(i, _)| args[i + 1].ends_with(":/workspace:Z"))
    );
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
