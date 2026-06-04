use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_agent_stream::{
    AgentControlEnvelope, AgentEventBudget, AgentOutputStream, AgentRunStreamKey, AgentTtyEvent,
    CONTROL_TOPIC, TTY_TOPIC,
};
use jeryu_agentbridge::{AgentDriver, AgentEvent, CollectingSink};

use crate::web::WebState;
use crate::web::workcells_support::parse_json_body;

use super::errors::agent_run_not_found;
use super::git::current_head_sha;
use super::preflight::{validate_request, verify_launch_preflight};
use super::source::{PreparedRun, current_env, prepare_run};
use super::state::{AgentRunOutcome, AgentRunPhase, AgentRunSnapshot};
use super::types::{AgentControlRequest, AgentWorkRequest};
use super::{EMPTY_TREE_SHA, now_millis};

pub(in crate::web) async fn start(State(state): State<Arc<WebState>>, body: Bytes) -> AxumResponse {
    start_with_env(state, body, current_env()).await
}

async fn start_with_env(
    state: Arc<WebState>,
    body: Bytes,
    env: BTreeMap<String, String>,
) -> AxumResponse {
    let request: AgentWorkRequest = match parse_json_body(
        &body,
        "start an agent-edit run",
        "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
    ) {
        Ok(request) => request,
        Err(response) => return *response,
    };
    if let Err(response) = validate_request(&request) {
        return *response;
    }

    if let Err(response) = verify_launch_preflight(&request, &env) {
        return *response;
    }

    let prepared = match prepare_run(&state, &request, &env) {
        Ok(prepared) => prepared,
        Err(response) => return *response,
    };

    let snapshot = AgentRunSnapshot {
        agent_run_id: prepared.agent_run_id.clone(),
        workcell_id: prepared.workcell_id.clone(),
        runner_id: prepared.runner_id.clone(),
        runner_epoch: prepared.runner_epoch,
        phase: AgentRunPhase::Starting,
        agent: request.agent.to_string(),
        model: request.model.clone(),
        prompt: request.prompt.clone(),
        source_kind: prepared.source_kind.clone(),
        source_root: prepared.workspace_root.clone(),
        owner: prepared.owner.clone(),
        repo: prepared.repo.clone(),
        base_ref: prepared.base_ref.clone(),
        branch_suffix: request.branch_suffix.clone(),
        allowed_paths: prepared.allowed_paths.clone(),
        base_sha: prepared.base_sha.clone(),
        head_sha: prepared.base_sha.clone(),
        status_url: format!("/api/v1/agent-runs/{}", prepared.agent_run_id),
        control_topic: CONTROL_TOPIC.to_string(),
        tty_topic: TTY_TOPIC.to_string(),
        export_pr_url: format!("/api/v1/agent-runs/{}/export_pr", prepared.agent_run_id),
        events: Vec::new(),
        controls: Vec::new(),
        outcome: None,
        error: None,
        export_pull_request_number: None,
        export_branch: Some(prepared.export_branch.clone()),
    };

    {
        let mut runs = state.agent_runs.lock().expect("agent-run manager lock");
        runs.insert(snapshot.clone());
    }

    let state_for_task = state.clone();
    let prepared_for_task = prepared.clone();
    let request_for_task = request.clone();
    tokio::task::spawn_blocking(move || {
        {
            let mut runs = state_for_task
                .agent_runs
                .lock()
                .expect("agent-run manager lock");
            let _ = runs.update(&prepared_for_task.agent_run_id, |run| {
                run.phase = AgentRunPhase::Running;
            });
        }

        let timeout = Duration::from_secs(request_for_task.budget.wall_secs);
        let output_budget =
            usize::try_from(request_for_task.budget.output_bytes).unwrap_or(usize::MAX);
        let driver = AgentDriver::new(timeout, output_budget);
        let sink = CollectingSink::new();
        let result = driver.run(
            &prepared_for_task.workspace_root,
            &prepared_for_task.command_spec,
            &sink,
        );
        let raw_events = sink.events();

        match result {
            Ok(result) => {
                let outcome = AgentRunOutcome::from(&result);
                let phase = if outcome.succeeded {
                    AgentRunPhase::Exited
                } else if outcome.timed_out || outcome.budget_exceeded {
                    AgentRunPhase::Terminated
                } else {
                    AgentRunPhase::Failed
                };
                let head_sha = current_head_sha(
                    &state_for_task.repo_manager.config().git_bin,
                    &prepared_for_task.workspace_root,
                )
                .unwrap_or_else(|| EMPTY_TREE_SHA.to_string());
                let events = convert_events(
                    &raw_events,
                    &prepared_for_task,
                    request_for_task.budget.wall_secs,
                    request_for_task.budget.output_bytes,
                    &outcome,
                );
                let mut runs = state_for_task
                    .agent_runs
                    .lock()
                    .expect("agent-run manager lock");
                let _ = runs.update(&prepared_for_task.agent_run_id, |run| {
                    run.phase = phase;
                    run.head_sha = head_sha;
                    run.events = events;
                    run.outcome = Some(outcome);
                    run.error = None;
                    run.export_branch = Some(prepared_for_task.export_branch.clone());
                });
            }
            Err(err) => {
                let message = err.to_string();
                let events = convert_events(
                    &raw_events,
                    &prepared_for_task,
                    request_for_task.budget.wall_secs,
                    request_for_task.budget.output_bytes,
                    &AgentRunOutcome {
                        exit_code: None,
                        timed_out: false,
                        budget_exceeded: false,
                        captured_bytes: 0,
                        enforcement_level: "unavailable".to_string(),
                        elapsed_ms: 0,
                        succeeded: false,
                    },
                );
                let mut runs = state_for_task
                    .agent_runs
                    .lock()
                    .expect("agent-run manager lock");
                let _ = runs.update(&prepared_for_task.agent_run_id, |run| {
                    run.phase = AgentRunPhase::Failed;
                    run.head_sha = current_head_sha(
                        &state_for_task.repo_manager.config().git_bin,
                        &prepared_for_task.workspace_root,
                    )
                    .unwrap_or_else(|| EMPTY_TREE_SHA.to_string());
                    run.events = events;
                    run.outcome = None;
                    run.error = Some(message);
                    run.export_branch = Some(prepared_for_task.export_branch.clone());
                });
            }
        }
    });

    Json(snapshot.start_response()).into_response()
}

