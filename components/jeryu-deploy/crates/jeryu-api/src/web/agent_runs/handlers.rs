//! HTTP, MCP, and native-driver orchestration for agent runs.

use super::export::export_workcell_agent_run;
use super::*;

pub(in crate::web) async fn start(State(state): State<Arc<WebState>>, body: Bytes) -> AxumResponse {
    let request: AgentRunStartRequest = match parse_agent_body(&body, "start an agent run") {
        Ok(request) => request,
        Err(response) => return *response,
    };
    match start_request(state, request) {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(response) => *response,
    }
}

fn start_request(
    state: Arc<WebState>,
    request: AgentRunStartRequest,
) -> AgentRunResponseResult<AgentRunStartResponse> {
    let resolved = resolve_agent_run_source(&state, &request)?;

    let agent_run_id = state.agent_runs.allocate_id();
    let (control_tx, control_rx) = mpsc::channel::<AgentControl>();
    let control = if request.io_mode == AgentRunIoMode::Pty {
        Some(control_tx)
    } else {
        None
    };
    let spec = CommandSpec {
        program: resolved.program.to_string_lossy().to_string(),
        args: request.args.clone(),
        env: resolved.env,
    };
    let timeout = Duration::from_secs(request.budget.wall_secs.clamp(1, 86_400));
    let output_budget = request.budget.output_bytes.clamp(1, 128 * 1024 * 1024);
    state.agent_runs.insert(AgentRunRecord {
        id: agent_run_id.clone(),
        state: AgentRunState::Running,
        io_mode: request.io_mode,
        source: resolved.source,
        repo_root: resolved.repo_root.clone(),
        program: spec.program.clone(),
        args: spec.args.clone(),
        events: Vec::new(),
        tty_events: TtyRing::new(),
        tty_tx: new_tty_broadcast(),
        controls: Vec::new(),
        outcome: None,
        error_code: None,
        error_message: None,
        control_tx: control,
        repo: None,
        branch: None,
        base_oid: None,
        runner: None,
        agent: None,
    });

    if let Some(prompt) = request.prompt.clone()
        && request.io_mode == AgentRunIoMode::Pty
    {
        let _ = state
            .agent_runs
            .control_sender(&agent_run_id)
            .and_then(|tx| tx.send(AgentControl::InjectPrompt(prompt)).ok());
    }

    spawn_driver_thread(DriverThreadInit {
        store: state.agent_runs.clone(),
        run_id: agent_run_id.clone(),
        repo_root: resolved.repo_root,
        spec,
        io_mode: request.io_mode,
        backend: PtyBackend::Native,
        docker_fallback: None,
        timeout,
        output_budget,
        require_cgroup: request.require_cgroup(),
        control_rx,
    });

    Ok(AgentRunStartResponse {
        agent_run_id: agent_run_id.clone(),
        status_url: format!("/api/v1/agent-runs/{agent_run_id}"),
        events_url: format!("/api/v1/agent-runs/{agent_run_id}/events"),
        control_url: format!("/api/v1/agent-runs/{agent_run_id}/control"),
        export_pr_url: format!("/api/v1/agent-runs/{agent_run_id}/export_pr"),
        ws_scope: format!("agent_run.{agent_run_id}"),
        tty_topic: TTY_TOPIC.to_string(),
        control_topic: CONTROL_TOPIC.to_string(),
        io_mode: request.io_mode,
        state: AgentRunState::Running,
    })
}

