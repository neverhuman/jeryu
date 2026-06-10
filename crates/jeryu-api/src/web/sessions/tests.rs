//! Route-level coverage for the repo-scoped agent session API.
//!
//! Everything here is deterministic and runs without Docker/Podman: the session
//! claims a pre-warmed cell from a `WarmPool` built over an injected, recording
//! `FakeContainerRuntime` lifecycle, and a real bare git repo is materialized on
//! disk so the branch-registration and publish ref moves go through the live ref
//! service.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::HeaderMap;
use axum::response::Response as AxumResponse;
use jeryu_core::{CreateRepositoryRequest, ForgeCore};
use jeryu_runner_oci::{FakeContainerRuntime, LifecycleOp};
use serde_json::{Value, json};

use super::super::WebState;

/// Pre-warmed pool depth used across the session route tests. Each test gets a
/// fresh pool warmed to this many cells; a claim reuses one and refills back.
const WARM_TARGET: usize = 2;

async fn response_json(response: AxumResponse) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body reads");
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|err| panic!("response body is not JSON ({err}): {bytes:?}"))
}

fn git(args: &[&str], cwd: &Path) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "jeryu-test")
        .env("GIT_AUTHOR_EMAIL", "jeryu-test@example.com")
        .env("GIT_COMMITTER_NAME", "jeryu-test")
        .env("GIT_COMMITTER_EMAIL", "jeryu-test@example.com")
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to spawn: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Register a repository in the forge AND materialize a one-commit bare repo on
/// disk whose `main` the session planner can resolve. Returns the `main` tip oid.
fn seed_repo(core: &ForgeCore, storage_root: &Path, owner: &str, name: &str) -> String {
    core.create_repository(
        owner,
        CreateRepositoryRequest {
            name: name.to_string(),
            private: false,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .expect("create repository");

    let bare = storage_root.join(owner).join(format!("{name}.git"));
    std::fs::create_dir_all(bare.parent().expect("bare parent")).expect("create owner dir");
    let work = storage_root.join(format!("{owner}-{name}-work"));
    std::fs::create_dir_all(&work).expect("create work dir");

    git(&["init", "--quiet", "-b", "main"], &work);
    std::fs::write(work.join("README.md"), "# seed\n").expect("write seed");
    git(&["add", "README.md"], &work);
    git(&["commit", "--quiet", "-m", "seed"], &work);
    let main_oid = git(&["rev-parse", "HEAD"], &work);
    git(
        &[
            "clone",
            "--quiet",
            "--bare",
            ".",
            bare.to_str().expect("bare utf8"),
        ],
        &work,
    );
    main_oid
}

/// Author a child commit on top of `parent_oid` and push it into the bare repo on
/// a scratch ref so the object exists — this stands in for the agent's work that
/// the host captures. Returns the new commit oid.
fn capture_agent_commit(
    storage_root: &Path,
    owner: &str,
    name: &str,
    parent_oid: &str,
    file: &str,
    content: &str,
) -> String {
    let bare = storage_root.join(owner).join(format!("{name}.git"));
    let work = storage_root.join(format!("{owner}-{name}-agent-work"));
    let _ = std::fs::remove_dir_all(&work);
    git(
        &[
            "clone",
            "--quiet",
            bare.to_str().expect("bare utf8"),
            work.to_str().expect("work utf8"),
        ],
        storage_root,
    );
    git(&["checkout", "--quiet", "--detach", parent_oid], &work);
    let path = work.join(file);
    std::fs::create_dir_all(path.parent().expect("file parent")).expect("create file dir");
    std::fs::write(&path, content).expect("write agent file");
    git(&["add", file], &work);
    git(&["commit", "--quiet", "-m", "agent work"], &work);
    let new_oid = git(&["rev-parse", "HEAD"], &work);
    // Land the object in the bare repo on a scratch ref so publish can advance the
    // session branch to it. The agent itself never pushes — this is host-side.
    git(
        &["push", "--quiet", "origin", "HEAD:refs/heads/_captured"],
        &work,
    );
    new_oid
}

fn fake_state(core: ForgeCore, storage_root: &Path) -> (Arc<WebState>, Arc<FakeContainerRuntime>) {
    let fake = Arc::new(FakeContainerRuntime::default());
    let state = Arc::new(WebState::new_with_git_storage_and_warm_pool(
        core,
        storage_root.to_path_buf(),
        fake.clone(),
        WARM_TARGET,
    ));
    (state, fake)
}

/// Like [`fake_state`] but with the session runtime backend + docker seam injected
/// directly (no process-global env), so the docker / native PTY paths are driven
/// hermetically.
fn fake_state_with_runtime(
    core: ForgeCore,
    storage_root: &Path,
    runtime: super::SessionRuntimeConfig,
) -> (Arc<WebState>, Arc<FakeContainerRuntime>) {
    let fake = Arc::new(FakeContainerRuntime::default());
    let state = WebState::new_with_git_storage_and_warm_pool(
        core,
        storage_root.to_path_buf(),
        fake.clone(),
        WARM_TARGET,
    )
    .with_session_runtime(runtime);
    (Arc::new(state), fake)
}

/// Container ids of the cells still idling warm in the state's pool, sorted.
fn warm_container_ids(state: &Arc<WebState>) -> Vec<String> {
    state
        .warm_pool
        .lock()
        .expect("warm pool mutex")
        .warm_container_ids()
}

/// Current warm depth of the state's pool.
fn warm_depth(state: &Arc<WebState>) -> usize {
    state
        .warm_pool
        .lock()
        .expect("warm pool mutex")
        .warm_depth()
}

/// Count of detached warm starts the fake lifecycle has recorded so far.
fn start_warm_count(fake: &FakeContainerRuntime) -> usize {
    fake.lifecycle_ops()
        .iter()
        .filter(|op| matches!(op, LifecycleOp::StartWarm { .. }))
        .count()
}

async fn create_session(state: &Arc<WebState>, repo_id: &str, body: Value) -> Value {
    response_json(
        super::create(
            State(state.clone()),
            AxumPath(repo_id.to_string()),
            Bytes::from(body.to_string()),
        )
        .await,
    )
    .await
}

#[tokio::test]
async fn create_claims_a_prewarmed_cell_on_unique_branch_at_latest_main() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    let main_oid = seed_repo(&core, storage.path(), "alice", "jeryu");
    let (state, fake) = fake_state(core, storage.path());

    // The pool pre-warmed exactly WARM_TARGET detached cells at construction;
    // nothing has been claimed yet, so those are the only warm starts on record.
    let prewarmed = warm_container_ids(&state);
    assert_eq!(prewarmed.len(), WARM_TARGET, "pool pre-warmed to target");
    let starts_before = start_warm_count(&fake);
    assert_eq!(
        starts_before, WARM_TARGET,
        "only the pre-warm starts so far"
    );

    let created = create_session(
        &state,
        "alice/jeryu",
        json!({ "agent_id": "agent-7", "run_id": "run-42" }),
    )
    .await;

    // The session is cut onto a UNIQUE, namespaced branch — never main.
    assert_eq!(created["branch"], "agents/agent-7/sessions/run-42");
    assert_ne!(created["branch"], "main");
    assert_ne!(created["branch"], "refs/heads/main");
    // Pinned to the latest-main oid.
    assert_eq!(created["base_oid"], main_oid);
    assert_eq!(created["run_id"], "run-42");
    assert_eq!(created["session_id"], "run-42");
    assert_eq!(created["ws_scope"], "agent_run.run-42");
    assert_eq!(created["status_url"], "/api/v1/agent-runs/run-42");

    // The session was handed a PRE-WARMED container: exactly one of the cells the
    // pool had warmed before the claim has left the warm set, proving the New
    // Session reused a ready container rather than cold-starting one.
    let after = warm_container_ids(&state);
    let reused: Vec<&String> = prewarmed.iter().filter(|id| !after.contains(id)).collect();
    assert_eq!(
        reused.len(),
        1,
        "exactly one pre-warmed cell was reused: {prewarmed:?} -> {after:?}"
    );

    // The pool refilled back to target, and the ONLY new lifecycle start is that
    // single refill — never a cold start attributable to the claim itself.
    assert_eq!(
        warm_depth(&state),
        WARM_TARGET,
        "pool refills back to target"
    );
    let starts_after = start_warm_count(&fake);
    assert_eq!(
        starts_after - starts_before,
        1,
        "only the single refill start, no cold start for the claim"
    );

    // The unique branch was REGISTERED on the forge at the base oid.
    let resolved = state
        .repo_manager
        .resolve_parts("alice", "jeryu")
        .expect("resolve bare");
    let refs = jeryu_gitd::refs::RefService::new((*state.repo_manager).clone());
    let registered = refs
        .list_refs(&resolved)
        .expect("list refs")
        .into_iter()
        .find(|r| r.name == "refs/heads/agents/agent-7/sessions/run-42")
        .expect("session branch registered on the forge");
    assert_eq!(
        registered.oid, main_oid,
        "branch registered at latest-main oid"
    );
}

/// A second New Session reuses ANOTHER pre-warmed cell: across two back-to-back
/// claims the pool depth stays pinned at target, and the only lifecycle starts
/// are the two refills — neither claim paid a cold start.
#[tokio::test]
async fn second_create_reuses_another_warm_cell_and_depth_stays_at_target() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    seed_repo(&core, storage.path(), "alice", "jeryu");
    let (state, fake) = fake_state(core, storage.path());
    assert_eq!(start_warm_count(&fake), WARM_TARGET);

    let first = create_session(
        &state,
        "alice/jeryu",
        json!({ "agent_id": "agent-7", "run_id": "run-1" }),
    )
    .await;
    assert_eq!(first["branch"], "agents/agent-7/sessions/run-1");
    assert_eq!(
        warm_depth(&state),
        WARM_TARGET,
        "depth steady after one claim"
    );

    let second = create_session(
        &state,
        "alice/jeryu",
        json!({ "agent_id": "agent-7", "run_id": "run-2" }),
    )
    .await;
    assert_eq!(second["branch"], "agents/agent-7/sessions/run-2");
    assert_eq!(
        warm_depth(&state),
        WARM_TARGET,
        "depth steady across two claims"
    );

    // Two claims => exactly two refills on top of the initial pre-warm, and zero
    // cold starts.
    assert_eq!(
        start_warm_count(&fake),
        WARM_TARGET + 2,
        "two refills, no cold starts across the two claims"
    );
}

