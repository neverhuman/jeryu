//! Route-level coverage for the repo-scoped agent session API.
//!
//! Everything here is deterministic and runs without Docker/Podman: the session
//! container is launched through an injected `FakeContainerRuntime` that records
//! the exact confined argv, and a real bare git repo is materialized on disk so
//! the branch-registration and publish ref moves go through the live ref service.

use std::path::Path;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::HeaderMap;
use axum::response::Response as AxumResponse;
use jeryu_core::{CreateRepositoryRequest, ForgeCore};
use jeryu_runner_oci::{FakeContainerRuntime, OciRunner};
use serde_json::{Value, json};

use super::super::WebState;

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
    let state = Arc::new(WebState::new_with_git_storage_and_runner(
        core,
        storage_root.to_path_buf(),
        OciRunner::with_runtime(fake.clone()),
    ));
    (state, fake)
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
async fn create_launches_hardened_session_on_unique_branch_at_latest_main() {
    let storage = tempfile::tempdir().expect("git storage");
    let core = ForgeCore::new();
    let main_oid = seed_repo(&core, storage.path(), "alice", "jeryu");
    let (state, fake) = fake_state(core, storage.path());

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

    // The launched container is HARDENED — the fake recorded the exact confined argv.
    let recorded = fake.recorded();
    assert_eq!(
        recorded.len(),
        1,
        "exactly one container launch: {recorded:?}"
    );
    let argv = &recorded[0];
    assert!(argv.contains(&"--read-only".to_string()), "argv: {argv:?}");
    assert!(argv.contains(&"--cap-drop=ALL".to_string()));
    assert!(
        argv.windows(2)
            .any(|w| w[0] == "--network" && w[1] == "none")
    );
    // ONLY the workspace is bind-mounted — no host paths leak in.
    let binds: Vec<&String> = argv
        .iter()
        .enumerate()
        .filter(|(_, a)| *a == "-v")
        .map(|(i, _)| &argv[i + 1])
        .collect();
    assert_eq!(
        binds.len(),
        1,
        "exactly one mount (the workspace): {binds:?}"
    );
    assert!(binds[0].ends_with(":/workspace:Z"));

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

    // A rejected plan must never launch a container.
    assert!(
        fake.recorded().is_empty(),
        "no container should launch for a rejected session id"
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

    create_session(
        &state,
        "alice/jeryu",
        json!({ "agent_id": "agent-7", "run_id": "run-42", "runner": "node-7" }),
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
    assert_eq!(row["status"], "running");
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