/// Everything one driver thread needs to supervise a real child against a run.
/// Both the public agent-run route and the repo-scoped session launch hand this
/// to [`spawn_driver_thread`], so the two share the exact same supervision path:
/// a [`RecordingSink`] feeds `append_event`/`publish_tty`, and `complete` records
/// the terminal outcome.
struct DriverThreadInit {
    store: AgentRunStore,
    run_id: String,
    repo_root: PathBuf,
    spec: CommandSpec,
    io_mode: AgentRunIoMode,
    /// Which PTY execution backend supervises the child. `Native` runs the program
    /// inside the in-process kernel sandbox; `DockerHost` runs an unsandboxed host
    /// `docker run ...` (the container is the jail). Only meaningful for the Pty
    /// io_mode; the Pipe path always uses the native sandbox driver.
    backend: PtyBackend,
    /// Auto-mode docker fallback. When the `Native` backend returns
    /// `sandbox_unavailable` (the host blocks the unprivileged-userns sandbox) and
    /// this carries a docker command, the same run retries on the docker host-PTY
    /// backend instead of failing — the `auto` runtime selector's whole point.
    docker_fallback: Option<CommandSpec>,
    timeout: Duration,
    output_budget: usize,
    require_cgroup: bool,
    control_rx: mpsc::Receiver<AgentControl>,
}

/// Which PTY backend a launched agent runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::web) enum PtyBackend {
    /// In-process kernel-sandbox PTY (the existing [`PtyAgentDriver::run`] path).
    Native,
    /// Host `docker run ...` on a PTY; the container engine confines the agent and
    /// the host process is unsandboxed by design (see [`PtyAgentDriver::run_host_pty`]).
    DockerHost,
}

/// Drive one agent child to completion on a background thread, streaming its TTY
/// output into the run's tty ring + broadcast through a [`RecordingSink`]. The
/// Pty path gives the child a controlling terminal and applies live control
/// commands; the Pipe path supervises over pipes. Either way the terminal
/// outcome lands through `store.complete`.
fn spawn_driver_thread(init: DriverThreadInit) {
    let DriverThreadInit {
        store,
        run_id,
        repo_root,
        spec,
        io_mode,
        backend,
        docker_fallback,
        timeout,
        output_budget,
        require_cgroup,
        control_rx,
    } = init;
    std::thread::spawn(move || {
        let sink = RecordingSink {
            store: store.clone(),
            run_id: run_id.clone(),
        };
        let driver = PtyAgentDriver::new(timeout, output_budget);
        let result = match (io_mode, backend) {
            // Docker-backed live PTY: the container is the jail, so the host
            // `docker run ...` runs unsandboxed on a controlling terminal.
            (AgentRunIoMode::Pty, PtyBackend::DockerHost) => {
                driver.run_host_pty(&repo_root, &spec, &sink, &control_rx)
            }
            (AgentRunIoMode::Pty, PtyBackend::Native) => {
                let native = driver.clone().with_require_cgroup(require_cgroup).run(
                    &repo_root,
                    &spec,
                    &sink,
                    &control_rx,
                );
                match (native, docker_fallback) {
                    // Auto fallback: the host blocked the kernel sandbox, so retry
                    // the same run on the docker host-PTY backend.
                    (Err(DriverError::SandboxUnavailable(_)), Some(docker_spec)) => {
                        driver.run_host_pty(&repo_root, &docker_spec, &sink, &control_rx)
                    }
                    (other, _) => other,
                }
            }
            (AgentRunIoMode::Pipe, _) => AgentDriver::new(timeout, output_budget)
                .with_require_cgroup(require_cgroup)
                .run(&repo_root, &spec, &sink),
        };
        store.complete(&run_id, result);
    });
}