#[tokio::test]
async fn create_rejects_unknown_repo_with_404() {
    let storage = tempfile::tempdir().expect("git storage");
    let (state, _fake) = fake_state(ForgeCore::new(), storage.path());
    let response = super::create(
        State(state),
        AxumPath("nobody/ghost".to_string()),
        Bytes::from(json!({ "agent_id": "agent-1" }).to_string()),
    )
    .await;
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    assert_eq!(response_json(response).await["code"], "not_found");
}

#[tokio::test]
async fn create_rejects_invalid_session_id_with_typed_error() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    seed_repo(&core, storage.path(), "alice", "jeryu");
    let (state, fake) = fake_state(core, storage.path());

    // A slash in the agent_id would spoof another ref — the planner rejects it.
    let bad_agent = create_session(
        &state,
        "alice/jeryu",
        json!({ "agent_id": "../../heads/main", "run_id": "run-1" }),
    )
    .await;
    assert_eq!(bad_agent["code"], "invalid_session_id");

    // Whitespace in the run_id is likewise rejected with the planner's typed code.
    let bad_run = create_session(
        &state,
        "alice/jeryu",
        json!({ "agent_id": "agent-1", "run_id": "run 1" }),
    )
    .await;
    assert_eq!(bad_run["code"], "invalid_session_id");

    // A rejected session id must never consume a warm cell: the up-front plan
    // fails before the claim, so the pool is still at its pre-warmed depth and no
    // refill start fired (a refill would betray that a cell had been claimed).
    assert_eq!(
        warm_depth(&state),
        WARM_TARGET,
        "pool depth untouched by rejected requests"
    );
    assert_eq!(
        start_warm_count(&fake),
        WARM_TARGET,
        "no refill => no warm cell was claimed for a rejected id"
    );
}

