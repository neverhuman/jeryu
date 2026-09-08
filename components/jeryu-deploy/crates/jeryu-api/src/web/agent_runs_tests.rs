//! Route-level coverage for `/api/v1/agent-runs`.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Bytes, to_bytes};
use axum::extract::{Path as AxumPath, Query, State};
use axum::response::Response as AxumResponse;
use jeryu_core::ForgeCore;
use jeryu_runnerd::{HoldFailedTreeRequest, StartupSync, WorkcellClaimRequest};
use serde_json::{Value, json};
use tempfile::tempdir;

use super::WebState;

async fn response_json(response: AxumResponse) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    serde_json::from_slice(&bytes).expect("json response")
}

fn write_agent_script(repo_root: &Path, name: &str, body: &str) -> PathBuf {
    let script = repo_root.join(name);
    std::fs::write(&script, body).expect("write agent script");
    let mut perms = std::fs::metadata(&script)
        .expect("script metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).expect("chmod agent script");
    script
}

fn seed_repairing_workcell(
    state: &Arc<WebState>,
    workspace_root: &Path,
    repo_root: &Path,
) -> (String, u64) {
    let claim = WorkcellClaimRequest {
        agent_id: "agent-repair".to_string(),
        workspace_root: workspace_root.to_path_buf(),
        repo_roots: vec![repo_root.to_path_buf()],
        branch_budget: 2,
        runner_id: "runner-a".to_string(),
        runner_epoch: 77,
        git_status_summary: "clean".to_string(),
        ci_snapshot_age_ms: Some(123),
        startup: StartupSync::Rebased {
            main_ref: "refs/heads/main".to_string(),
            base_sha: "base".to_string(),
            head_sha: "head".to_string(),
        },
    };
    let mut manager = state.workcells.lock().expect("workcell manager");
    let held = manager
        .hold_failed_tree(HoldFailedTreeRequest {
            claim,
            ci_run_id: "ci-100".to_string(),
            failed_run_id: "failed-200".to_string(),
            failed_receipt_id: "receipt-300".to_string(),
            failure_log_digest: "sha256:abc".to_string(),
        })
        .expect("hold failed tree");
    let repairing = manager
        .begin_live_repair(&held.workcell_id, held.runner_epoch)
        .expect("begin live repair");
    (repairing.workcell_id, repairing.runner_epoch)
}

fn claim_workcell(
    state: &Arc<WebState>,
    workspace_root: &Path,
    repo_roots: Vec<PathBuf>,
) -> (String, u64) {
    let claim = WorkcellClaimRequest {
        agent_id: "agent-claimed".to_string(),
        workspace_root: workspace_root.to_path_buf(),
        repo_roots,
        branch_budget: 1,
        runner_id: "runner-claimed".to_string(),
        runner_epoch: 19,
        git_status_summary: "clean".to_string(),
        ci_snapshot_age_ms: None,
        startup: StartupSync::Rebased {
            main_ref: "refs/heads/main".to_string(),
            base_sha: "base".to_string(),
            head_sha: "head".to_string(),
        },
    };
    let mut manager = state.workcells.lock().expect("workcell manager");
    let lease = manager.claim(claim).expect("claim workcell");
    (lease.workcell_id, lease.runner_epoch)
}

fn seed_held_workcell_without_repo_roots(
    state: &Arc<WebState>,
    workspace_root: &Path,
) -> (String, u64) {
    let claim = WorkcellClaimRequest {
        agent_id: "agent-empty".to_string(),
        workspace_root: workspace_root.to_path_buf(),
        repo_roots: Vec::new(),
        branch_budget: 1,
        runner_id: "runner-empty".to_string(),
        runner_epoch: 31,
        git_status_summary: "clean".to_string(),
        ci_snapshot_age_ms: Some(9),
        startup: StartupSync::Rebased {
            main_ref: "refs/heads/main".to_string(),
            base_sha: "base".to_string(),
            head_sha: "head".to_string(),
        },
    };
    let mut manager = state.workcells.lock().expect("workcell manager");
    let held = manager
        .hold_failed_tree(HoldFailedTreeRequest {
            claim,
            ci_run_id: "ci-empty".to_string(),
            failed_run_id: "failed-empty".to_string(),
            failed_receipt_id: "receipt-empty".to_string(),
            failure_log_digest: "sha256:empty".to_string(),
        })
        .expect("hold empty workcell");
    (held.workcell_id, held.runner_epoch)
}

async fn wait_for_terminal_status(state: Arc<WebState>, agent_run_id: &str) -> Value {
    for _ in 0..100 {
        let status = response_json(
            super::agent_runs::status(State(state.clone()), AxumPath(agent_run_id.to_string()))
                .await,
        )
        .await;
        if status["state"] != "running" {
            return status;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    response_json(super::agent_runs::status(State(state), AxumPath(agent_run_id.to_string())).await)
        .await
}

#[tokio::test]
async fn agent_runs_unavailable_sources_and_invalid_requests_are_typed() {
    let state = Arc::new(WebState::new(ForgeCore::new()));

    let malformed = response_json(
        super::agent_runs::start(State(state.clone()), Bytes::from_static(b"{")).await,
    )
    .await;
    assert_eq!(malformed["code"], "agent_run_invalid_request");
    assert_eq!(malformed["purpose"], "start an agent run");

    let cases = [
        (
            json!({"source": {"kind": "repo", "repo": "alice/jeryu"}, "program": "agent.sh"}),
            "agent_run_repo_source_unavailable",
        ),
        (
            json!({"source": {"kind": "local_path", "local_path": "/tmp/jeryu"}, "program": "agent.sh"}),
            "agent_run_local_path_unavailable",
        ),
        (
            json!({"source": {"kind": "scratch", "name": "triage"}, "program": "agent.sh"}),
            "agent_run_scratch_unavailable",
        ),
        (
            json!({"source": {"kind": "scratch"}, "program": "agent.sh"}),
            "agent_run_scratch_unavailable",
        ),
    ];
    for (body, code) in cases {
        let response = response_json(
            super::agent_runs::start(State(state.clone()), Bytes::from(body.to_string())).await,
        )
        .await;
        assert_eq!(response["code"], code);
        assert_eq!(
            response["docs_url"],
            "docs/workcell.md#agent-run-control-surface"
        );
        assert!(
            response["common_fixes"]
                .as_array()
                .expect("common fixes")
                .len()
                >= 2
        );
    }
}

#[tokio::test]
async fn agent_runs_workcell_source_denials_are_typed() {
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let temp = tempdir().expect("workspace");
    let repo_root = temp.path().join("repo");
    std::fs::create_dir_all(&repo_root).expect("repo root");
    write_agent_script(&repo_root, "agent.sh", "#!/bin/sh\nprintf ok\n");

    let missing = response_json(
        super::agent_runs::start(
            State(state.clone()),
            Bytes::from(
                json!({
                    "source": {"kind": "workcell", "workcell_id": "wc-missing", "runner_epoch": 1},
                    "program": "agent.sh"
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(missing["code"], "not_found");
    assert_eq!(
        missing["purpose"],
        "start an agent run from a failed-CI workcell"
    );

    let (workcell_id, runner_epoch) = seed_repairing_workcell(&state, temp.path(), &repo_root);
    let stale = response_json(
        super::agent_runs::start(
            State(state.clone()),
            Bytes::from(
                json!({
                    "source": {"kind": "workcell", "workcell_id": workcell_id, "runner_epoch": runner_epoch + 1},
                    "program": "agent.sh"
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(stale["code"], "workcell_epoch_fenced");
    assert!(stale.get("repair_hint").is_some());

    let outside = temp.path().join("outside.sh");
    std::fs::write(&outside, "#!/bin/sh\nprintf no\n").expect("outside script");
    let denied = response_json(
        super::agent_runs::start(
            State(state.clone()),
            Bytes::from(
                json!({
                    "source": {"kind": "workcell", "workcell_id": workcell_id, "runner_epoch": runner_epoch},
                    "program": outside
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(denied["code"], "agent_run_path_denied");
    assert_eq!(
        denied["docs_url"],
        "docs/workcell.md#agent-run-control-surface"
    );

    let missing_program = response_json(
        super::agent_runs::start(
            State(state.clone()),
            Bytes::from(
                json!({
                    "source": {"kind": "workcell", "workcell_id": workcell_id, "runner_epoch": runner_epoch},
                    "program": "missing-agent.sh"
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(missing_program["code"], "agent_run_path_denied");

    let missing_repo = temp.path().join("missing-repo");
    let missing_repo_response = response_json(
        super::agent_runs::start(
            State(state.clone()),
            Bytes::from(
                json!({
                    "source": {"kind": "workcell", "workcell_id": workcell_id, "runner_epoch": runner_epoch},
                    "repo_root": missing_repo,
                    "program": "agent.sh"
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(missing_repo_response["code"], "agent_run_path_denied");

    let (claimed_id, claimed_epoch) = claim_workcell(&state, temp.path(), vec![repo_root.clone()]);
    let state_denied = response_json(
        super::agent_runs::start(
            State(state.clone()),
            Bytes::from(
                json!({
                    "source": {"kind": "workcell", "workcell_id": claimed_id, "runner_epoch": claimed_epoch},
                    "program": "agent.sh"
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(state_denied["code"], "agent_run_workcell_state_denied");

    let (empty_id, empty_epoch) = seed_held_workcell_without_repo_roots(&state, temp.path());
    let empty_slice = response_json(
        super::agent_runs::start(
            State(state),
            Bytes::from(
                json!({
                    "source": {"kind": "workcell", "workcell_id": empty_id, "runner_epoch": empty_epoch},
                    "program": "agent.sh"
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(empty_slice["code"], "agent_run_path_denied");
}

#[tokio::test]
async fn agent_runs_pty_accepts_live_input_control() {
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let temp = tempdir().expect("workspace");
    let repo_root = temp.path().join("repo");
    std::fs::create_dir_all(&repo_root).expect("repo root");
    write_agent_script(
        &repo_root,
        "agent.sh",
        "#!/bin/sh\nread line\nprintf 'GOT:%s\\n' \"$line\"\nprintf 'CI:%s\\n' \"$JERYU_CI_RUN_ID\"\n",
    );
    let (workcell_id, runner_epoch) = seed_repairing_workcell(&state, temp.path(), &repo_root);

    let start = response_json(
        super::agent_runs::start(
            State(state.clone()),
            Bytes::from(
                json!({
                    "source": {"kind": "workcell", "workcell_id": workcell_id, "runner_epoch": runner_epoch},
                    "io_mode": "pty",
                    "repo_root": repo_root,
                    "program": "agent.sh",
                    "budget": {"wall_secs": 10, "output_bytes": 65536},
                    "require_cgroup": false
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    let Some(agent_run_id) = start["agent_run_id"].as_str().map(ToString::to_string) else {
        panic!("start should return an agent_run_id: {start:?}");
    };

    let control = response_json(
        super::agent_runs::control(
            State(state.clone()),
            AxumPath(agent_run_id.clone()),
            Bytes::from(json!({"kind": "send_input", "text": "hello\n"}).to_string()),
        )
        .await,
    )
    .await;
    if control["code"] == "agent_run_finished" {
        let status = response_json(
            super::agent_runs::status(State(state.clone()), AxumPath(agent_run_id.clone())).await,
        )
        .await;
        if status["error_code"] == "agent_run_sandbox_unavailable" {
            eprintln!("SKIP agent_runs pty route: sandbox unavailable");
            return;
        }
        panic!("control closed before delivery: {status:?}");
    }
    assert_eq!(control["accepted"], true);
    assert_eq!(control["command"], "send_input");

    let mut final_status = Value::Null;
    for _ in 0..100 {
        let status = response_json(
            super::agent_runs::status(State(state.clone()), AxumPath(agent_run_id.clone())).await,
        )
        .await;
        if status["error_code"] == "agent_run_sandbox_unavailable" {
            eprintln!("SKIP agent_runs pty route: sandbox unavailable");
            return;
        }
        let saw_output = status["events"]
            .as_array()
            .map(|events| {
                events.iter().any(|event| {
                    event["text"]
                        .as_str()
                        .is_some_and(|text| text.contains("GOT:hello"))
                })
            })
            .unwrap_or(false);
        final_status = status;
        if saw_output && final_status["state"] != "running" {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    assert_eq!(final_status["state"], "succeeded", "{final_status:?}");
    let events = final_status["events"].as_array().expect("events array");
    assert!(
        events.iter().any(|event| event["text"]
            .as_str()
            .is_some_and(|text| text.contains("GOT:hello"))),
        "status should stream PTY output events: {final_status:?}"
    );
    assert!(
        events.iter().any(|event| event["text"]
            .as_str()
            .is_some_and(|text| text.contains("CI:ci-100"))),
        "failed-CI context env should reach the agent: {final_status:?}"
    );
    assert_eq!(final_status["controls"][0]["command"], "send_input");

    let first_page = response_json(
        super::agent_runs::events(
            State(state.clone()),
            AxumPath(agent_run_id.clone()),
            Query(super::agent_runs::AgentRunEventsQuery {
                after_seq: Some(0),
                limit: Some(1),
            }),
        )
        .await,
    )
    .await;
    assert_eq!(first_page["agent_run_id"], agent_run_id);
    assert_eq!(first_page["after_seq"], 0);
    assert_eq!(first_page["events"].as_array().unwrap().len(), 1);
    let cursor = first_page["next_after_seq"].as_u64().unwrap();
    let resumed = response_json(
        super::agent_runs::events(
            State(state),
            AxumPath(agent_run_id),
            Query(super::agent_runs::AgentRunEventsQuery {
                after_seq: Some(cursor),
                limit: Some(50),
            }),
        )
        .await,
    )
    .await;
    assert!(
        resumed["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["seq"].as_u64().unwrap() > cursor)
    );
}

#[tokio::test]
async fn agent_runs_control_denials_and_pipe_mode_are_typed() {
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let temp = tempdir().expect("workspace");
    let repo_root = temp.path().join("repo");
    std::fs::create_dir_all(&repo_root).expect("repo root");
    write_agent_script(
        &repo_root,
        "agent.sh",
        "#!/bin/sh\nprintf 'PROMPT:%s\\n' \"$JERYU_AGENT_PROMPT\"\nprintf 'ERR:%s\\n' \"$JERYU_FAILED_RUN_ID\" >&2\nsleep 1\n",
    );
    let (workcell_id, runner_epoch) = seed_repairing_workcell(&state, temp.path(), &repo_root);

    let unknown_status = response_json(
        super::agent_runs::status(State(state.clone()), AxumPath("ar-missing".to_string())).await,
    )
    .await;
    assert_eq!(unknown_status["code"], "not_found");

    let malformed_control = response_json(
        super::agent_runs::control(
            State(state.clone()),
            AxumPath("ar-missing".to_string()),
            Bytes::from_static(b"{"),
        )
        .await,
    )
    .await;
    assert_eq!(malformed_control["code"], "agent_run_invalid_request");

    let invalid_control = response_json(
        super::agent_runs::control(
            State(state.clone()),
            AxumPath("ar-missing".to_string()),
            Bytes::from(json!({"kind": "send_input"}).to_string()),
        )
        .await,
    )
    .await;
    assert_eq!(invalid_control["code"], "agent_run_invalid_control");

    let unknown_control = response_json(
        super::agent_runs::control(
            State(state.clone()),
            AxumPath("ar-missing".to_string()),
            Bytes::from(json!({"kind": "terminate"}).to_string()),
        )
        .await,
    )
    .await;
    assert_eq!(unknown_control["code"], "not_found");

    let start = response_json(
        super::agent_runs::start(
            State(state.clone()),
            Bytes::from(
                json!({
                    "source": {"kind": "workcell", "workcell_id": workcell_id, "runner_epoch": runner_epoch},
                    "io_mode": "pipe",
                    "repo_root": repo_root,
                    "program": "agent.sh",
                    "prompt": "repair prompt",
                    "budget": {"wall_secs": 10, "output_bytes": 65536},
                    "require_cgroup": false
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    let agent_run_id = start["agent_run_id"].as_str().expect("agent run id");

    let unfinished_export = response_json(
        super::agent_runs::export_pr(
            State(state.clone()),
            AxumPath(agent_run_id.to_string()),
            axum::http::HeaderMap::new(),
            Bytes::from(
                json!({
                    "owner": "alice",
                    "repo": "demo",
                    "author": "agent-repair",
                    "title": "Export agent run"
                })
                .to_string(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(unfinished_export["code"], "agent_run_not_finished");

    let unsupported = response_json(
        super::agent_runs::control(
            State(state.clone()),
            AxumPath(agent_run_id.to_string()),
            Bytes::from(
                json!({"command": {"kind": "resize_pty", "cols": 120, "rows": 40}}).to_string(),
            ),
        )
        .await,
    )
    .await;
    assert_eq!(unsupported["code"], "agent_run_control_unsupported");

    let final_status = wait_for_terminal_status(state.clone(), agent_run_id).await;
    if final_status["error_code"] == "agent_run_sandbox_unavailable" {
        eprintln!("SKIP agent_runs pipe route: sandbox unavailable");
        return;
    }
    assert_eq!(final_status["state"], "succeeded", "{final_status:?}");
    let events = final_status["events"].as_array().expect("events array");
    assert!(
        events.iter().any(|event| event["text"]
            .as_str()
            .is_some_and(|text| text.contains("PROMPT:repair prompt"))),
        "pipe prompt should reach the agent env: {final_status:?}"
    );
    assert!(
        events.iter().any(|event| event["text"]
            .as_str()
            .is_some_and(|text| text.contains("ERR:failed-200"))),
        "stderr should stream into tty events: {final_status:?}"
    );

    let late = response_json(
        super::agent_runs::control(
            State(state),
            AxumPath(agent_run_id.to_string()),
            Bytes::from(json!({"kind": "send_input", "text": "late\n"}).to_string()),
        )
        .await,
    )
    .await;
    assert_eq!(late["code"], "agent_run_finished");
}

/// Reads SSE frames off an open `text/event-stream` body one at a time, so a test
/// can interleave reading replayed frames with publishing fresh live events on the
/// same connection.
struct SseReader {
    stream: axum::body::BodyDataStream,
    buffer: String,
}

impl SseReader {
    fn new(response: AxumResponse) -> Self {
        Self {
            stream: response.into_body().into_data_stream(),
            buffer: String::new(),
        }
    }

    /// The next complete SSE frame (terminated by a blank line). Waits up to five
    /// seconds for more body bytes so a stalled stream fails loudly rather than
    /// hanging the whole suite.
    async fn next_frame(&mut self) -> Option<SseFrame> {
        use futures_util::StreamExt;
        loop {
            if let Some(idx) = self.buffer.find("\n\n") {
                let raw: String = self.buffer.drain(..idx + 2).collect();
                return Some(SseFrame::parse(&raw));
            }
            let chunk = tokio::time::timeout(Duration::from_secs(5), self.stream.next())
                .await
                .ok()??;
            let chunk = chunk.ok()?;
            self.buffer.push_str(&String::from_utf8_lossy(&chunk));
        }
    }
}

/// One parsed SSE frame: an optional `event:` name and the joined `data:` payload.
struct SseFrame {
    event: Option<String>,
    data: String,
}

impl SseFrame {
    fn parse(raw: &str) -> Self {
        let mut event = None;
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
            } else if let Some(rest) = line.strip_prefix("event:") {
                event = Some(rest.strip_prefix(' ').unwrap_or(rest).to_string());
            }
        }
        Self { event, data }
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.data).unwrap_or(Value::Null)
    }
}

async fn open_tty_stream(state: &Arc<WebState>, run_id: &str, after_seq: Option<u64>) -> SseReader {
    let response = super::agent_runs::tty_stream(
        State(state.clone()),
        AxumPath(run_id.to_string()),
        Query(super::agent_runs::AgentTtyStreamQuery { after_seq }),
    )
    .await;
    SseReader::new(response)
}

#[tokio::test]
async fn tty_stream_replays_buffer_then_delivers_live_raw_bytes() {
    use base64::Engine;

    let state = Arc::new(WebState::new(ForgeCore::new()));
    state.agent_runs.seed_test_run("ar-sse-live", 16);
    // A deliberately non-UTF8 byte sequence to prove byte-for-byte fidelity.
    let raw = [0xff_u8, 0x00, 0xfe, b'h', b'i', 0x80];
    state.agent_runs.push_test_tty(
        "ar-sse-live",
        super::agent_runs::test_raw_tty_event("ar-sse-live", 1, b"one"),
    );
    state.agent_runs.push_test_tty(
        "ar-sse-live",
        super::agent_runs::test_raw_tty_event("ar-sse-live", 2, &raw),
    );

    let mut reader = open_tty_stream(&state, "ar-sse-live", Some(0)).await;

    let first = reader.next_frame().await.expect("replayed seq 1");
    assert_eq!(first.event, None, "replay frames are default data events");
    assert_eq!(first.json()["seq"], 1);

    let second = reader.next_frame().await.expect("replayed seq 2");
    assert_eq!(second.json()["seq"], 2);
    let encoded = second.json()["bytes_b64"]
        .as_str()
        .expect("bytes_b64")
        .to_string();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("decode");
    assert_eq!(
        decoded, raw,
        "raw non-UTF8 bytes survive the SSE transport byte-identical"
    );

    // Publish a fresh event AFTER the stream is open; it must arrive live on the
    // same connection through the single append/publish point.
    state.agent_runs.push_test_tty(
        "ar-sse-live",
        super::agent_runs::test_raw_tty_event("ar-sse-live", 3, b"live"),
    );
    let live = reader.next_frame().await.expect("live seq 3");
    assert_eq!(live.event, None);
    assert_eq!(live.json()["seq"], 3);
}

#[tokio::test]
async fn tty_stream_reconnect_cursor_replays_only_newer_events() {
    let state = Arc::new(WebState::new(ForgeCore::new()));
    state.agent_runs.seed_test_run("ar-sse-cursor", 16);
    for seq in 1..=3u64 {
        state.agent_runs.push_test_tty(
            "ar-sse-cursor",
            super::agent_runs::test_raw_tty_event(
                "ar-sse-cursor",
                seq,
                format!("chunk-{seq}").as_bytes(),
            ),
        );
    }

    // Reconnect with a mid-stream cursor: only the event past it should replay.
    let mut reader = open_tty_stream(&state, "ar-sse-cursor", Some(2)).await;
    let frame = reader.next_frame().await.expect("replayed seq 3");
    assert_eq!(frame.event, None);
    assert_eq!(
        frame.json()["seq"],
        3,
        "a reconnect at cursor 2 replays only newer events"
    );
}

#[tokio::test]
async fn tty_stream_signals_resync_when_slow_subscriber_overflows() {
    let state = Arc::new(WebState::new(ForgeCore::new()));
    state.agent_runs.seed_test_run("ar-sse-lag", 16);

    // Open the stream (subscribing now), then flood past the broadcast depth without
    // reading, so the live subscriber overflows and must be told to resync.
    let mut reader = open_tty_stream(&state, "ar-sse-lag", Some(0)).await;
    let flood = super::agent_runs::tty_broadcast_capacity() + 50;
    for seq in 1..=flood as u64 {
        state.agent_runs.push_test_tty(
            "ar-sse-lag",
            super::agent_runs::test_raw_tty_event("ar-sse-lag", seq, b"x"),
        );
    }

    let frame = reader
        .next_frame()
        .await
        .expect("resync frame after overflow");
    assert_eq!(
        frame.event.as_deref(),
        Some("resync"),
        "an overflowed subscriber is handed a resync marker, not an error"
    );
    let oldest = frame.json()["oldest_retained_seq"]
        .as_u64()
        .expect("oldest_retained_seq");
    assert!(
        oldest >= 1,
        "resync carries the ring floor to re-pull from: {oldest}"
    );
}

#[tokio::test]
async fn tty_stream_unknown_run_is_denied_not_found() {
    let state = Arc::new(WebState::new(ForgeCore::new()));
    let response = super::agent_runs::tty_stream(
        State(state),
        AxumPath("ar-not-a-member".to_string()),
        Query(super::agent_runs::AgentTtyStreamQuery { after_seq: None }),
    )
    .await;
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
    let body = response_json(response).await;
    assert_eq!(body["code"], "not_found");
}
