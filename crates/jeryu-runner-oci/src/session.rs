#![doc = "Session-launch planner: materialize an isolated workspace on a fresh unique branch off latest main, then launch the hardened agent container."]

use crate::OciSpec;
use jeryu_runner_core::error::{RunnerError, RunnerResult};
use jeryu_runner_core::job::JobRequest;
use jeryu_runner_core::sandbox::SandboxPlan;
use std::path::PathBuf;

/// A fully-resolved plan for one agent session.
///
/// It pins the session to a freshly-assigned, namespaced branch (never `main`),
/// records the HOST-side git steps that materialize an isolated checkout at the
/// latest `main` oid, and carries the locked-down [`OciSpec`] that runs the agent.
/// The git steps execute BEFORE the container starts; the agent only ever works on
/// the pre-checked-out branch inside its confined workspace.
#[derive(Debug, Clone)]
pub struct AgentSessionPlan {
    /// Repository owner / namespace.
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// Agent identity that owns the session.
    pub agent_id: String,
    /// Unique run identity for this session.
    pub run_id: String,
    /// Freshly-assigned, namespaced branch this session works on (never `main`).
    pub branch: String,
    /// Base ref the branch is cut from.
    pub base_ref: String,
    /// Latest-main commit the branch is pinned to.
    pub base_oid: String,
    /// Isolated workspace directory the branch is checked out into.
    pub workspace: PathBuf,
    /// HOST-side git argv run, in order, before the container starts.
    pub git_steps: Vec<Vec<String>>,
    /// Locked-down agent container spec.
    pub container: OciSpec,
}

/// Reject any session identity that is empty or carries `/` or whitespace, so the
/// branch name cannot be spoofed into another ref (e.g. `refs/heads/main`).
fn validate_session_id(label: &str, value: &str) -> RunnerResult<()> {
    if value.is_empty() || value.contains('/') || value.chars().any(char::is_whitespace) {
        return Err(RunnerError::new(
            "invalid_session_id",
            format!("{label} must be non-empty and contain no '/' or whitespace: {value:?}"),
        ));
    }
    Ok(())
}