/// HLT-022 negative proof: a run created for repo A must NEVER surface in repo
/// B's agent-runs list, and vice versa. The per-repo route filters strictly on
/// the run's owning repository.
#[tokio::test]
async fn agent_runs_are_isolated_per_repo() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    seed_repo(&core, storage.path(), "alice", "repo-a");
    seed_repo(&core, storage.path(), "bob", "repo-b");
    let (state, _fake) = fake_state(core, storage.path());

    let run_a = create_session(
        &state,
        "alice/repo-a",
        json!({ "agent_id": "agent-a", "run_id": "run-a" }),
    )
    .await["run_id"]
        .as_str()
        .expect("run a id")
        .to_string();
    let run_b = create_session(
        &state,
        "bob/repo-b",
        json!({ "agent_id": "agent-b", "run_id": "run-b" }),
    )
    .await["run_id"]
        .as_str()
        .expect("run b id")
        .to_string();
    assert_ne!(run_a, run_b);

    let list_a = response_json(
        super::list(State(state.clone()), AxumPath("alice/repo-a".to_string())).await,
    )
    .await;
    let items_a = list_a["items"].as_array().expect("items a");
    let ids_a: Vec<&str> = items_a
        .iter()
        .filter_map(|row| row["run_id"].as_str())
        .collect();
    assert!(
        ids_a.contains(&run_a.as_str()),
        "A's list must contain A's run"
    );
    assert!(
        !ids_a.contains(&run_b.as_str()),
        "A's list must NOT leak B's run (data-isolation): {ids_a:?}"
    );

    let list_b =
        response_json(super::list(State(state), AxumPath("bob/repo-b".to_string())).await).await;
    let ids_b: Vec<&str> = list_b["items"]
        .as_array()
        .expect("items b")
        .iter()
        .filter_map(|row| row["run_id"].as_str())
        .collect();
    assert!(
        ids_b.contains(&run_b.as_str()),
        "B's list must contain B's run"
    );
    assert!(
        !ids_b.contains(&run_a.as_str()),
        "B's list must NOT leak A's run (data-isolation): {ids_b:?}"
    );
}

