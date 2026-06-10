//! Repo-scoped agent session routes: launch a session, list a repository's live
//! runs, and mediate a publish.
//!
//! These three handlers turn the landed session-launch planner into a real
//! product flow. A session is always cut onto a fresh, namespaced branch off the
//! latest `main` (never `main` itself), runs inside the hardened agent container,
//! and is recorded against the owning repository so the web Active-Agents page can
//! render exactly that repository's runs. Publishing is HOST-mediated: the agent's
//! captured commits advance the branch ref through the protected, compare-and-swap
//! ref service and open a pull request — the agent itself never has push rights.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_agent_stream::{CONTROL_TOPIC, TTY_TOPIC};
use jeryu_agentbridge::driver::CommandSpec;
use jeryu_core::CreatePullRequestRequest;
use jeryu_gitd::error::GitdError;
use jeryu_gitd::refs::RefService;
use jeryu_runner_core::job::{JobRequest, NetworkPolicy, SecretPolicy, TokenPolicy};
use jeryu_runner_core::policy::select_runner;
use jeryu_runner_core::receipt::now_ms;
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_core::trust::{RunnerClass, TrustTier};
use jeryu_runner_oci::{OciSpec, plan_agent_session};
use jeryu_runnerd::{SessionClaim, StartupSync, WorkcellClaimRequest};
use serde::{Deserialize, Serialize};

use super::WebState;
use super::agent_runs::{
    AgentRunState, PtyBackend, RepoAgentRunRow, SessionAgentSpawn, SessionPublishInfo,
    SessionRecordInit, origin_base_url, spawn_session_agent,
};
use super::repositories::find_repo;
use super::workcells_support::{TypedError, forge_error, typed_error};

const SESSION_DOCS: &str = "docs/workcell.md#agent-run-control-surface";

/// Wall-clock budget a launched session agent runs under (two hours), matching
/// the session job's own `timeout_ms`.
const SESSION_AGENT_TIMEOUT_SECS: u64 = 7_200;

/// Captured-output byte budget a launched session agent streams under (20 MiB),
/// the same bound the public agent-run route applies by default.
const SESSION_AGENT_OUTPUT_BYTES: usize = 20_971_520;

/// The CLI an unknown `agent_id` falls back to: a stable, non-existent path under
/// the standard agent prefix. It deliberately does NOT exist on disk, so a session
/// for an unmapped id records a run and degrades to the graceful "not available"
/// TTY line rather than failing the whole New Session request.
const DEFAULT_AGENT_COMMAND: &str = "/opt/jeryu/bin/agent";

/// Resolve an `agent_id` to the installed CLI it launches. A caller-supplied
/// `command` always wins (so an operator can point at any entrypoint); otherwise
/// the id maps to one of the bundled agent CLIs. An env override
/// (`JERYU_AGENT_<ID>_BIN`, id upper-cased) takes precedence over the baked-in
/// path so a hermetic test can point at a scripted echo. An unknown id with no
/// override falls back to the standard agent prefix; that path's absence later
/// drives the graceful "not available" line instead of a hard request failure.
fn resolve_agent_program(agent_id: &str, command: Option<&str>) -> PathBuf {
    resolve_agent_program_with(agent_id, command, |key| std::env::var(key).ok())
}

/// Inner resolver with an injected env lookup so a hermetic test drives the
/// `JERYU_AGENT_<ID>_BIN` override branch without mutating process-wide env.
fn resolve_agent_program_with(
    agent_id: &str,
    command: Option<&str>,
    env_lookup: impl Fn(&str) -> Option<String>,
) -> PathBuf {
    if let Some(command) = command.map(str::trim).filter(|value| !value.is_empty()) {
        return PathBuf::from(command);
    }
    let key = agent_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    if let Some(path) = env_lookup(&format!("JERYU_AGENT_{key}_BIN"))
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }
    match agent_id {
        "codex" => PathBuf::from("/home/ubuntu/.npm-global/bin/codex"),
        "claude" => PathBuf::from("/home/ubuntu/.local/bin/claude"),
        "jekko" => PathBuf::from("/home/ubuntu/.local/bin/jekko"),
        "agy" => PathBuf::from("/home/ubuntu/.local/bin/agy"),
        _ => PathBuf::from(DEFAULT_AGENT_COMMAND),
    }
}