/// Compute a session-launch plan: an isolated workspace checked out at the latest
/// `main` oid on a freshly-assigned UNIQUE branch, plus the hardened agent container.
///
/// `base_oid` is the latest-main sha to pin the branch to. `origin_url` is the clone
/// source. The branch is `agents/<agent_id>/sessions/<run_id>` — unique per run and
/// namespaced, so it can never be `main`. The container env pins the in-container git
/// guard to exactly this branch; credentials are NEVER placed in env (mount/broker only).
// The session identity (owner/repo/agent/run), the base oid, the clone source, and
// both the job and sandbox plan are each independent inputs to one launch decision.
#[allow(clippy::too_many_arguments)]
pub fn plan_agent_session(
    owner: &str,
    repo: &str,
    agent_id: &str,
    run_id: &str,
    base_oid: &str,
    origin_url: &str,
    job: &JobRequest,
    plan: &SandboxPlan,
) -> RunnerResult<AgentSessionPlan> {
    validate_session_id("agent_id", agent_id)?;
    validate_session_id("run_id", run_id)?;

    let branch = format!("agents/{agent_id}/sessions/{run_id}");
    let base_ref = "refs/heads/main".to_string();
    let workspace = job.workspace.clone();
    let workspace_arg = workspace.display().to_string();

    let git_steps = vec![
        vec![
            "clone".to_string(),
            "--single-branch".to_string(),
            "--branch".to_string(),
            "main".to_string(),
            origin_url.to_string(),
            workspace_arg.clone(),
        ],
        vec![
            "-C".to_string(),
            workspace_arg,
            "checkout".to_string(),
            "-b".to_string(),
            branch.clone(),
            base_oid.to_string(),
        ],
    ];

    let mut container = OciSpec::from_agent_job(job, plan)?;
    // Sessions need outbound network for model API calls (Anthropic, OpenAI).
    container.network = "bridge".to_string();
    container.env = vec![
        ("JERYU_BRANCH".to_string(), branch.clone()),
        ("JERYU_REAL_GIT".to_string(), "/usr/bin/git".to_string()),
    ];

    Ok(AgentSessionPlan {
        owner: owner.to_string(),
        repo: repo.to_string(),
        agent_id: agent_id.to_string(),
        run_id: run_id.to_string(),
        branch,
        base_ref,
        base_oid: base_oid.to_string(),
        workspace,
        git_steps,
        container,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_runner_core::job::{NetworkPolicy, SecretPolicy, TokenPolicy};
    use jeryu_runner_core::policy::select_runner;
    use jeryu_runner_core::trust::{RunnerClass, TrustTier};
    use std::path::PathBuf;

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

    fn plan_for(job: &JobRequest) -> SandboxPlan {
        let decision = select_runner(job).unwrap_or_else(|err| panic!("{err}"));
        SandboxPlan::from_decision(&job.workspace, &decision)
    }

    fn sample_session() -> AgentSessionPlan {
        let job = oci_agent_job();
        let plan = plan_for(&job);
        plan_agent_session(
            "jeryu",
            "jeryu",
            "agent-7",
            "run-42",
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            "https://forge.invalid/jeryu/jeryu.git",
            &job,
            &plan,
        )
        .unwrap_or_else(|err| panic!("{err}"))
    }

    #[test]
    fn branch_is_unique_namespaced_and_never_main() {
        let session = sample_session();
        assert_eq!(session.branch, "agents/agent-7/sessions/run-42");
        assert_ne!(session.branch, "main");
        assert_ne!(session.branch, "refs/heads/main");
        assert!(session.branch.starts_with("agents/agent-7/sessions/"));
    }

    #[test]
    fn base_ref_is_main_and_oid_is_passed_sha() {
        let session = sample_session();
        assert_eq!(session.base_ref, "refs/heads/main");
        assert_eq!(session.base_oid, "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
    }

    #[test]
    fn git_steps_clone_main_then_branch_pinned_to_base_oid() {
        let session = sample_session();
        // First step clones single-branch main into the workspace.
        let clone = &session.git_steps[0];
        assert_eq!(clone[0], "clone");
        assert!(clone.contains(&"--single-branch".to_string()));
        assert!(
            clone
                .windows(2)
                .any(|w| w[0] == "--branch" && w[1] == "main")
        );
        assert!(clone.contains(&"/tmp/agent-ws".to_string()));
        // Second step creates + checks out the unique branch pinned to base_oid.
        let checkout = &session.git_steps[1];
        assert!(
            checkout
                .windows(3)
                .any(|w| { w[0] == "checkout" && w[1] == "-b" && w[2] == session.branch }),
            "checkout step: {checkout:?}"
        );
        assert_eq!(
            checkout
                .last()
                .unwrap_or_else(|| panic!("empty checkout step")),
            &session.base_oid,
            "branch must be pinned to the latest-main oid"
        );
    }

    #[test]
    fn container_is_hardened_with_the_workspace_mount_and_bridge_network() {
        let session = sample_session();
        let args = session.container.args();
        assert!(args.contains(&"--read-only".to_string()), "args: {args:?}");
        assert!(args.contains(&"--cap-drop=ALL".to_string()));
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--network" && w[1] == "bridge")
        );
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
    }

    #[test]
    fn container_env_pins_the_branch_for_the_git_guard() {
        let session = sample_session();
        assert!(
            session
                .container
                .env
                .iter()
                .any(|(k, v)| k == "JERYU_BRANCH" && v == &session.branch),
            "env: {:?}",
            session.container.env
        );
        let args = session.container.args();
        let expected = format!("JERYU_BRANCH={}", session.branch);
        assert!(
            args.windows(2).any(|w| w[0] == "-e" && w[1] == expected),
            "args must emit -e JERYU_BRANCH=...: {args:?}"
        );
    }

    #[test]
    fn invalid_session_ids_are_rejected() {
        let job = oci_agent_job();
        let plan = plan_for(&job);
        let base = ("jeryu", "jeryu", "oid", "https://forge.invalid/x.git");
        // Empty agent_id.
        let err = plan_agent_session(base.0, base.1, "", "run-1", base.2, base.3, &job, &plan)
            .err()
            .unwrap_or_else(|| panic!("expected rejection"));
        assert_eq!(err.code(), "invalid_session_id");
        // run_id with a slash would spoof another ref.
        let err = plan_agent_session(
            base.0,
            base.1,
            "agent-1",
            "../../heads/main",
            base.2,
            base.3,
            &job,
            &plan,
        )
        .err()
        .unwrap_or_else(|| panic!("expected rejection"));
        assert_eq!(err.code(), "invalid_session_id");
        // Whitespace in agent_id.
        let err = plan_agent_session(
            base.0, base.1, "agent 1", "run-1", base.2, base.3, &job, &plan,
        )
        .err()
        .unwrap_or_else(|| panic!("expected rejection"));
        assert_eq!(err.code(), "invalid_session_id");
    }
}