/// Inputs the repo-scoped session launch hands to [`spawn_session_agent`] to put
/// the selected agent on a controlling PTY against the session workspace.
pub(in crate::web) struct SessionAgentSpawn {
    /// The recorded session run the TTY stream is keyed to.
    pub run_id: String,
    /// The materialized session checkout the agent runs inside (its cwd). For the
    /// native backend this is also the agent's cwd; for the docker backend it is
    /// where the host `docker run` process runs (and the workspace it bind-mounts).
    pub workspace: PathBuf,
    /// The launch command the driver runs. `None` means the agent could not be
    /// resolved (a missing host binary, or `docker` absent from PATH), in which
    /// case the run records one graceful "not available" line instead of starting.
    pub spec: Option<CommandSpec>,
    /// Which PTY backend supervises the child (native sandbox vs. host docker).
    pub backend: PtyBackend,
    /// Auto-mode docker fallback command: when `backend` is `Native` and native
    /// returns `sandbox_unavailable`, the run retries on the docker host-PTY
    /// backend with this command instead of failing.
    pub docker_fallback: Option<CommandSpec>,
    /// Live control sender already recorded against the run, moved into the driver.
    pub control_rx: mpsc::Receiver<AgentControl>,
    /// Wall-clock budget for the session agent.
    pub timeout: Duration,
    /// Captured-output byte budget for the session agent.
    pub output_budget: usize,
    /// Whether enforced cgroup-v2 limits are required (false only under test, and
    /// only consulted by the native backend; the docker backend ignores it).
    pub require_cgroup: bool,
}

/// Launch the selected agent for a repo-scoped session on a controlling PTY,
/// wiring its raw terminal output into the recorded run exactly like the public
/// agent-run route. The native backend runs the agent inside the in-process kernel
/// sandbox; the docker backend runs `docker run ...` on the host PTY and the
/// container is the jail. Either way the agent works against the session checkout
/// (its cwd for native, its `/workspace` bind mount for docker) on its own branch.
///
/// When the agent could not be resolved (`spec` is `None`) — a missing host binary
/// or `docker` absent from PATH — rather than fail the whole New Session request,
/// record one clear TTY line so the web terminal shows why the agent never started,
/// and mark the run finished.
pub(in crate::web) fn spawn_session_agent(store: &AgentRunStore, spawn: SessionAgentSpawn) {
    let SessionAgentSpawn {
        run_id,
        workspace,
        spec,
        backend,
        docker_fallback,
        control_rx,
        timeout,
        output_budget,
        require_cgroup,
    } = spawn;

    let Some(spec) = spec else {
        store.note_agent_unavailable(&run_id);
        return;
    };
    spawn_driver_thread(DriverThreadInit {
        store: store.clone(),
        run_id,
        repo_root: workspace,
        spec,
        io_mode: AgentRunIoMode::Pty,
        backend,
        docker_fallback,
        timeout,
        output_budget,
        require_cgroup,
        control_rx,
    });
}

pub(in crate::web) async fn list(
    State(state): State<Arc<WebState>>,
) -> Json<Vec<AgentRunStatusResponse>> {
    Json(state.agent_runs.list())
}

pub(in crate::web) async fn status(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
) -> AxumResponse {
    match state.agent_runs.status(&agent_run_id) {
        Some(response) => Json(response).into_response(),
        None => agent_run_not_found(&agent_run_id),
    }
}

pub(in crate::web) async fn control(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let command = match parse_control_body(&body) {
        Ok(command) => command,
        Err(response) => return *response,
    };
    match state.agent_runs.send_control(&agent_run_id, command) {
        Ok(response) => Json(response).into_response(),
        Err(response) => *response,
    }
}