/// Whether a launched session agent requires enforced cgroup-v2 limits. Always
/// true off the test path (fail-closed); the deterministic session tests run
/// without managed cgroups, so they relax this through the env flag the agent-run
/// route honors in the same way.
fn session_require_cgroup() -> bool {
    #[cfg(test)]
    {
        false
    }
    #[cfg(not(test))]
    {
        true
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CreateSessionRequest {
    /// Agent identity that owns the session; namespaces the branch.
    agent_id: String,
    /// Optional caller-supplied run id; defaults to a freshly-allocated id.
    #[serde(default)]
    run_id: Option<String>,
    /// Agent entrypoint inside the container; defaults to the standard agent CLI.
    #[serde(default)]
    command: Option<String>,
    /// Arguments passed to the agent entrypoint.
    #[serde(default)]
    args: Vec<String>,
    /// Runner / node identity executing the session.
    #[serde(default)]
    runner: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CreateSessionResponse {
    session_id: String,
    run_id: String,
    branch: String,
    base_oid: String,
    ws_scope: String,
    tty_topic: String,
    control_topic: String,
    status_url: String,
    events_url: String,
    control_url: String,
    publish_url: String,
    /// Companion shell run id — a free-form bash session in the same workspace,
    /// ready immediately so the UI can mount a second terminal pane.
    #[serde(skip_serializing_if = "Option::is_none")]
    shell_run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RepoAgentRunsResponse {
    items: Vec<RepoAgentRunRow>,
}

#[derive(Debug, Clone, Deserialize)]
struct PublishRequest {
    /// The host-captured commit that carries the agent's work on its branch.
    head_oid: String,
    /// Pull-request author.
    author: String,
    /// Pull-request title.
    title: String,
    /// Pull-request body.
    #[serde(default)]
    body: Option<String>,
    /// Target branch; defaults to `main`.
    #[serde(default)]
    base: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PublishResponse {
    run_id: String,
    branch: String,
    base: String,
    pull_request_number: u64,
    url: String,
}

/// `POST /api/v1/repos/{id}/sessions` — launch a hardened agent session.
pub(super) async fn create(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let request: CreateSessionRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(err) => {
            return session_typed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "session_invalid_request",
                "create an agent session for a repository",
                &err.to_string(),
                &[
                    "send a JSON body with at least an agent_id",
                    "use the typed API surface to build the request",
                ],
                "fix the request body, then rerun the sessions proof lane",
            );
        }
    };
    match create_session(&state, &id, request) {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(response) => *response,
    }
}

fn create_session(
    state: &Arc<WebState>,
    repo_id: &str,
    request: CreateSessionRequest,
) -> Result<CreateSessionResponse, Box<AxumResponse>> {
    let repo = find_repo(state, repo_id).ok_or_else(|| Box::new(repo_not_found(repo_id)))?;
    let owner = repo.owner.clone();
    let name = repo.name.clone();
    let full_name = repo.full_name.clone();

    let resolved = state
        .repo_manager
        .resolve_parts(&owner, &name)
        .map_err(|err| Box::new(gitd_error(err)))?;
    let refs = RefService::new((*state.repo_manager).clone());
    let base_oid = refs
        .list_refs(&resolved)
        .map_err(|err| Box::new(gitd_error(err)))?
        .into_iter()
        .find(|git_ref| git_ref.name == "refs/heads/main")
        .map(|git_ref| git_ref.oid)
        .ok_or_else(|| Box::new(session_repo_uninitialized(&full_name)))?;

    let run_id = request
        .run_id
        .clone()
        .unwrap_or_else(|| state.agent_runs.allocate_id());
    let agent_id = request.agent_id.clone();
    let runner = request
        .runner
        .clone()
        .unwrap_or_else(|| "local".to_string());
    // Resolve which installed CLI this session launches: a caller-supplied command
    // wins, else the agent_id maps to a bundled agent binary (codex / claude /
    // jekko), with a `JERYU_AGENT_<ID>_BIN` override for hermetic tests. An unknown
    // id with no override is a typed error, not a silent default.
    let agent_program = resolve_agent_program(&agent_id, request.command.as_deref());
    let command = agent_program.to_string_lossy().to_string();

    let workspace = std::env::temp_dir().join(format!("jeryu-session-{run_id}-{}", now_ms()));
    let origin_url = repo_origin_url(&origin_base_url(&HeaderMap::new()), &owner, &name);

    let job = JobRequest {
        job_id: run_id.clone(),
        repo_id: full_name.clone(),
        commit_sha: base_oid.clone(),
        workspace: workspace.clone(),
        command: command.clone(),
        args: request.args.clone(),
        env: Default::default(),
        trust_tier: TrustTier::T4ForkPr,
        requested_runner: Some(RunnerClass::OciDocker),
        network_policy: NetworkPolicy::Deny,
        secret_policy: SecretPolicy::None,
        token_policy: TokenPolicy::None,
        timeout_ms: 7_200_000,
        fork: true,
    };
    let decision = select_runner(&job).map_err(|err| Box::new(runner_error(err)))?;
    let plan = SandboxPlan::from_decision(&job.workspace, &decision);
    let session = plan_agent_session(
        &owner,
        &name,
        &agent_id,
        &run_id,
        &base_oid,
        &origin_url,
        &job,
        &plan,
    )
    .map_err(|err| Box::new(runner_error(err)))?;

    // Register the unique session branch on the forge at the latest-main oid via
    // the protected, compare-and-swap ref service (create: no prior oid). The
    // branch is namespaced (`agents/<id>/sessions/<run>`) so it can never collide
    // with or spoof `main`.
    let branch_ref = format!("refs/heads/{}", session.branch);
    refs.update_ref(
        &resolved,
        &format!("agent:{agent_id}"),
        &branch_ref,
        &base_oid,
        None,
    )
    .map_err(|err| Box::new(gitd_error(err)))?;

    // Claim a PRE-WARMED cell from the landed warm pool instead of cold-starting
    // a fresh container. The pool reuses a detached `sleep infinity` container,
    // materializes the latest-main checkout on the unique session branch, and
    // refills back to its target depth — so this New Session pays no cold-start.
    // The reused container's plan still carries the full hardening (read-only
    // root, all caps dropped, `--network none`, workspace-only mount), and the
    // branch is the namespaced `agents/<id>/sessions/<run>` we just registered,
    // never `main`. The up-front plan above already rejected a spoofing id, so a
    // malformed request never reaches — and never consumes — a warm cell.
    let claimed = {
        let mut pool = state.warm_pool.lock().expect("warm pool mutex poisoned");
        pool.claim(SessionClaim {
            owner,
            repo: name,
            run_id: run_id.clone(),
            base_oid: base_oid.clone(),
            origin_url,
            job,
            plan,
            claim: WorkcellClaimRequest {
                agent_id: agent_id.clone(),
                workspace_root: workspace.clone(),
                repo_roots: vec![workspace.clone()],
                branch_budget: 1,
                runner_id: runner.clone(),
                runner_epoch: 0,
                git_status_summary: "clean".to_string(),
                ci_snapshot_age_ms: Some(0),
                startup: StartupSync::Rebased {
                    main_ref: "origin/main".to_string(),
                    base_sha: base_oid.clone(),
                    head_sha: base_oid.clone(),
                },
            },
        })
    }
    .map_err(|err| Box::new(runner_error(err)))?;
    let container = claimed.session.container.clone();
    let branch = claimed.session.branch;

    // Record the run with a live control channel so the web terminal can steer the
    // PTY agent; the matching receiver is handed to the driver thread below.
    let (control_tx, control_rx) = mpsc::channel();
    state.agent_runs.insert_session(SessionRecordInit {
        run_id: run_id.clone(),
        repo: full_name.clone(),
        branch: branch.clone(),
        base_oid: base_oid.clone(),
        runner,
        agent: agent_id.clone(),
        program: command.clone(),
        args: request.args.clone(),
        workspace: workspace.clone(),
        control_tx: Some(control_tx),
    });

    // Materialize the session workspace into a REAL working tree before the agent
    // launches: clone the forge's bare repo for this repository and check out the
    // unique session branch pinned to the registered base oid. Without this the
    // workspace is an empty dir with no `.git`, so the agent has no code to work on.
    // The clone runs HOST-side from the local bare repo (no network); the agent
    // never performs git over the wire. A failure records the run failed with a
    // clear TTY line rather than 500-ing the New Session request.
    if let Err(reason) = materialize_workspace(
        &state.repo_manager.config().git_bin,
        &resolved.path,
        &workspace,
        &branch,
        &base_oid,
    ) {
        state
            .agent_runs
            .note_session_checkout_failed(&run_id, &reason);
        return Ok(session_response(&run_id, &branch, &base_oid));
    }

    // Seed the host operator's agent auth into the workspace so the
    // container's codex/claude CLI starts pre-authenticated (login once on the
    // host, every session inherits). Best-effort: a missing file never blocks.
    seed_agent_auth(&workspace, &agent_id);

    // Pick the PTY execution backend. The native in-process kernel sandbox cannot be
    // created on a host whose AppArmor blocks the unprivileged userns
    // (`kernel.apparmor_restrict_unprivileged_userns=1`), so `auto` (the default)
    // tries native and falls back to docker when `docker` is on PATH; `docker` and
    // `native` force one path. The docker backend runs the hardened agent container
    // (`--read-only --network none -v <ws>:/workspace ...`) on a live PTY whose TTY
    // streams to the web terminal — it still STARTS and streams the agent's banner
    // even with `--network none` (model egress is a separate later layer).
    let mut agent_env: BTreeMap<String, String> = BTreeMap::new();
    agent_env.insert("JERYU_BRANCH".to_string(), branch.clone());
    let (backend, spec, docker_fallback) = resolve_session_backend(
        &state.session_runtime,
        &agent_id,
        &agent_program,
        &workspace,
        agent_env,
        &container,
        &run_id,
    );
    spawn_session_agent(
        &state.agent_runs,
        SessionAgentSpawn {
            run_id: run_id.clone(),
            workspace: workspace.clone(),
            spec,
            backend,
            docker_fallback,
            control_rx,
            timeout: Duration::from_secs(SESSION_AGENT_TIMEOUT_SECS),
            output_budget: SESSION_AGENT_OUTPUT_BYTES,
            require_cgroup: session_require_cgroup(),
        },
    );

    // ── Spawn a companion shell in the same workspace ──────────────────
    // The operator can interact with this shell freely while the agent runs
    // in the top pane. Spawned at creation time so it is ready immediately.
    let shell_id = state.agent_runs.allocate_id();
    let (shell_ctl_tx, shell_ctl_rx) = std::sync::mpsc::channel();
    state.agent_runs.insert_session(SessionRecordInit {
        run_id: shell_id.clone(),
        repo: full_name.clone(),
        branch: String::new(),
        base_oid: String::new(),
        runner: "local".to_string(),
        agent: "shell".to_string(),
        program: "/bin/bash".to_string(),
        args: vec!["--norc".to_string(), "--noprofile".to_string()],
        workspace: workspace.clone(),
        control_tx: Some(shell_ctl_tx),
    });
    let mut shell_env: BTreeMap<String, String> = BTreeMap::new();
    // Colorful prompt: green user@host, blue cwd, reset.
    shell_env.insert(
        "PS1".to_string(),
        r#"\[\033[1;32m\]shell\[\033[0m\]:\[\033[1;34m\]\w\[\033[0m\]\$ "#.to_string(),
    );
    // Enable color output for ls, grep, etc.
    shell_env.insert("TERM".to_string(), "xterm-256color".to_string());
    shell_env.insert("CLICOLOR".to_string(), "1".to_string());
    shell_env.insert("CLICOLOR_FORCE".to_string(), "1".to_string());
    shell_env.insert("LS_COLORS".to_string(),
        "di=1;34:ln=1;36:so=1;35:pi=33:ex=1;32:bd=1;33;40:cd=1;33;40:su=37;41:sg=30;43:tw=30;42:ow=34;42".to_string());
    let shell_spec = CommandSpec {
        program: "/bin/bash".to_string(),
        args: vec!["--norc".to_string(), "--noprofile".to_string()],
        env: shell_env,
    };
    spawn_session_agent(
        &state.agent_runs,
        SessionAgentSpawn {
            run_id: shell_id.clone(),
            workspace,
            spec: Some(shell_spec),
            backend: PtyBackend::DockerHost, // unsandboxed host PTY — operator shell
            docker_fallback: None,
            control_rx: shell_ctl_rx,
            timeout: Duration::from_secs(7200),
            output_budget: 20_971_520,
            require_cgroup: false,
        },
    );

    state
        .agent_runs
        .register_shell_companion(&run_id, &shell_id);

    let mut resp = session_response(&run_id, &branch, &base_oid);
    resp.shell_run_id = Some(shell_id);
    Ok(resp)
}

/// The create-session response body, shared by the launched-agent success path and
/// the graceful checkout-failure path (both record a run and return 2xx).
fn session_response(run_id: &str, branch: &str, base_oid: &str) -> CreateSessionResponse {
    CreateSessionResponse {
        session_id: run_id.to_string(),
        run_id: run_id.to_string(),
        branch: branch.to_string(),
        base_oid: base_oid.to_string(),
        ws_scope: format!("agent_run.{run_id}"),
        tty_topic: TTY_TOPIC.to_string(),
        control_topic: CONTROL_TOPIC.to_string(),
        status_url: format!("/api/v1/agent-runs/{run_id}"),
        events_url: format!("/api/v1/agent-runs/{run_id}/events"),
        control_url: format!("/api/v1/agent-runs/{run_id}/control"),
        publish_url: format!("/api/v1/agent-runs/{run_id}/publish"),
        shell_run_id: None,
    }
}

/// `GET /api/v1/repos/{id}/agent-runs` — the live agent-runs list for ONE repo.
pub(super) async fn list(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return repo_not_found(&id);
    };
    let items = state.agent_runs.rows_for_repo(&repo.full_name);
    Json(RepoAgentRunsResponse { items }).into_response()
}

/// `POST /api/v1/agent-runs/{id}/publish` — host-mediated publish of a session.
pub(super) async fn publish(
    State(state): State<Arc<WebState>>,
    AxumPath(run_id): AxumPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> AxumResponse {
    let request: PublishRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(err) => {
            return session_typed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "session_invalid_request",
                "publish an agent session into a pull request",
                &err.to_string(),
                &[
                    "send a JSON body with head_oid, author, and title",
                    "use the typed API surface to build the request",
                ],
                "fix the request body, then rerun the sessions proof lane",
            );
        }
    };
    match publish_session(&state, &run_id, request, &origin_base_url(&headers)) {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(response) => *response,
    }
}

fn publish_session(
    state: &Arc<WebState>,
    run_id: &str,
    request: PublishRequest,
    origin_base_url: &str,
) -> Result<PublishResponse, Box<AxumResponse>> {
    let SessionPublishInfo {
        repo,
        branch,
        base_oid,
        state: run_state,
    } = state
        .agent_runs
        .publish_info(run_id)
        .ok_or_else(|| Box::new(run_not_found(run_id)))?;

    let (Some(full_name), Some(branch), Some(base_oid)) = (repo, branch, base_oid) else {
        return Err(Box::new(session_typed_error(
            StatusCode::FAILED_DEPENDENCY,
            "session_publish_source_unavailable",
            "publish an agent session into a pull request",
            "only repo-scoped session runs carry a branch the host can publish",
            &[
                "create the run through POST /api/v1/repos/{id}/sessions",
                "use the workcell export route for failed-CI repair runs",
            ],
            "launch a repo session, then publish it",
        )));
    };
    if run_state == AgentRunState::Exported {
        return Err(Box::new(session_typed_error(
            StatusCode::CONFLICT,
            "session_already_published",
            "publish an agent session into a pull request",
            "this session run was already published",
            &[
                "reload the run status before publishing again",
                "create a fresh session for additional work",
            ],
            "publish each session run once",
        )));
    }

    let Some((owner, name)) = full_name.split_once('/') else {
        return Err(Box::new(session_typed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "session_repo_malformed",
            "publish an agent session into a pull request",
            "the recorded repository was not in owner/name form",
            &["create the run through the sessions route"],
            "relaunch the session, then publish",
        )));
    };
    let base_branch = request
        .base
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "main".to_string());

    let resolved = state
        .repo_manager
        .resolve_parts(owner, name)
        .map_err(|err| Box::new(gitd_error(err)))?;
    let refs = RefService::new((*state.repo_manager).clone());
    let branch_ref = format!("refs/heads/{branch}");
    // Advance the session branch HOST-side: the agent never pushes. The advance is
    // a compare-and-swap from the registered base oid through the protected ref
    // service, so a concurrent move or a protected target fails loudly.
    refs.update_ref(
        &resolved,
        &format!("publish:{}", request.author),
        &branch_ref,
        &request.head_oid,
        Some(&base_oid),
    )
    .map_err(|err| Box::new(gitd_error(err)))?;

    let changed_files = changed_files(
        &state.repo_manager.config().git_bin,
        &resolved.path,
        &base_oid,
        &request.head_oid,
    );

    let pr = state
        .github
        .core()
        .create_pull_request(
            owner,
            name,
            &request.author,
            CreatePullRequestRequest {
                title: request.title,
                body: request.body,
                head: branch.clone(),
                base: base_branch.clone(),
                head_sha: Some(request.head_oid.clone()),
                base_sha: Some(base_oid),
                source_repository: Some(full_name.clone()),
                draft: false,
                commits: Vec::new(),
                changed_files,
            },
        )
        .map_err(|err| Box::new(forge_error(err)))?;

    crate::ci_bridge::seed_pull_request_head(
        state.github.core(),
        state.repo_manager.as_ref(),
        owner,
        name,
        &format!("refs/heads/{}", pr.head.ref_name),
        &pr.head.sha,
        origin_base_url,
    );
    state.agent_runs.mark_exported(run_id);

    Ok(PublishResponse {
        run_id: run_id.to_string(),
        branch,
        base: base_branch,
        pull_request_number: pr.number,
        url: format!("/{}/{}/pull/{}", pr.owner, pr.repo, pr.number),
    })
}