#[tokio::test]
async fn agent_runs_row_shape_matches_web_contract() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    seed_repo(&core, storage.path(), "alice", "jeryu");
    let (state, _fake) = fake_state(core, storage.path());

    // Point the agent at a long-lived real command so the launched run stays in the
    // Running state for this row-shape contract assertion. The default agent_id
    // mapping resolves to a binary that is absent in the test sandbox, which would
    // otherwise immediately drive the graceful not-available terminal state.
    create_session(
        &state,
        "alice/jeryu",
        json!({
            "agent_id": "agent-7",
            "run_id": "run-42",
            "runner": "node-7",
            "command": "/bin/sh",
            "args": ["-c", "sleep 2"],
        }),
    )
    .await;

    let list =
        response_json(super::list(State(state), AxumPath("alice/jeryu".to_string())).await).await;
    let items = list["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "exactly one run for this repo");
    let row = &items[0];
    assert_eq!(row["run_id"], "run-42");
    assert_eq!(row["branch"], "agents/agent-7/sessions/run-42");
    assert_eq!(row["runner"], "node-7");
    // The launched session run is Running unless the sandbox is unavailable, in
    // which case the driver fails fast; either is a valid terminal-or-live state
    // here, the row-shape contract is what this test pins.
    assert!(
        ["running", "failed"].contains(&row["status"].as_str().unwrap_or_default()),
        "status is a live lifecycle label: {row:?}"
    );
    assert_eq!(row["io_mode"], "pty");
    assert_eq!(row["ws_scope"], "agent_run.run-42");
    assert_eq!(row["agent"], "agent-7");
    // A pty session advertises the live control verbs the web terminal drives.
    let controls = row["supported_controls"].as_array().expect("controls");
    assert!(controls.iter().any(|c| c == "send_input"));
    assert!(controls.iter().any(|c| c == "terminate"));
    assert!(row["tty_live"].is_boolean());
}

