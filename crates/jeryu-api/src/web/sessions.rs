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

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_agent_stream::{CONTROL_TOPIC, TTY_TOPIC};
use jeryu_core::CreatePullRequestRequest;
use jeryu_gitd::error::GitdError;
use jeryu_gitd::refs::RefService;
use jeryu_runner_core::job::{JobRequest, NetworkPolicy, SecretPolicy, TokenPolicy};
use jeryu_runner_core::policy::select_runner;
use jeryu_runner_core::receipt::now_ms;
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_core::trust::{RunnerClass, TrustTier};
use jeryu_runner_oci::plan_agent_session;
use jeryu_runnerd::{SessionClaim, StartupSync, WorkcellClaimRequest};
use serde::{Deserialize, Serialize};

use super::WebState;
use super::agent_runs::{
    AgentRunState, RepoAgentRunRow, SessionPublishInfo, SessionRecordInit, origin_base_url,
};
use super::repositories::find_repo;
use super::workcells_support::{TypedError, forge_error, typed_error};

const SESSION_DOCS: &str = "docs/workcell.md#agent-run-control-surface";
const DEFAULT_AGENT_COMMAND: &str = "/opt/jeryu/bin/agent";

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
    let command = request
        .command
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_AGENT_COMMAND.to_string());

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
    let branch = claimed.session.branch;

    state.agent_runs.insert_session(SessionRecordInit {
        run_id: run_id.clone(),
        repo: full_name,
        branch: branch.clone(),
        base_oid: base_oid.clone(),
        runner,
        agent: agent_id,
        program: command,
        args: request.args,
        workspace,
    });

    Ok(CreateSessionResponse {
        session_id: run_id.clone(),
        run_id: run_id.clone(),
        branch,
        base_oid,
        ws_scope: format!("agent_run.{run_id}"),
        tty_topic: TTY_TOPIC.to_string(),
        control_topic: CONTROL_TOPIC.to_string(),
        status_url: format!("/api/v1/agent-runs/{run_id}"),
        events_url: format!("/api/v1/agent-runs/{run_id}/events"),
        control_url: format!("/api/v1/agent-runs/{run_id}/control"),
        publish_url: format!("/api/v1/agent-runs/{run_id}/publish"),
    })
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