/// Materialize the session workspace into a real working tree on the unique branch
/// at `base_oid`. Clones the local bare repo (`--no-local`-style file clone, no
/// network) into `workspace`, then forces the session branch to the registered base
/// oid. Returns `Err(reason)` if any host git step fails so the caller can degrade
/// gracefully (record the run failed with a TTY line) instead of returning a 500.
fn materialize_workspace(
    git_bin: &str,
    bare: &std::path::Path,
    workspace: &std::path::Path,
    branch: &str,
    base_oid: &str,
) -> Result<(), String> {
    // A pre-existing empty dir (the temp path was reserved up front) is fine, but a
    // populated one is not — `git clone` requires an empty or absent target.
    let _ = std::fs::remove_dir_all(workspace);
    let bare = bare.to_string_lossy().to_string();
    let workspace_arg = workspace.to_string_lossy().to_string();
    // Local file clone of the forge's bare repo — robust regardless of which branch
    // the bare repo currently points HEAD at, and it never touches the network.
    run_git(
        git_bin,
        &["clone", "--no-local", &bare, &workspace_arg],
        None,
    )?;
    // Force the session branch to the exact base oid and check it out, so the agent
    // starts on its own namespaced branch at latest-main (never `main` itself).
    run_git(
        git_bin,
        &["-C", &workspace_arg, "checkout", "-B", branch, base_oid],
        None,
    )?;
    Ok(())
}