pub(in crate::web) async fn events(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
    Query(query): Query<AgentRunEventsQuery>,
) -> AxumResponse {
    match state.agent_runs.events(&agent_run_id, query) {
        Some(response) => Json(response).into_response(),
        None => agent_run_not_found(&agent_run_id),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(in crate::web) struct AgentTtyStreamQuery {
    #[serde(default)]
    pub(in crate::web) after_seq: Option<u64>,
}

/// Server-Sent-Events push transport for one run's raw TTY byte stream.
///
/// `GET /api/v1/agent-runs/{id}/tty/stream?after_seq=N` is the jpmc-subscribable
/// live transport: an outside service opens it once and is pushed raw bytes as they
/// reach the single `append_event` publish point, with no cursor-polling of
/// `agent_work.tail`. The handler first replays the retained ring slice past
/// `after_seq` (so a reconnect catches up byte-for-byte), then hands over the live
/// broadcast. Each `data:` frame carries the same JSON shape a tail event does
/// (`seq` + `stream` + `bytes_b64`). It mirrors the WS scope-membership rule: an
/// unknown or non-member run yields the same typed not-found the WS snapshot path
/// ignores. When the live broadcast overflows for a slow subscriber, one
/// `event: resync` frame carrying `oldest_retained_seq` is pushed so the client
/// re-pulls the ring through `agent_work.tail` instead of stalling or erroring.
pub(in crate::web) async fn tty_stream(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
    Query(query): Query<AgentTtyStreamQuery>,
) -> AxumResponse {
    let after_seq = query.after_seq.unwrap_or(0);
    let Some((receiver, replay)) = state.agent_runs.tty_stream_start(&agent_run_id, after_seq)
    else {
        return agent_run_not_found(&agent_run_id);
    };

    // Replay prelude: a resync marker when the cursor already fell behind the ring,
    // then every retained event past the cursor, oldest first.
    let mut prelude: Vec<Result<SseEvent, Infallible>> = Vec::new();
    if replay.lagged {
        prelude.push(Ok(tty_resync_frame(replay.oldest_retained_seq)));
    }
    for event in &replay.events {
        prelude.push(Ok(tty_data_frame(event)));
    }

    // Live tail: events past the replay cursor, with broadcast overflow turned into
    // a single resync marker so a lagged subscriber is told to re-pull, never stalled.
    let store = state.agent_runs.clone();
    let live = stream::unfold(
        (receiver, replay.next_after_seq, store, agent_run_id),
        |(mut receiver, cursor, store, run_id)| async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        if event.seq <= cursor {
                            continue;
                        }
                        let next_cursor = event.seq;
                        let frame = Ok(tty_data_frame(&event));
                        return Some((frame, (receiver, next_cursor, store, run_id)));
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let oldest = store.oldest_retained_tty_seq(&run_id);
                        let frame = Ok(tty_resync_frame(oldest));
                        return Some((frame, (receiver, cursor, store, run_id)));
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    );

    Sse::new(stream::iter(prelude).chain(live)).into_response()
}

/// Encode one raw TTY event as an SSE `data:` frame. The payload is the event's
/// own JSON, so a subscriber reads the same `seq` + `stream` + `bytes_b64` shape a
/// cursor-pull tail returns and raw non-UTF8 bytes ride through base64 intact.
fn tty_data_frame(event: &AgentTtyEvent) -> SseEvent {
    let payload = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
    SseEvent::default().data(payload)
}

/// Build the `resync` marker frame a lagged subscriber receives, carrying the
/// oldest `seq` the ring still retains as the floor to re-pull from.
fn tty_resync_frame(oldest_retained_seq: u64) -> SseEvent {
    SseEvent::default()
        .event("resync")
        .data(json!({ "oldest_retained_seq": oldest_retained_seq }).to_string())
}

pub(in crate::web) async fn export_pr(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> AxumResponse {
    let request: AgentRunExportPrRequest =
        match parse_agent_body(&body, "export an agent run into a pull request") {
            Ok(request) => request,
            Err(response) => return *response,
        };
    match export_workcell_agent_run(&state, &agent_run_id, request, &origin_base_url(&headers)) {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(response) => *response,
    }
}

/// `POST /api/v1/agent-runs/{id}/shell` — spawn a companion shell in the same
/// workspace as the given run. Returns the companion run's id and URLs so the
/// frontend can mount a second terminal pane for free-form operator interaction.
#[derive(Debug, Serialize)]
struct CompanionShellResponse {
    shell_run_id: String,
    status_url: String,
    tty_stream_url: String,
    control_url: String,
}

pub(in crate::web) async fn shell(
    State(state): State<Arc<WebState>>,
    AxumPath(parent_run_id): AxumPath<String>,
) -> AxumResponse {
    // Look up the parent run's workspace.
    let workspace = {
        let inner = state.agent_runs.inner.lock().expect("runs mutex");
        inner.runs.get(&parent_run_id).map(|r| r.repo_root.clone())
    };
    let Some(workspace) = workspace else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": { "code": "not_found", "message": format!("agent run {parent_run_id} not found") }
            })),
        ).into_response();
    };

    // Allocate a new run for the companion shell.
    let shell_id = state.agent_runs.allocate_id();
    let (control_tx, control_rx) = std::sync::mpsc::channel();

    let repo_name = {
        let inner = state.agent_runs.inner.lock().expect("runs mutex");
        inner
            .runs
            .get(&parent_run_id)
            .and_then(|r| r.repo.clone())
            .unwrap_or_default()
    };

    state.agent_runs.insert_session(SessionRecordInit {
        run_id: shell_id.clone(),
        repo: repo_name,
        branch: String::new(),
        base_oid: String::new(),
        runner: "local".to_string(),
        agent: "shell".to_string(),
        program: "/bin/bash".to_string(),
        args: vec!["--login".to_string()],
        workspace: workspace.clone(),
        control_tx: Some(control_tx),
    });

    let spec = CommandSpec {
        program: "/bin/bash".to_string(),
        args: vec!["--login".to_string()],
        env: Default::default(),
    };

    spawn_session_agent(
        &state.agent_runs,
        SessionAgentSpawn {
            run_id: shell_id.clone(),
            workspace,
            spec: Some(spec),
            backend: PtyBackend::Native,
            docker_fallback: None,
            control_rx,
            timeout: std::time::Duration::from_secs(7200),
            output_budget: 20_971_520,
            require_cgroup: false,
        },
    );

    (
        StatusCode::CREATED,
        Json(CompanionShellResponse {
            status_url: format!("/api/v1/agent-runs/{shell_id}"),
            tty_stream_url: format!("/api/v1/agent-runs/{shell_id}/tty/stream"),
            control_url: format!("/api/v1/agent-runs/{shell_id}/control"),
            shell_run_id: shell_id,
        }),
    )
        .into_response()
}