pub(in crate::web) async fn status(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
) -> AxumResponse {
    let runs = state.agent_runs.lock().expect("agent-run manager lock");
    match runs.get(&agent_run_id) {
        Some(run) => Json(run).into_response(),
        None => agent_run_not_found(&agent_run_id),
    }
}

pub(in crate::web) async fn control(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let request: AgentControlRequest = match parse_json_body(
        &body,
        "control an agent-edit run",
        "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
    ) {
        Ok(request) => request,
        Err(response) => return *response,
    };
    let mut runs = state.agent_runs.lock().expect("agent-run manager lock");
    let Some(updated) = runs.update(&agent_run_id, |run| {
        run.controls.push(AgentControlEnvelope::new(
            agent_run_id.clone(),
            request.command.clone(),
        ));
    }) else {
        return agent_run_not_found(&agent_run_id);
    };
    Json(updated).into_response()
}

fn convert_events(
    events: &[AgentEvent],
    run: &PreparedRun,
    wall_secs: u64,
    output_bytes: u64,
    outcome: &AgentRunOutcome,
) -> Vec<AgentTtyEvent> {
    let key = AgentRunStreamKey {
        repo: Some(format!("{}/{}", run.owner, run.repo)),
        workcell_id: run.workcell_id.clone(),
        agent_run_id: run.agent_run_id.clone(),
        agent: run.agent.clone(),
        model: run.model.clone(),
    };
    let mut used_output_bytes = 0u64;
    let mut out = Vec::with_capacity(events.len().saturating_add(1));
    for (idx, event) in events.iter().enumerate() {
        let seq = u64::try_from(idx).unwrap_or(u64::MAX).saturating_add(1);
        match event {
            AgentEvent::Started { pid } => {
                let mut tty = AgentTtyEvent::text(
                    seq,
                    now_millis(),
                    &key,
                    AgentOutputStream::Event,
                    format!("pid={pid}"),
                );
                tty.budget = Some(AgentEventBudget {
                    wall_secs,
                    output_bytes,
                    used_output_bytes,
                });
                out.push(tty);
            }
            AgentEvent::Stdout(text) => {
                let mut tty = AgentTtyEvent::text(
                    seq,
                    now_millis(),
                    &key,
                    AgentOutputStream::Stdout,
                    text.clone(),
                );
                tty.budget = Some(AgentEventBudget {
                    wall_secs,
                    output_bytes,
                    used_output_bytes,
                });
                out.push(tty);
            }
            AgentEvent::Stderr(text) => {
                let mut tty = AgentTtyEvent::text(
                    seq,
                    now_millis(),
                    &key,
                    AgentOutputStream::Stderr,
                    text.clone(),
                );
                tty.budget = Some(AgentEventBudget {
                    wall_secs,
                    output_bytes,
                    used_output_bytes,
                });
                out.push(tty);
            }
            AgentEvent::Budget { used, .. } => {
                used_output_bytes = u64::try_from(*used).unwrap_or(u64::MAX);
                let mut tty = AgentTtyEvent::text(
                    seq,
                    now_millis(),
                    &key,
                    AgentOutputStream::Event,
                    format!("budget used={used}"),
                );
                tty.budget = Some(AgentEventBudget {
                    wall_secs,
                    output_bytes,
                    used_output_bytes,
                });
                out.push(tty);
            }
            AgentEvent::Finished { exit_code, .. } => {
                let mut tty = AgentTtyEvent::finished(
                    seq,
                    now_millis(),
                    &key,
                    *exit_code,
                    outcome.enforcement_level.clone(),
                );
                tty.budget = Some(AgentEventBudget {
                    wall_secs,
                    output_bytes,
                    used_output_bytes: outcome.captured_bytes as u64,
                });
                out.push(tty);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use axum::body::Bytes;
    use axum::extract::{Path as AxumPath, State};
    use axum::http::StatusCode;
    use jeryu_core::ForgeCore;

    use super::*;
    use crate::web::agent_runs::source::PreparedRun;

    fn prepared_run() -> PreparedRun {
        PreparedRun {
            agent_run_id: "ar-test".to_string(),
            workcell_id: "wc-test".to_string(),
            runner_id: "runner-test".to_string(),
            runner_epoch: 7,
            agent: "codex".to_string(),
            model: "gpt-5.4-mini".to_string(),
            owner: "local".to_string(),
            repo: "demo".to_string(),
            source_kind: "scratch".to_string(),
            workspace_root: PathBuf::from("/tmp/jeryu-agent-lifecycle"),
            base_sha: "base".to_string(),
            allowed_paths: vec!["/tmp/jeryu-agent-lifecycle/src".to_string()],
            command_spec: jeryu_agentbridge::CommandSpec::new("/bin/echo"),
            export_branch: "agents/codex/wc-test/agent-edit".to_string(),
            base_ref: "main".to_string(),
        }
    }

    fn snapshot(run_id: &str) -> AgentRunSnapshot {
        AgentRunSnapshot {
            agent_run_id: run_id.to_string(),
            workcell_id: "wc-test".to_string(),
            runner_id: "runner-test".to_string(),
            runner_epoch: 7,
            phase: AgentRunPhase::Running,
            agent: "codex".to_string(),
            model: "gpt-5.4-mini".to_string(),
            prompt: "fix".to_string(),
            source_kind: "scratch".to_string(),
            source_root: PathBuf::from("/tmp/jeryu-agent-lifecycle"),
            owner: "local".to_string(),
            repo: "demo".to_string(),
            base_ref: "main".to_string(),
            branch_suffix: "agent-edit".to_string(),
            allowed_paths: vec!["/tmp/jeryu-agent-lifecycle/src".to_string()],
            base_sha: "base".to_string(),
            head_sha: "base".to_string(),
            status_url: format!("/api/v1/agent-runs/{run_id}"),
            control_topic: CONTROL_TOPIC.to_string(),
            tty_topic: TTY_TOPIC.to_string(),
            export_pr_url: format!("/api/v1/agent-runs/{run_id}/export_pr"),
            events: Vec::new(),
            controls: Vec::new(),
            outcome: None,
            error: None,
            export_pull_request_number: None,
            export_branch: Some("agents/codex/wc-test/agent-edit".to_string()),
        }
    }

    fn evidenced_env(data_home: &std::path::Path) -> BTreeMap<String, String> {
        let auth_dir = data_home.join("agent-auth/codex");
        std::fs::create_dir_all(&auth_dir).unwrap();
        std::fs::write(auth_dir.join("auth.json"), "{}").unwrap();
        let mut env = BTreeMap::new();
        env.insert(
            "JERYU_AGENT_AUTH_DATA_HOME".to_string(),
            data_home.display().to_string(),
        );
        env.insert(
            "JERYU_AGENT_TOOL_CODEX_PATH".to_string(),
            "/bin/echo".to_string(),
        );
        env.insert(
            "JERYU_AGENT_EGRESS_PROXY".to_string(),
            "127.0.0.1:19090".to_string(),
        );
        env.insert("JERYU_AGENT_NETGUARD_ATTACHED".to_string(), "1".to_string());
        env.insert("JERYU_AGENT_SANDBOX_ENFORCED".to_string(), "1".to_string());
        env
    }

    #[test]
    fn convert_events_assigns_sequence_budget_and_finish_metadata() {
        let run = prepared_run();
        let outcome = AgentRunOutcome {
            exit_code: Some(0),
            timed_out: false,
            budget_exceeded: false,
            captured_bytes: 11,
            enforcement_level: "enforced".to_string(),
            elapsed_ms: 9,
            succeeded: true,
        };
        let events = vec![
            AgentEvent::Started { pid: 42 },
            AgentEvent::Stdout("hello".to_string()),
            AgentEvent::Stderr("warn".to_string()),
            AgentEvent::Budget { used: 5, limit: 10 },
            AgentEvent::Finished {
                exit_code: Some(0),
                timed_out: false,
                budget_exceeded: false,
            },
        ];
        let tty = convert_events(&events, &run, 60, 1024, &outcome);
        assert_eq!(tty.len(), 5);
        assert_eq!(tty[0].seq, 1);
        assert_eq!(tty[4].seq, 5);
        assert_eq!(tty[4].exit_code, Some(0));
        assert_eq!(tty[4].budget.as_ref().unwrap().used_output_bytes, 11);
    }

    #[tokio::test]
    async fn handlers_return_typed_errors_and_record_controls() {
        let git_root = std::env::temp_dir().join(format!(
            "jeryu-agent-lifecycle-git-{}",
            crate::web::agent_runs::now_millis()
        ));
        let state = Arc::new(crate::web::WebState::new_with_git_storage(
            ForgeCore::new(),
            git_root.clone(),
        ));
        let invalid = start(State(state.clone()), Bytes::from_static(b"{}")).await;
        assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let missing = status(State(state.clone()), AxumPath("missing".to_string())).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        {
            let mut runs = state.agent_runs.lock().unwrap();
            runs.insert(snapshot("ar-control"));
        }
        let body = Bytes::from_static(br#"{"command":{"kind":"terminate"}}"#);
        let controlled = control(
            State(state.clone()),
            AxumPath("ar-control".to_string()),
            body,
        )
        .await;
        assert_eq!(controlled.status(), StatusCode::OK);
        let run = state.agent_runs.lock().unwrap().get("ar-control").unwrap();
        assert_eq!(run.controls.len(), 1);
        let _ = std::fs::remove_dir_all(git_root);
    }

    #[tokio::test]
    async fn start_with_evidence_creates_run_and_background_outcome() {
        let stamp = crate::web::agent_runs::now_millis();
        let data_home = std::env::temp_dir().join(format!("jeryu-agent-start-data-{stamp}"));
        let git_root = std::env::temp_dir().join(format!("jeryu-agent-start-git-{stamp}"));
        let state = Arc::new(crate::web::WebState::new_with_git_storage(
            ForgeCore::new(),
            git_root.clone(),
        ));
        let body = Bytes::from_static(
            br#"{
                "source":{"kind":"scratch","name":"start-demo"},
                "agent":"codex",
                "prompt":"print ready",
                "model":"gpt-5.4-mini",
                "base_ref":"main",
                "effort":"low",
                "allowed_paths":["src"],
                "branch_suffix":"agent-edit",
                "budget":{"wall_secs":1,"output_bytes":4096},
                "stream":{"required":false}
            }"#,
        );

        let response = start_with_env(state.clone(), body, evidenced_env(&data_home)).await;
        assert_eq!(response.status(), StatusCode::OK);
        let snapshot = state
            .agent_runs
            .lock()
            .unwrap()
            .first()
            .expect("start inserted a run");
        assert_eq!(snapshot.agent, "codex");
        assert_eq!(snapshot.model, "gpt-5.4-mini");
        assert_eq!(snapshot.source_kind, "scratch");
        assert_eq!(snapshot.owner, "local");
        assert_eq!(snapshot.repo, "start-demo");
        assert!(snapshot.status_url.contains(&snapshot.agent_run_id));

        let _ = std::fs::remove_dir_all(data_home);
        let _ = std::fs::remove_dir_all(git_root);
    }
}