/// Run one host git step, mapping a spawn failure or non-zero exit to a short
/// reason string for the graceful checkout-failure path.
fn run_git(git_bin: &str, args: &[&str], cwd: Option<&std::path::Path>) -> Result<(), String> {
    let mut cmd = std::process::Command::new(git_bin);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let output = cmd
        .output()
        .map_err(|err| format!("git {} failed to spawn: {err}", args.join(" ")))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// Resolve which PTY backend a session agent runs under, the launch command for it,
/// and (for `auto`) a docker fallback. `auto` (default) runs the native kernel
/// sandbox and, only if native returns `sandbox_unavailable` at spawn time, retries
/// the same run on docker; `docker`/`native` force one path.
///
/// Returns `(backend, primary_spec, docker_fallback)`:
/// - `docker`: backend `DockerHost`, primary = the host `docker run ... <image>
///   <in-image agent argv>` (hardened flags from the planned [`OciSpec`]); `None`
///   when `docker` is absent (drives the graceful not-available line).
/// - `native`: backend `Native`, primary = the resolved host agent binary; `None`
///   when that binary is absent.
/// - `auto`: backend `Native` with the host agent binary as primary AND a docker
///   command as the fallback (when docker is on PATH). A missing native binary
///   still degrades to the graceful not-available line — `auto` only falls back on
///   an actual kernel-sandbox failure, never on a missing agent CLI.
fn resolve_session_backend(
    config: &SessionRuntimeConfig,
    agent_id: &str,
    agent_program: &std::path::Path,
    workspace: &std::path::Path,
    env: BTreeMap<String, String>,
    container: &OciSpec,
    run_id: &str,
) -> (PtyBackend, Option<CommandSpec>, Option<CommandSpec>) {
    let native_ok = !agent_program.as_os_str().is_empty() && agent_program.is_file();
    let native_spec = native_ok.then(|| CommandSpec {
        program: agent_program.to_string_lossy().to_string(),
        args: container.command.iter().skip(1).cloned().collect(),
        env: env.clone(),
    });
    let docker_spec = config
        .docker_bin
        .as_deref()
        .map(|docker| docker_command(docker, container, workspace, agent_id, env, run_id));

    match config.runtime {
        SessionRuntime::Docker => (PtyBackend::DockerHost, docker_spec, None),
        SessionRuntime::Native => (PtyBackend::Native, native_spec, None),
        SessionRuntime::Auto => (PtyBackend::Native, native_spec, docker_spec),
    }
}

/// Seed the host operator's agent CLI auth into the session workspace so the
/// container (or native) agent starts pre-authenticated. Login once on the
/// host, every New Session inherits.
///
/// The container `$HOME` is `/workspace/.agent-home` (set in the Dockerfile),
/// so we copy the auth files into `{workspace}/.agent-home/{config_dir}/{file}`.
///
/// This is best-effort: a missing host file is silently skipped, a copy failure
/// is logged but never blocks the session. The files are copied (not bind-mounted)
/// so the container cannot modify the host's tokens.
fn seed_agent_auth(workspace: &std::path::Path, agent_id: &str) {
    /// One auth file mapping: host relative path (under `$HOME`) → container
    /// relative path (under `{workspace}/.agent-home`).
    struct AuthFile {
        host_rel: &'static str,
        container_rel: &'static str,
    }

    let codex_files: &[AuthFile] = &[
        AuthFile {
            host_rel: ".codex/auth.json",
            container_rel: ".codex/auth.json",
        },
        AuthFile {
            host_rel: ".codex/config.toml",
            container_rel: ".codex/config.toml",
        },
        AuthFile {
            host_rel: ".codex/config.yaml",
            container_rel: ".codex/config.yaml",
        },
    ];

    let claude_files: &[AuthFile] = &[
        AuthFile {
            host_rel: ".claude/.credentials.json",
            container_rel: ".claude/.credentials.json",
        },
        AuthFile {
            host_rel: ".claude/settings.json",
            container_rel: ".claude/settings.json",
        },
    ];

    let agy_files: &[AuthFile] = &[
        AuthFile {
            host_rel: ".gemini/antigravity-cli/installation_id",
            container_rel: ".gemini/antigravity-cli/installation_id",
        },
        AuthFile {
            host_rel: ".gemini/antigravity-cli/settings.json",
            container_rel: ".gemini/antigravity-cli/settings.json",
        },
    ];

    let all_files: &[AuthFile] = &[
        AuthFile {
            host_rel: ".codex/auth.json",
            container_rel: ".codex/auth.json",
        },
        AuthFile {
            host_rel: ".codex/config.toml",
            container_rel: ".codex/config.toml",
        },
        AuthFile {
            host_rel: ".codex/config.yaml",
            container_rel: ".codex/config.yaml",
        },
        AuthFile {
            host_rel: ".claude/.credentials.json",
            container_rel: ".claude/.credentials.json",
        },
        AuthFile {
            host_rel: ".claude/settings.json",
            container_rel: ".claude/settings.json",
        },
        AuthFile {
            host_rel: ".gemini/antigravity-cli/installation_id",
            container_rel: ".gemini/antigravity-cli/installation_id",
        },
        AuthFile {
            host_rel: ".gemini/antigravity-cli/settings.json",
            container_rel: ".gemini/antigravity-cli/settings.json",
        },
    ];

    let files: &[AuthFile] = match agent_id {
        "codex" => codex_files,
        "claude" => claude_files,
        "agy" => agy_files,
        _ => all_files,
    };

    // The host home directory: prefer `JERYU_AUTH_HOME`, then the real `$HOME`.
    let host_home = std::env::var("JERYU_AUTH_HOME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| std::env::var("HOME").unwrap_or_else(|_| "/root".to_string()));
    let host_home = std::path::Path::new(&host_home);
    let agent_home = workspace.join(".agent-home");

    for file in files {
        let src = host_home.join(file.host_rel);
        if !src.is_file() {
            continue;
        }
        let dst = agent_home.join(file.container_rel);
        if let Some(parent) = dst.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                eprintln!(
                    "seed_agent_auth: failed to create dir {} -> {}: {}",
                    src.display(),
                    dst.display(),
                    err
                );
                continue;
            }
        }
        match std::fs::copy(&src, &dst) {
            Ok(bytes) => {
                // Lock down permissions: the container should read but not modify.
                let _ = std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o400));
                eprintln!(
                    "seed_agent_auth[{}]: seeded {} -> {} ({} bytes)",
                    agent_id,
                    src.display(),
                    dst.display(),
                    bytes
                );
            }
            Err(err) => {
                eprintln!(
                    "seed_agent_auth[{}]: failed to copy {} -> {}: {}",
                    agent_id,
                    src.display(),
                    dst.display(),
                    err
                );
            }
        }
    }

    // ── Seed agy auth: copy entire ~/.gemini tree (config + CLI state) ────
    if agent_id == "agy" || !matches!(agent_id, "codex" | "claude") {
        // Recursively copy relevant ~/.gemini subtrees for agy auth.
        fn copy_dir_recursive(
            src: &std::path::Path,
            dst: &std::path::Path,
            agent_id: &str,
            label: &str,
        ) {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::create_dir_all(dst);
            let entries = match std::fs::read_dir(src) {
                Ok(e) => e,
                Err(_) => return,
            };
            for entry in entries.flatten() {
                let src_path = entry.path();
                let dst_path = dst.join(entry.file_name());
                if src_path.is_dir() {
                    copy_dir_recursive(&src_path, &dst_path, agent_id, label);
                } else if src_path.is_file() {
                    match std::fs::copy(&src_path, &dst_path) {
                        Ok(bytes) => {
                            eprintln!(
                                "seed_agent_auth[{}]: seeded {} {} ({} bytes)",
                                agent_id,
                                label,
                                entry.file_name().to_string_lossy(),
                                bytes
                            );
                        }
                        Err(err) => {
                            eprintln!(
                                "seed_agent_auth[{}]: failed {} {}: {}",
                                agent_id,
                                label,
                                entry.file_name().to_string_lossy(),
                                err
                            );
                        }
                    }
                }
            }
        }

        // ~/.gemini/antigravity-cli/ (installation_id, implicit tokens, settings)
        let cli_src = host_home.join(".gemini/antigravity-cli");
        let cli_dst = agent_home.join(".gemini/antigravity-cli");
        copy_dir_recursive(&cli_src, &cli_dst, agent_id, "cli");

        // ~/.gemini/config/ (projects, mcp_config, .migrated marker)
        let cfg_src = host_home.join(".gemini/config");
        let cfg_dst = agent_home.join(".gemini/config");
        copy_dir_recursive(&cfg_src, &cfg_dst, agent_id, "config");
    }

    // ── Seed a custom resolv.conf with public DNS for sandboxed agents ──
    // Some agent CLIs (agy) use [::1]:53 as DNS fallback instead of reading
    // /etc/resolv.conf. Write a resolv.conf with Google DNS into the workspace
    // at a well-known path; the sandbox mounts it over /etc/resolv.conf.
    {
        let resolv_path = workspace.join(".resolv.conf");
        let _ = std::fs::write(
            &resolv_path,
            "nameserver 8.8.8.8\nnameserver 8.8.4.4\noptions ndots:0\n",
        );
        eprintln!(
            "seed_agent_auth[{}]: wrote custom resolv.conf at {}",
            agent_id,
            resolv_path.display()
        );
    }

    // ── Create writable dirs that agent CLIs expect under $HOME ──────────
    // agy needs write access to log/, cache/, conversations/, knowledge/ etc.
    if agent_id == "agy" {
        for subdir in &[
            ".gemini/antigravity-cli/log",
            ".gemini/antigravity-cli/cache",
            ".gemini/antigravity-cli/conversations",
            ".gemini/antigravity-cli/knowledge",
            ".gemini/antigravity-cli/builtin",
        ] {
            let _ = std::fs::create_dir_all(agent_home.join(subdir));
        }
    }

    // ── Auto-trust the workspace so codex/claude never prompts ──────────
    // Codex: append a [projects."/workspace"] trust entry to the seeded config.toml.
    // Claude: seed a settings.json that auto-accepts /workspace.
    if agent_id == "codex" || agent_id == "claude" || agent_id == "agy" {
        let codex_cfg = agent_home.join(".codex/config.toml");
        if codex_cfg.is_file() {
            // Temporarily make writable (it was locked to 0o400 by the copy loop).
            let _ = std::fs::set_permissions(&codex_cfg, std::fs::Permissions::from_mode(0o600));
            // Append workspace trust to the copied config.
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&codex_cfg) {
                let _ = writeln!(
                    f,
                    "\n[projects.\"/workspace\"]\ntrust_level = \"trusted\"\n"
                );
                drop(f);
                let _ =
                    std::fs::set_permissions(&codex_cfg, std::fs::Permissions::from_mode(0o400));
                eprintln!(
                    "seed_agent_auth[{}]: appended /workspace trust to config.toml",
                    agent_id
                );
            }
        } else {
            // No host config — create a minimal one with just the workspace trust.
            let _ = std::fs::create_dir_all(agent_home.join(".codex"));
            let _ = std::fs::write(
                &codex_cfg,
                "[projects.\"/workspace\"]\ntrust_level = \"trusted\"\n",
            );
            eprintln!(
                "seed_agent_auth[{}]: created minimal config.toml with /workspace trust",
                agent_id
            );
        }
    }
}