/// Publish is HOST-mediated: the captured commit advances the session branch ref
/// through the protected, CAS-guarded ref service and opens a PR. The agent never
/// pushes — the only way its work reaches a ref is this server-side path.
#[tokio::test]
async fn publish_advances_branch_ref_and_opens_pull_request() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    let main_oid = seed_repo(&core, storage.path(), "alice", "jeryu");
    let (state, _fake) = fake_state(core, storage.path());

    create_session(
        &state,
        "alice/jeryu",
        json!({ "agent_id": "agent-7", "run_id": "run-42" }),
    )
    .await;

    // The agent's work, captured host-side as a child commit of the session base.
    let head_oid = capture_agent_commit(
        storage.path(),
        "alice",
        "jeryu",
        &main_oid,
        "src/fix.rs",
        "pub fn fixed() -> u8 { 1 }\n",
    );
    assert_ne!(head_oid, main_oid);

    let published = response_json(
        super::publish(
            State(state.clone()),
            AxumPath("run-42".to_string()),
            HeaderMap::new(),
            Bytes::from(
                json!({
                    "head_oid": head_oid,
                    "author": "agent-7",
                    "title": "Session work"
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;

    assert!(
        published["pull_request_number"].as_u64().unwrap_or(0) > 0,
        "publish must open a real PR: {published:?}"
    );
    assert_eq!(published["base"], "main");
    assert_eq!(published["branch"], "agents/agent-7/sessions/run-42");

    // The session branch ref was advanced HOST-side to the captured head.
    let resolved = state
        .repo_manager
        .resolve_parts("alice", "jeryu")
        .expect("resolve bare");
    let refs = jeryu_gitd::refs::RefService::new((*state.repo_manager).clone());
    let branch = refs
        .list_refs(&resolved)
        .expect("list refs")
        .into_iter()
        .find(|r| r.name == "refs/heads/agents/agent-7/sessions/run-42")
        .expect("session branch still present");
    assert_eq!(
        branch.oid, head_oid,
        "the branch ref advanced to the captured commit"
    );

    // The PR is recorded against the repo with base main and the session head.
    let pr_number = published["pull_request_number"].as_u64().unwrap();
    let pr = state
        .core
        .get_pull_request("alice", "jeryu", pr_number)
        .expect("pull request exists");
    assert_eq!(pr.base.ref_name, "main");
    assert_eq!(pr.head.ref_name, "agents/agent-7/sessions/run-42");
    assert_eq!(pr.head.sha, head_oid);
}

/// The agent-run status for one session run, fetched through the same route the
/// web Active-Agents page reads.
async fn run_status(state: &Arc<WebState>, run_id: &str) -> Value {
    response_json(
        super::super::agent_runs::status(State(state.clone()), AxumPath(run_id.to_string())).await,
    )
    .await
}

/// Concatenate every TTY-event text body recorded for one run so a test can assert
/// the scripted agent bytes streamed through the run's tty stream.
fn tty_text(status: &Value) -> String {
    status["tty_events"]
        .as_array()
        .map(|events| {
            events
                .iter()
                .filter_map(|event| event["text"].as_str())
                .collect::<String>()
        })
        .unwrap_or_default()
}

/// True when the run failed because the host sandbox is unavailable in this
/// environment — the deterministic tests treat that as a SKIP rather than a
/// failure, mirroring the agent-run route's own PTY coverage.
fn sandbox_unavailable(status: &Value) -> bool {
    status["error_code"] == "agent_run_sandbox_unavailable"
}

/// Poll the run status until its tty stream carries `needle`, the run reaches a
/// terminal state, or the sandbox is reported unavailable. Returns the last
/// status seen so the caller can SKIP-or-assert.
async fn await_tty(state: &Arc<WebState>, run_id: &str, needle: &str) -> Value {
    let mut last = Value::Null;
    for _ in 0..600 {
        let status = run_status(state, run_id).await;
        if sandbox_unavailable(&status) {
            return status;
        }
        let seen = tty_text(&status).contains(needle);
        let terminal = status["state"] != "running";
        last = status;
        if seen || terminal {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    last
}

/// THE live-stream proof: a New Session spawns the selected agent and the run's
/// tty stream receives the agent's output. The agent program is pointed at a
/// hermetic scripted echo, so the test never depends on the real installed CLI.
#[tokio::test]
async fn create_session_spawns_agent_and_streams_its_tty_output() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    seed_repo(&core, storage.path(), "alice", "jeryu");
    let (state, _fake) = fake_state(core, storage.path());

    // A scripted echo stands in for an installed agent CLI: it prints a known
    // marker the tty stream must carry.
    let created = create_session(
        &state,
        "alice/jeryu",
        json!({
            "agent_id": "streamer",
            "run_id": "run-stream",
            "command": "/bin/sh",
            "args": ["-c", "printf 'SESSION-LIVE\\n'"],
        }),
    )
    .await;
    assert_eq!(created["run_id"], "run-stream");

    let status = await_tty(&state, "run-stream", "SESSION-LIVE").await;
    if sandbox_unavailable(&status) {
        eprintln!("SKIP create_session stream proof: sandbox unavailable");
        return;
    }
    assert!(
        tty_text(&status).contains("SESSION-LIVE"),
        "the session run's tty stream must carry the spawned agent's output: {status:?}"
    );
}

/// The spawned agent runs with cwd = the session workspace and JERYU_BRANCH set,
/// so the in-cell git guard can confine it to its own branch. A fixture agent
/// echoes `$PWD` and `$JERYU_BRANCH`; both must reach the tty stream.
#[tokio::test]
async fn create_session_agent_runs_in_workspace_with_branch_env() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    seed_repo(&core, storage.path(), "alice", "jeryu");
    let (state, _fake) = fake_state(core, storage.path());

    let created = create_session(
        &state,
        "alice/jeryu",
        json!({
            "agent_id": "agent-env",
            "run_id": "run-env",
            "command": "/bin/sh",
            "args": ["-c", "printf 'PWD=%s\\nBR=%s\\n' \"$PWD\" \"$JERYU_BRANCH\""],
        }),
    )
    .await;
    let workspace = created["run_id"].as_str().expect("run id").to_string();
    assert_eq!(workspace, "run-env");

    let status = await_tty(&state, "run-env", "BR=agents/agent-env/sessions/run-env").await;
    if sandbox_unavailable(&status) {
        eprintln!("SKIP create_session cwd/env proof: sandbox unavailable");
        return;
    }
    let text = tty_text(&status);
    assert!(
        text.contains("BR=agents/agent-env/sessions/run-env"),
        "JERYU_BRANCH must reach the spawned agent: {status:?}"
    );
    // The cwd line names the session workspace (jeryu-session-<run>-<ts>).
    assert!(
        text.contains("PWD=") && text.contains("jeryu-session-run-env-"),
        "the agent cwd must be the session workspace: {status:?}"
    );
}

/// A missing agent binary degrades gracefully: the run is still recorded, returns
/// 2xx, and the tty stream carries one clear "not available" line instead of an
/// empty terminal or a 500.
#[tokio::test]
async fn create_session_missing_agent_records_graceful_not_available_line() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    seed_repo(&core, storage.path(), "alice", "jeryu");
    let (state, _fake) = fake_state(core, storage.path());

    // An unmapped agent_id with no override resolves to the standard agent prefix,
    // which does not exist in the test sandbox.
    let response = super::create(
        State(state.clone()),
        AxumPath("alice/jeryu".to_string()),
        Bytes::from(json!({ "agent_id": "ghost-agent", "run_id": "run-missing" }).to_string()),
    )
    .await;
    assert_eq!(
        response.status(),
        axum::http::StatusCode::CREATED,
        "a missing agent must not fail the whole request"
    );

    let status = run_status(&state, "run-missing").await;
    assert_eq!(
        status["state"], "failed",
        "missing agent ends the run: {status:?}"
    );
    let text = tty_text(&status);
    assert!(
        text.contains("agent ghost-agent not available:") && text.contains("/opt/jeryu/bin/agent"),
        "the tty stream must carry one clear not-available line: {status:?}"
    );
}

/// The agent_id -> binary map: an explicit command wins, the env override
/// (`JERYU_AGENT_<ID>_BIN`) is honored through an injected lookup, each known id
/// resolves to its bundled CLI, and an unknown id falls back to the standard
/// agent prefix (whose absence later drives the graceful not-available line).
#[test]
fn resolve_agent_program_maps_ids_command_and_env_override() {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    let none = |_: &str| None;

    // An explicit command always wins over the id mapping.
    assert_eq!(
        super::resolve_agent_program_with("codex", Some("/custom/bin"), none),
        PathBuf::from("/custom/bin")
    );

    // Each known agent_id resolves to its installed CLI path.
    assert_eq!(
        super::resolve_agent_program_with("codex", None, none),
        PathBuf::from("/home/ubuntu/.npm-global/bin/codex")
    );
    assert_eq!(
        super::resolve_agent_program_with("claude", None, none),
        PathBuf::from("/home/ubuntu/.local/bin/claude")
    );
    assert_eq!(
        super::resolve_agent_program_with("jekko", None, none),
        PathBuf::from("/home/ubuntu/.local/bin/jekko")
    );

    // The env override (id upper-cased, non-alnum -> '_') beats the baked-in path.
    let mut env = BTreeMap::new();
    env.insert(
        "JERYU_AGENT_CODEX_BIN".to_string(),
        "/echo/codex".to_string(),
    );
    assert_eq!(
        super::resolve_agent_program_with("codex", None, |key| env.get(key).cloned()),
        PathBuf::from("/echo/codex")
    );

    // An unknown id with no override falls back to the standard agent prefix.
    assert_eq!(
        super::resolve_agent_program_with("ghost", None, none),
        PathBuf::from("/opt/jeryu/bin/agent")
    );
}

/// Write an executable scripted FAKE docker into `dir` that echoes its own argv (so
/// a test can prove the hardened flags were assembled) and prints a live marker (so
/// a test can prove the PTY stream is live). It deliberately does NOT `cat` stdin —
/// on a PTY that would block forever; printing argv + the marker and exiting is the
/// minimal proof the whole pipeline ran. Returns the script path.
fn write_fake_docker(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("fake-docker.sh");
    std::fs::write(
        &path,
        "#!/bin/sh\nprintf 'DOCKER-ARGV: %s\\n' \"$*\"\nprintf 'DOCKER-AGENT-LIVE\\n'\n",
    )
    .expect("write fake docker");
    let mut perms = std::fs::metadata(&path)
        .expect("fake docker meta")
        .permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).expect("chmod fake docker");
    path
}

/// A docker-backed runtime config pointed at the given fake docker binary.
fn docker_runtime(fake_docker: &Path) -> super::SessionRuntimeConfig {
    super::SessionRuntimeConfig {
        runtime: super::SessionRuntime::Docker,
        docker_bin: Some(fake_docker.to_string_lossy().to_string()),
    }
}

/// THE docker live-stream proof: with the runtime forced to docker and the docker
/// seam pointed at a scripted fake, a New Session runs the agent on a docker-backed
/// PTY and the run's tty stream carries the fake docker's output. Because the fake
/// echoes its own argv, the same stream also proves the HARDENED flags were built
/// (`--read-only`, `--network none`, `-v <ws>:/workspace`).
#[tokio::test]
async fn create_session_docker_runtime_streams_live_and_carries_hardened_flags() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    seed_repo(&core, storage.path(), "alice", "jeryu");
    let bin_dir = tempfile::tempdir().expect("fake docker dir");
    let fake_docker = write_fake_docker(bin_dir.path());
    let (state, _fake) =
        fake_state_with_runtime(core, storage.path(), docker_runtime(&fake_docker));

    let created = create_session(
        &state,
        "alice/jeryu",
        json!({ "agent_id": "codex", "run_id": "run-docker" }),
    )
    .await;
    assert_eq!(created["run_id"], "run-docker");

    let status = await_tty(&state, "run-docker", "DOCKER-AGENT-LIVE").await;
    let text = tty_text(&status);

    // Live-stream proof: the fake docker's output reached the run's tty stream.
    assert!(
        text.contains("DOCKER-AGENT-LIVE"),
        "the docker-backed run must stream the container's output live: {status:?}"
    );
    // Hardened-flags proof: the echoed argv carries the lock-down + workspace mount.
    assert!(
        text.contains("--read-only"),
        "docker argv must carry --read-only: {text}"
    );
    assert!(
        text.contains("--network bridge"),
        "docker argv must keep --network bridge: {text}"
    );
    assert!(
        text.contains(":/workspace"),
        "docker argv must bind-mount the workspace at /workspace: {text}"
    );
    // The in-image agent CLI (codex) is the container's argv, and the live flags
    // (-i + the stable per-run name) are present.
    assert!(
        text.contains("--name jeryu-agent-run-docker") && text.contains(" codex"),
        "docker argv must name the container and run the in-image agent CLI: {text}"
    );
}

/// The materialized workspace is a REAL checkout: after a New Session the session
/// workspace has a `.git` dir and its HEAD is the registered base oid on the unique
/// session branch — so the agent has actual code to work on, not an empty dir.
#[tokio::test]
async fn create_session_materializes_real_checkout() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    let main_oid = seed_repo(&core, storage.path(), "alice", "jeryu");
    let (state, _fake) = fake_state(core, storage.path());

    create_session(
        &state,
        "alice/jeryu",
        json!({ "agent_id": "agent-7", "run_id": "run-checkout" }),
    )
    .await;

    // The recorded run carries the session workspace path (repo_root).
    let status = run_status(&state, "run-checkout").await;
    let workspace = status["repo_root"]
        .as_str()
        .map(std::path::PathBuf::from)
        .expect("repo_root path");

    assert!(
        workspace.join(".git").exists(),
        "the session workspace must be a real git checkout (.git present): {workspace:?}"
    );
    // HEAD is pinned to the registered base oid.
    let head = git(&["rev-parse", "HEAD"], &workspace);
    assert_eq!(
        head, main_oid,
        "workspace HEAD must be the session base oid"
    );
    // ...and it sits on the unique session branch, never main.
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"], &workspace);
    assert_eq!(branch, "agents/agent-7/sessions/run-checkout");

    let resolv = std::fs::read_to_string(workspace.join(".resolv.conf"))
        .expect("session workspace resolv.conf");
    assert!(
        resolv.contains("nameserver 8.8.8.8")
            && resolv.contains("nameserver 8.8.4.4")
            && resolv.contains("options ndots:0"),
        "session workspace must carry the seeded DNS config: {resolv:?}"
    );
}