pub(in crate::web) fn mcp_start(state: Arc<WebState>, args: Value) -> Result<Value, String> {
    let request: AgentRunStartRequest =
        serde_json::from_value(args).map_err(|err| err.to_string())?;
    let response = start_request(state, request).map_err(|_| {
        "agent_work.start failed; use the REST route for typed repair details".to_string()
    })?;
    serde_json::to_value(response).map_err(|err| err.to_string())
}

pub(in crate::web) fn mcp_status(state: &Arc<WebState>, args: &Value) -> Result<Value, String> {
    let run_id = required_run_id(args)?;
    let response = state
        .agent_runs
        .status(&run_id)
        .ok_or_else(|| format!("agent run {run_id} was not found"))?;
    serde_json::to_value(response).map_err(|err| err.to_string())
}

pub(in crate::web) fn mcp_control(state: &Arc<WebState>, args: Value) -> Result<Value, String> {
    let run_id = required_run_id(&args)?;
    let command_value = args
        .get("command")
        .cloned()
        .ok_or_else(|| "agent_work.control requires command".to_string())?;
    let command: AgentControlCommand =
        serde_json::from_value(command_value).map_err(|err| err.to_string())?;
    let response = state
        .agent_runs
        .send_control(&run_id, command)
        .map_err(|_| "agent_work.control failed; use REST for typed repair details".to_string())?;
    serde_json::to_value(response).map_err(|err| err.to_string())
}

pub(in crate::web) fn mcp_events(state: &Arc<WebState>, args: &Value) -> Result<Value, String> {
    let run_id = required_run_id(args)?;
    let query = AgentRunEventsQuery {
        after_seq: args.get("after_seq").and_then(Value::as_u64),
        limit: args
            .get("limit")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok()),
    };
    let response = state
        .agent_runs
        .events(&run_id, query)
        .ok_or_else(|| format!("agent run {run_id} was not found"))?;
    serde_json::to_value(response).map_err(|err| err.to_string())
}