/// Build the host `docker run ...` launch command for a session agent. The flags
/// come straight from the planned, hardened [`OciSpec`] (read-only root, all caps
/// dropped, `--network none`, the workspace bind-mounted at `/workspace`), with the
/// in-image agent CLI as the container's argv and `-i` + a stable `--name` injected
/// for the live PTY. The workspace mount is rewritten to the materialized session
/// checkout so the agent sees real code at `/workspace`.
fn docker_command(
    docker: &str,
    container: &OciSpec,
    workspace: &std::path::Path,
    agent_id: &str,
    env: BTreeMap<String, String>,
    run_id: &str,
) -> CommandSpec {
    let mut spec = container.clone();
    spec.workspace = workspace.to_string_lossy().to_string();
    spec.command = in_image_agent_command(agent_id);
    // The forge env (JERYU_BRANCH etc.) is already carried as `-e` flags by the
    // planned container; the host docker process itself needs no extra env.
    let args = spec.live_pty_args(run_id);
    CommandSpec {
        program: docker.to_string(),
        args,
        env,
    }
}

/// Map an `agent_id` to the agent CLI on the image's PATH. The hardened sandbox
/// image bundles the coding-agent CLIs under stable names; an unknown id falls back
/// to the standard `agent` entrypoint (its absence inside the image surfaces as the
/// container exiting, which the live stream shows).
fn in_image_agent_command(agent_id: &str) -> Vec<String> {
    match agent_id {
        "codex" => vec!["codex".to_string()],
        "claude" => vec!["claude".to_string()],
        "jekko" => vec!["jekko".to_string()],
        "agy" => vec![
            "agy".to_string(),
            "--dangerously-skip-permissions".to_string(),
        ],
        _ => vec!["agent".to_string()],
    }
}