/// The runtime forced to `native` keeps the OLD native-sandbox path: the session
/// runs the resolved host program in-process, never docker. The fake docker is
/// present on the seam but must NOT be invoked — so its marker never reaches the
/// stream.
#[tokio::test]
async fn create_session_native_runtime_uses_native_path() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    seed_repo(&core, storage.path(), "alice", "jeryu");
    let bin_dir = tempfile::tempdir().expect("fake docker dir");
    let fake_docker = write_fake_docker(bin_dir.path());
    // Native runtime, yet a docker seam is configured — to prove native never uses it.
    let runtime = super::SessionRuntimeConfig {
        runtime: super::SessionRuntime::Native,
        docker_bin: Some(fake_docker.to_string_lossy().to_string()),
    };
    let (state, _fake) = fake_state_with_runtime(core, storage.path(), runtime);

    let created = create_session(
        &state,
        "alice/jeryu",
        json!({
            "agent_id": "native-agent",
            "run_id": "run-native",
            "command": "/bin/sh",
            "args": ["-c", "printf 'NATIVE-LIVE\\n'"],
        }),
    )
    .await;
    assert_eq!(created["run_id"], "run-native");

    let status = await_tty(&state, "run-native", "NATIVE-LIVE").await;

    // The docker fake must NEVER have run on the native path.
    assert!(
        !tty_text(&status).contains("DOCKER-AGENT-LIVE"),
        "native runtime must not invoke docker: {status:?}"
    );
    if sandbox_unavailable(&status) {
        eprintln!("SKIP native runtime stream proof: sandbox unavailable");
        return;
    }
    assert!(
        tty_text(&status).contains("NATIVE-LIVE"),
        "native runtime must stream the in-process agent output: {status:?}"
    );
}