pub(in crate::web) fn mcp_tail(state: &Arc<WebState>, args: &Value) -> Result<Value, String> {
    let run_id = required_run_id(args)?;
    let after_seq = args.get("after_seq").and_then(Value::as_u64).unwrap_or(0);
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok());
    let response = state
        .agent_runs
        .tail_tty(&run_id, after_seq, limit)
        .ok_or_else(|| format!("agent run {run_id} was not found"))?;
    serde_json::to_value(response).map_err(|err| err.to_string())
}

pub(in crate::web) fn mcp_export_pr(state: &Arc<WebState>, args: Value) -> Result<Value, String> {
    let run_id = required_run_id(&args)?;
    let request: AgentRunExportPrRequest =
        serde_json::from_value(args).map_err(|err| err.to_string())?;
    let response = export_workcell_agent_run(state, &run_id, request, "").map_err(|_| {
        "agent_work.export_pr failed; use REST for typed repair details".to_string()
    })?;
    serde_json::to_value(response).map_err(|err| err.to_string())
}

fn required_run_id(args: &Value) -> Result<String, String> {
    args.get("agent_run_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| "agent_run_id is required".to_string())
}

fn resolve_agent_run_source(
    state: &Arc<WebState>,
    request: &AgentRunStartRequest,
) -> AgentRunResponseResult<ResolvedAgentRun> {
    match &request.source {
        AgentRunSource::Workcell {
            workcell_id,
            runner_epoch,
        } => resolve_workcell_source(state, request, workcell_id, *runner_epoch),
        AgentRunSource::Repo { repo } => {
            let reason = format!("repo source {repo} needs a checkout allocator before launch");
            Err(agent_run_unavailable(
                "agent_run_repo_source_unavailable",
                "start an agent run from a repository",
                &reason,
            ))
        }
        AgentRunSource::LocalPath { local_path } => {
            let reason = format!(
                "local_path source {} is not enabled for the public agent-run route",
                local_path.display()
            );
            Err(agent_run_unavailable(
                "agent_run_local_path_unavailable",
                "start an agent run from a local path",
                &reason,
            ))
        }
        AgentRunSource::Scratch { name } => {
            let reason = format!(
                "scratch source {} needs a workspace allocator before launch",
                name.as_deref().unwrap_or("unnamed")
            );
            Err(agent_run_unavailable(
                "agent_run_scratch_unavailable",
                "start an agent run from a scratch workspace",
                &reason,
            ))
        }
    }
}

fn resolve_workcell_source(
    state: &Arc<WebState>,
    request: &AgentRunStartRequest,
    workcell_id: &str,
    runner_epoch: u64,
) -> AgentRunResponseResult<ResolvedAgentRun> {
    let lease = match manager(state).workcell(workcell_id).cloned() {
        Some(lease) => lease,
        None => return Err(agent_run_workcell_not_found(workcell_id)),
    };
    if lease.runner_epoch != runner_epoch {
        return Err(boxed_agent_run_typed_error(
            StatusCode::CONFLICT,
            "workcell_epoch_fenced",
            "start an agent run from a failed-CI workcell",
            "request runner_epoch did not match the active workcell epoch",
            &[
                "reload workcell status and retry with the active runner_epoch",
                "discard stale failed-CI repair requests",
            ],
            "the agent run request used a stale workcell epoch",
        ));
    }
    if !matches!(lease.state, WorkcellState::Held | WorkcellState::Repairing) {
        return Err(boxed_agent_run_typed_error(
            StatusCode::CONFLICT,
            "agent_run_workcell_state_denied",
            "start an agent run from a failed-CI workcell",
            "the workcell is not held or repairing",
            &[
                "freeze the failed CI tree before launching the repair agent",
                "use /api/v1/workcells/{id}/run_agent for deterministic claimed-cell commands",
            ],
            "start from a held or repairing workcell, then rerun the agent_runs proof lane",
        ));
    }
    let repo_root = select_repo_root(&lease, request.repo_root.as_deref())?;
    let program = resolve_program(&repo_root, &request.program)?;
    let mut env = request.env.clone();
    inject_workcell_env(&mut env, &lease);
    if request.io_mode == AgentRunIoMode::Pipe
        && let Some(prompt) = &request.prompt
    {
        env.insert("JERYU_AGENT_PROMPT".to_string(), prompt.clone());
    }
    let snapshot = lease.frozen_snapshot.as_ref();
    Ok(ResolvedAgentRun {
        source: AgentRunSourceSnapshot::Workcell {
            workcell_id: lease.workcell_id,
            runner_epoch,
            ci_run_id: snapshot.map(|s| s.ci_run_id.clone()),
            failed_run_id: lease.failed_run_id,
            failed_receipt_id: lease.failed_receipt_id,
            failure_log_digest: lease.failure_log_digest,
        },
        repo_root,
        program,
        env,
    })
}

fn select_repo_root(
    lease: &WorkcellLease,
    requested: Option<&Path>,
) -> AgentRunResponseResult<PathBuf> {
    let selected = match requested {
        Some(path) => normalize_deprecated_host_path(path),
        None => lease.repo_roots.first().cloned().ok_or_else(|| {
            agent_run_path_denied("the workcell has no claimed repo roots to run inside")
        })?,
    };
    let selected = canonical_existing(&selected, "the selected repo root does not exist")?;
    let allowed = lease
        .repo_roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| selected == root);
    if !allowed {
        return Err(agent_run_path_denied(
            "the selected repo root is outside the held workcell slice",
        ));
    }
    Ok(selected)
}