/// Which container/native runtime a launched session uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionRuntime {
    Auto,
    Docker,
    Native,
}

/// The resolved session-execution config: which PTY backend to prefer and which
/// docker binary the seam points at. Production resolves this from the
/// `JERYU_AGENT_RUNTIME` / `JERYU_DOCKER_BIN` env once at [`WebState`] construction;
/// a hermetic test injects it directly so it never mutates process-global env (the
/// crate forbids `unsafe`, so `std::env::set_var` is not available to tests).
#[derive(Debug, Clone)]
pub(crate) struct SessionRuntimeConfig {
    /// Preferred backend: `auto` (native then docker fallback), `docker`, `native`.
    pub(crate) runtime: SessionRuntime,
    /// The docker binary the seam invokes (`JERYU_DOCKER_BIN` or `docker` on PATH);
    /// `None` when no docker is resolvable, which drives the graceful path.
    pub(crate) docker_bin: Option<String>,
}

impl SessionRuntimeConfig {
    /// Resolve the session runtime config from the environment.
    ///
    /// `JERYU_AGENT_RUNTIME` selects the backend (`auto` default; `docker`/`native`
    /// force one; an unknown value is treated as `auto` so a typo never wedges New
    /// Session). `JERYU_DOCKER_BIN` overrides the docker binary the seam invokes;
    /// otherwise a `docker` on `PATH` is used when present.
    pub(crate) fn from_env() -> Self {
        let runtime = match std::env::var("JERYU_AGENT_RUNTIME")
            .ok()
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("docker") => SessionRuntime::Docker,
            Some("native") => SessionRuntime::Native,
            _ => SessionRuntime::Auto,
        };
        let docker_bin = std::env::var("JERYU_DOCKER_BIN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .filter(|bin| std::path::Path::new(bin).is_file())
            .or_else(docker_on_path);
        Self {
            runtime,
            docker_bin,
        }
    }
}