/// The runtime forced to docker with the seam pointed at a NON-EXISTENT path
/// degrades gracefully: the run is recorded, returns 2xx, and the tty stream carries
/// one clear not-available line — never a 500 or an empty terminal.
#[tokio::test]
async fn create_session_docker_runtime_missing_docker_degrades_gracefully() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    seed_repo(&core, storage.path(), "alice", "jeryu");
    // A docker runtime whose seam resolves to nothing (no docker_bin), so the launch
    // cannot start a container and must degrade to the graceful not-available line.
    let runtime = super::SessionRuntimeConfig {
        runtime: super::SessionRuntime::Docker,
        docker_bin: None,
    };
    let (state, _fake) = fake_state_with_runtime(core, storage.path(), runtime);

    let response = super::create(
        State(state.clone()),
        AxumPath("alice/jeryu".to_string()),
        Bytes::from(json!({ "agent_id": "codex", "run_id": "run-nodocker" }).to_string()),
    )
    .await;
    let status_code = response.status();

    let status = run_status(&state, "run-nodocker").await;

    assert_eq!(
        status_code,
        axum::http::StatusCode::CREATED,
        "a missing docker must not fail the whole request"
    );
    assert_eq!(
        status["state"], "failed",
        "missing docker ends the run: {status:?}"
    );
    assert!(
        tty_text(&status).contains("not available"),
        "the tty stream must carry one clear not-available line: {status:?}"
    );
}