fn resolve_program(repo_root: &Path, program: &str) -> AgentRunResponseResult<PathBuf> {
    let candidate = PathBuf::from(program);
    let candidate = if candidate.is_absolute() {
        normalize_deprecated_host_path(&candidate)
    } else {
        repo_root.join(candidate)
    };
    let candidate = canonical_existing(&candidate, "the requested agent program does not exist")?;
    if !candidate.starts_with(repo_root) {
        return Err(agent_run_path_denied(
            "the requested agent program is outside the selected repo root",
        ));
    }
    Ok(candidate)
}

fn canonical_existing(path: &Path, reason: &'static str) -> AgentRunResponseResult<PathBuf> {
    path.canonicalize()
        .map_err(|_| agent_run_path_denied(reason))
}

fn inject_workcell_env(env: &mut BTreeMap<String, String>, lease: &WorkcellLease) {
    env.insert("JERYU_WORKCELL_ID".to_string(), lease.workcell_id.clone());
    env.insert(
        "JERYU_RUNNER_EPOCH".to_string(),
        lease.runner_epoch.to_string(),
    );
    if let Some(snapshot) = &lease.frozen_snapshot {
        env.insert("JERYU_CI_RUN_ID".to_string(), snapshot.ci_run_id.clone());
        env.insert(
            "JERYU_FAILED_RUN_ID".to_string(),
            snapshot.failed_run_id.clone(),
        );
        env.insert(
            "JERYU_FAILED_RECEIPT_ID".to_string(),
            snapshot.failed_receipt_id.clone(),
        );
        env.insert(
            "JERYU_FAILURE_LOG_DIGEST".to_string(),
            snapshot.failure_log_digest.clone(),
        );
    }
    if let Some(failed_run_id) = &lease.failed_run_id {
        env.entry("JERYU_FAILED_RUN_ID".to_string())
            .or_insert_with(|| failed_run_id.clone());
    }
    if let Some(receipt_id) = &lease.failed_receipt_id {
        env.entry("JERYU_FAILED_RECEIPT_ID".to_string())
            .or_insert_with(|| receipt_id.clone());
    }
    if let Some(digest) = &lease.failure_log_digest {
        env.entry("JERYU_FAILURE_LOG_DIGEST".to_string())
            .or_insert_with(|| digest.clone());
    }
}