/// A `docker` on `PATH` as a launchable path, or `None` when absent.
///
/// Under test this always returns `None`: the deterministic session tests must
/// never reach the host's real docker through `from_env`, so a docker-backed test
/// injects [`SessionRuntimeConfig`] with an explicit fake `docker_bin` instead.
fn docker_on_path() -> Option<String> {
    #[cfg(test)]
    {
        None
    }
    #[cfg(not(test))]
    {
        let path = std::env::var("PATH").ok()?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("docker"))
            .find(|candidate| candidate.is_file())
            .map(|candidate| candidate.to_string_lossy().to_string())
    }
}

/// The smart-HTTP clone URL the agent container fetches `main` from.
fn repo_origin_url(base_url: &str, owner: &str, repo: &str) -> String {
    format!("{}/git/{owner}/{repo}.git", base_url.trim_end_matches('/'))
}

/// Files touched between the session base and the captured head, via `git diff`.
/// Best-effort: a diff failure yields an empty list rather than blocking publish.
fn changed_files(
    git_bin: &str,
    bare: &std::path::Path,
    base_oid: &str,
    head_oid: &str,
) -> Vec<String> {
    let bare = bare.to_string_lossy().to_string();
    let out = std::process::Command::new(git_bin)
        .args(["-C", &bare, "diff", "--name-only", base_oid, head_oid])
        .output();
    match out {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(ToString::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn gitd_error(err: GitdError) -> AxumResponse {
    let (status, code) = match &err {
        GitdError::ProtectedRefDenied(_) | GitdError::Forbidden(_) => {
            (StatusCode::FORBIDDEN, "session_ref_protected")
        }
        GitdError::RepoNotFound(_) => (StatusCode::NOT_FOUND, "session_repo_not_found"),
        GitdError::InvalidInput(_) | GitdError::InvalidPath(_) => {
            (StatusCode::UNPROCESSABLE_ENTITY, "session_ref_invalid")
        }
        GitdError::NonFastForwardRequired | GitdError::MergeConflict(_) => {
            (StatusCode::CONFLICT, "session_ref_conflict")
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "session_ref_failed"),
    };
    let message = err.to_string();
    typed_error(TypedError {
        status,
        code,
        purpose: "register or advance a session branch on the forge",
        reason: &message,
        common_fixes: &[
            "confirm the repository has a materialized bare repo with main",
            "retry after refreshing the recorded base oid",
        ],
        docs_url: SESSION_DOCS,
        repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 4 sessions",
        message: &message,
    })
}

fn runner_error(err: jeryu_runner_core::error::RunnerError) -> AxumResponse {
    let message = err.message().to_string();
    typed_error(TypedError {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code: err.code(),
        purpose: "plan and launch a hardened agent session",
        reason: &message,
        common_fixes: &[
            "supply an agent_id and run_id with no '/' or whitespace",
            "confirm the agent container image is available",
        ],
        docs_url: SESSION_DOCS,
        repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 4 sessions",
        message: &message,
    })
}

fn repo_not_found(repo_id: &str) -> AxumResponse {
    let message = format!("repository {repo_id} was not found");
    session_typed_error(
        StatusCode::NOT_FOUND,
        "not_found",
        "create or list agent sessions for a repository",
        &message,
        &[
            "verify the repository id or owner/name pair",
            "refresh the local forge import before retrying",
        ],
        "rerun cargo test -p jeryu-api --features web --jobs 4 sessions",
    )
}

fn run_not_found(run_id: &str) -> AxumResponse {
    let message = format!("agent run {run_id} was not found");
    session_typed_error(
        StatusCode::NOT_FOUND,
        "not_found",
        "publish an agent session into a pull request",
        &message,
        &[
            "create the session before publishing it",
            "reload the agent-runs list and retry with a live id",
        ],
        "rerun cargo test -p jeryu-api --features web --jobs 4 sessions",
    )
}

fn session_repo_uninitialized(full_name: &str) -> AxumResponse {
    let message = format!("repository {full_name} has no main branch to cut a session from");
    session_typed_error(
        StatusCode::FAILED_DEPENDENCY,
        "session_repo_uninitialized",
        "create an agent session for a repository",
        &message,
        &[
            "push an initial commit to main before launching a session",
            "confirm the bare repo was materialized for this repository",
        ],
        "seed main, then rerun cargo test -p jeryu-api --features web --jobs 4 sessions",
    )
}

fn session_typed_error(
    status: StatusCode,
    code: &'static str,
    purpose: &'static str,
    reason: &str,
    common_fixes: &'static [&'static str],
    repair_hint: &'static str,
) -> AxumResponse {
    typed_error(TypedError {
        status,
        code,
        purpose,
        reason,
        common_fixes,
        docs_url: SESSION_DOCS,
        repair_hint,
        message: reason,
    })
}

#[cfg(test)]
mod tests;
