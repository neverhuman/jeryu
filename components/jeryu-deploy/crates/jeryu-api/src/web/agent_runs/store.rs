//! Bounded run, event, control, and live-TTY state.

use super::*;

impl AgentRunRecord {
    /// Single publish point for one raw TTY event: append it to the bounded ring,
    /// then fan it out to any live SSE subscriber. The ring push happens first so a
    /// subscriber that resyncs after a broadcast overflow always finds the event in
    /// the retained byte history. A send with no live receivers is a harmless drop.
    fn publish_tty(&mut self, event: AgentTtyEvent) {
        self.tty_events.push(event.clone());
        let _ = self.tty_tx.send(event);
    }
}

impl AgentRunStore {
    pub(in crate::web) fn new() -> Self {
        Self::default()
    }

    pub(in crate::web) fn allocate_id(&self) -> String {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        format!("ar-{id:06}")
    }

    pub(super) fn insert(&self, record: AgentRunRecord) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        inner.runs.insert(record.id.clone(), record);
    }

    /// Record a freshly-launched repo-scoped agent session. The run starts in the
    /// `Running` state on its own unique branch; the `repo` it carries is what the
    /// per-repo agent-runs route filters on, so a session is only ever visible to
    /// the repository that owns it.
    pub(in crate::web) fn insert_session(&self, init: SessionRecordInit) {
        self.insert(AgentRunRecord {
            id: init.run_id,
            state: AgentRunState::Running,
            io_mode: AgentRunIoMode::Pty,
            source: AgentRunSourceSnapshot::Repo {
                repo: init.repo.clone(),
            },
            repo_root: init.workspace,
            program: init.program,
            args: init.args,
            events: Vec::new(),
            tty_events: TtyRing::new(),
            tty_tx: new_tty_broadcast(),
            controls: Vec::new(),
            outcome: None,
            error_code: None,
            error_message: None,
            // The session's live control channel: the launch records the sender so
            // the web terminal can steer the PTY agent, and hands the receiver to
            // the driver thread.
            control_tx: init.control_tx,
            repo: Some(init.repo),
            branch: Some(init.branch),
            base_oid: Some(init.base_oid),
            runner: Some(init.runner),
            agent: Some(init.agent),
        });
    }

    /// Record one clear TTY line for a session whose agent binary could not be
    /// resolved, then mark the run finished. The New Session request still returns
    /// a recorded run, and the web terminal degrades gracefully to a single line
    /// that names the missing agent instead of an empty stream.
    pub(in crate::web) fn note_agent_unavailable(&self, run_id: &str) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        let Some(record) = inner.runs.get_mut(run_id) else {
            return;
        };
        let program = record.program.clone();
        let agent = record.agent.clone().unwrap_or_else(|| "agent".to_string());
        let line = format!("agent {agent} not available: {program}\r\n");
        let seq = (record.events.len() as u64).saturating_add(1);
        let event = AgentRunEventInput {
            kind: "tty",
            stream: Some("stderr"),
            text: Some(line),
            pid: None,
            used: None,
            limit: None,
            exit_code: None,
            timed_out: false,
            budget_exceeded: false,
        }
        .into_event(seq);
        let tty_event = tty_event_for(record, &event);
        record.events.push(event);
        record.publish_tty(tty_event);
        record.state = AgentRunState::Failed;
        record.control_tx = None;
    }

    /// Record one clear TTY line for a session whose workspace checkout could not be
    /// materialized (the bare clone or branch checkout failed), then mark the run
    /// failed. The New Session request still returns a recorded run and 2xx; the web
    /// terminal degrades to a single line naming the reason rather than launching an
    /// agent against an empty, code-less workspace.
    pub(in crate::web) fn note_session_checkout_failed(&self, run_id: &str, reason: &str) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        let Some(record) = inner.runs.get_mut(run_id) else {
            return;
        };
        let line = format!("session workspace checkout failed: {reason}\r\n");
        let seq = (record.events.len() as u64).saturating_add(1);
        let event = AgentRunEventInput {
            kind: "tty",
            stream: Some("stderr"),
            text: Some(line),
            pid: None,
            used: None,
            limit: None,
            exit_code: None,
            timed_out: false,
            budget_exceeded: false,
        }
        .into_event(seq);
        let tty_event = tty_event_for(record, &event);
        record.events.push(event);
        record.publish_tty(tty_event);
        record.state = AgentRunState::Failed;
        record.control_tx = None;
    }

    /// Live agent-run rows for ONE repository. Filters strictly on the run's owning
    /// `repo`, so runs that belong to a different repository (or to a workcell, with
    /// no repo) are never returned here — the data-isolation invariant the
    /// per-repo route depends on.
    pub(in crate::web) fn rows_for_repo(&self, repo_full_name: &str) -> Vec<RepoAgentRunRow> {
        let inner = self.inner.lock().expect("agent run store mutex");
        inner
            .runs
            .values()
            .filter(|record| {
                record.repo.as_deref() == Some(repo_full_name)
                    && record.agent.as_deref() != Some("shell")
            })
            .map(|record| {
                let mut row = RepoAgentRunRow::from_record(record);
                row.shell_run_id = inner.shell_companions.get(&record.id).cloned();
                row
            })
            .collect()
    }

    /// Register a companion shell for a given agent run.
    pub(in crate::web) fn register_shell_companion(&self, agent_run_id: &str, shell_run_id: &str) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        inner
            .shell_companions
            .insert(agent_run_id.to_string(), shell_run_id.to_string());
    }

    /// The branch + base oid + state needed to mediate a publish for one run.
    pub(in crate::web) fn publish_info(&self, run_id: &str) -> Option<SessionPublishInfo> {
        let inner = self.inner.lock().expect("agent run store mutex");
        let record = inner.runs.get(run_id)?;
        Some(SessionPublishInfo {
            repo: record.repo.clone(),
            branch: record.branch.clone(),
            base_oid: record.base_oid.clone(),
            state: record.state,
        })
    }

    pub(super) fn status(&self, run_id: &str) -> Option<AgentRunStatusResponse> {
        let inner = self.inner.lock().expect("agent run store mutex");
        inner
            .runs
            .get(run_id)
            .and_then(|record| status_from_record(run_id, record))
    }

    pub(in crate::web) fn list(&self) -> Vec<AgentRunStatusResponse> {
        let inner = self.inner.lock().expect("agent run store mutex");
        inner
            .runs
            .iter()
            .filter_map(|(run_id, record)| status_from_record(run_id, record))
            .collect()
    }

    pub(in crate::web) fn list_json(&self) -> Vec<Value> {
        self.list()
            .into_iter()
            .filter_map(|status| serde_json::to_value(status).ok())
            .collect()
    }

    pub(super) fn events(
        &self,
        run_id: &str,
        query: AgentRunEventsQuery,
    ) -> Option<AgentRunEventsResponse> {
        let after_seq = query.after_seq.unwrap_or(0);
        let limit = query.limit.unwrap_or(100).clamp(1, 1_000);
        let inner = self.inner.lock().expect("agent run store mutex");
        let record = inner.runs.get(run_id)?;
        let all_events: Vec<AgentRunEvent> = record
            .events
            .iter()
            .filter(|event| event.seq > after_seq)
            .cloned()
            .collect();
        let has_more = all_events.len() > limit;
        let events: Vec<AgentRunEvent> = all_events.into_iter().take(limit).collect();
        let next_after_seq = events.last().map(|event| event.seq).unwrap_or(after_seq);
        let tty_events = record
            .tty_events
            .iter()
            .filter(|event| event.seq > after_seq && event.seq <= next_after_seq)
            .cloned()
            .collect::<Vec<_>>();
        Some(AgentRunEventsResponse {
            agent_run_id: record.id.clone(),
            after_seq,
            next_after_seq,
            limit,
            has_more,
            events,
            tty_events,
        })
    }

    pub(super) fn control_sender(&self, run_id: &str) -> Option<Sender<AgentControl>> {
        let inner = self.inner.lock().expect("agent run store mutex");
        inner
            .runs
            .get(run_id)
            .and_then(|record| record.control_tx.clone())
    }

    pub(super) fn record(&self, run_id: &str) -> Option<AgentRunRecord> {
        let inner = self.inner.lock().expect("agent run store mutex");
        inner.runs.get(run_id).cloned()
    }

    pub(in crate::web) fn mark_exported(&self, run_id: &str) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        if let Some(record) = inner.runs.get_mut(run_id) {
            record.state = AgentRunState::Exported;
        }
    }

    pub(super) fn send_control(
        &self,
        run_id: &str,
        command: AgentControlCommand,
    ) -> AgentRunResponseResult<AgentRunControlResponse> {
        let (tx, control, command_name, seq) = {
            let mut inner = self.inner.lock().expect("agent run store mutex");
            let record = inner
                .runs
                .get_mut(run_id)
                .ok_or_else(|| Box::new(agent_run_not_found(run_id)))?;
            if record.state != AgentRunState::Running {
                return Err(boxed_agent_run_typed_error(
                    StatusCode::CONFLICT,
                    "agent_run_finished",
                    "send control to an agent run",
                    "the agent run is already finished",
                    &[
                        "reload the run status before sending more control",
                        "start a new agent run for additional repair work",
                    ],
                    "start a fresh run, then send control while it is running",
                ));
            }
            if record.io_mode != AgentRunIoMode::Pty {
                return Err(boxed_agent_run_typed_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "agent_run_control_unsupported",
                    "send control to an agent run",
                    "the selected io_mode does not support live control",
                    &[
                        "start the run with io_mode pty",
                        "use pipe mode only for deterministic non-interactive commands",
                    ],
                    "rerun the agent with io_mode pty before sending control",
                ));
            }
            let Some(tx) = record.control_tx.clone() else {
                return Err(boxed_agent_run_typed_error(
                    StatusCode::CONFLICT,
                    "agent_run_control_unavailable",
                    "send control to an agent run",
                    "the live control channel is no longer available",
                    &[
                        "reload the run status before sending more control",
                        "check whether the driver has already exited",
                    ],
                    "retry only while the run is still marked running",
                ));
            };
            let control = map_control(&command);
            let command_name = command_name(&command).to_string();
            let seq = (record.controls.len() as u64).saturating_add(1);
            record.controls.push(AgentRunControlRecord {
                seq,
                command: command_name.clone(),
            });
            (tx, control, command_name, seq)
        };
        tx.send(control).map_err(|_| {
            boxed_agent_run_typed_error(
                StatusCode::CONFLICT,
                "agent_run_control_closed",
                "send control to an agent run",
                "the live driver stopped before the control command was delivered",
                &[
                    "reload the run status before sending more control",
                    "start a new run if more repair work is required",
                ],
                "send controls only while the status endpoint reports running",
            )
        })?;
        Ok(AgentRunControlResponse {
            agent_run_id: run_id.to_string(),
            accepted: true,
            control_seq: seq,
            command: command_name,
        })
    }

    pub(super) fn append_event(&self, run_id: &str, event: AgentRunEventInput) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        let Some(record) = inner.runs.get_mut(run_id) else {
            return;
        };
        let seq = (record.events.len() as u64).saturating_add(1);
        let event = event.into_event(seq);
        let tty_event = tty_event_for(record, &event);
        record.events.push(event);
        record.publish_tty(tty_event);
    }

    /// Cursor-pull tail of one run's raw TTY events. Returns every retained event
    /// with `seq > after_seq` (raw `bytes_b64` intact), capped at `limit`. When the
    /// cursor fell behind the drop-oldest ring `lagged` is true and the events come
    /// from the oldest retained `seq` so the caller (jpmc) can resync. `next_after_seq`
    /// is the highest returned `seq`, or the input cursor when nothing is newer.
    pub(super) fn tail_tty(
        &self,
        run_id: &str,
        after_seq: u64,
        limit: Option<usize>,
    ) -> Option<AgentRunTailResponse> {
        let limit = limit.unwrap_or(TTY_RING_CAP).clamp(1, TTY_RING_CAP);
        let inner = self.inner.lock().expect("agent run store mutex");
        let record = inner.runs.get(run_id)?;
        let lagged = record.tty_events.lagged(after_seq);
        let events = record.tty_events.tail(after_seq, limit);
        let next_after_seq = events.last().map_or(after_seq, |event| event.seq);
        Some(AgentRunTailResponse {
            agent_run_id: record.id.clone(),
            after_seq,
            next_after_seq,
            oldest_retained_seq: record.tty_events.oldest_retained_seq,
            lagged,
            tty_topic: TTY_TOPIC.to_string(),
            events,
        })
    }

    /// Atomically open a live SSE subscription for one run: under a single lock it
    /// subscribes to the run's broadcast AND snapshots the retained ring past
    /// `after_seq`. Subscribing before releasing the lock guarantees no event slips
    /// between the replay snapshot and the live feed (any later publish also takes
    /// the lock), so the caller only needs to drop live events whose `seq` is not
    /// past the replay's `next_after_seq`. Returns `None` for an unknown run, which
    /// is how the SSE edge denies an unknown or non-member scope.
    pub(super) fn tty_stream_start(
        &self,
        run_id: &str,
        after_seq: u64,
    ) -> Option<(broadcast::Receiver<AgentTtyEvent>, AgentRunTailResponse)> {
        let inner = self.inner.lock().expect("agent run store mutex");
        let record = inner.runs.get(run_id)?;
        let receiver = record.tty_tx.subscribe();
        let lagged = record.tty_events.lagged(after_seq);
        let events = record.tty_events.tail(after_seq, TTY_RING_CAP);
        let next_after_seq = events.last().map_or(after_seq, |event| event.seq);
        let response = AgentRunTailResponse {
            agent_run_id: record.id.clone(),
            after_seq,
            next_after_seq,
            oldest_retained_seq: record.tty_events.oldest_retained_seq,
            lagged,
            tty_topic: TTY_TOPIC.to_string(),
            events,
        };
        Some((receiver, response))
    }

    /// The oldest raw TTY `seq` still retained for one run (0 when empty or unknown).
    /// A live SSE stream reads this when its broadcast overflows so the `resync`
    /// marker tells a lagged subscriber the floor to re-pull the ring from.
    pub(super) fn oldest_retained_tty_seq(&self, run_id: &str) -> u64 {
        let inner = self.inner.lock().expect("agent run store mutex");
        inner
            .runs
            .get(run_id)
            .map_or(0, |record| record.tty_events.oldest_retained_seq)
    }

    pub(super) fn complete(&self, run_id: &str, result: Result<AgentRunResult, DriverError>) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        let Some(record) = inner.runs.get_mut(run_id) else {
            return;
        };
        record.control_tx = None;
        match result {
            Ok(result) => {
                let outcome = AgentRunOutcome::from_result(result);
                record.state = if outcome.succeeded {
                    AgentRunState::Succeeded
                } else {
                    AgentRunState::Failed
                };
                record.outcome = Some(outcome);
            }
            Err(err) => {
                let (code, message) = driver_error_parts(err);
                record.state = AgentRunState::Failed;
                record.error_code = Some(code.to_string());
                record.error_message = Some(message);
            }
        }
    }
}

#[cfg(test)]
impl AgentRunStore {
    /// Seed a minimal repo-scoped run for tail/ring coverage with a chosen ring
    /// cap so eviction can be forced without pushing the whole production bound.
    pub(in crate::web) fn seed_test_run(&self, run_id: &str, ring_cap: usize) {
        self.insert(AgentRunRecord {
            id: run_id.to_string(),
            state: AgentRunState::Running,
            io_mode: AgentRunIoMode::Pty,
            source: AgentRunSourceSnapshot::Repo {
                repo: "owner/repo".to_string(),
            },
            repo_root: PathBuf::from("/tmp/agent-run"),
            program: "agent".to_string(),
            args: Vec::new(),
            events: Vec::new(),
            tty_events: TtyRing::with_cap(ring_cap),
            tty_tx: new_tty_broadcast(),
            controls: Vec::new(),
            outcome: None,
            error_code: None,
            error_message: None,
            control_tx: None,
            repo: Some("owner/repo".to_string()),
            branch: Some("sessions/test".to_string()),
            base_oid: Some("oid-test".to_string()),
            runner: None,
            agent: None,
        });
    }

    /// Publish one prebuilt raw TTY event through the same ring-plus-broadcast path
    /// `append_event` uses, so a test can drive both the cursor-pull replay and the
    /// live SSE fan-out from one helper.
    pub(in crate::web) fn push_test_tty(&self, run_id: &str, event: AgentTtyEvent) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        if let Some(record) = inner.runs.get_mut(run_id) {
            record.publish_tty(event);
        }
    }
}

/// Live fan-out depth of the per-run raw TTY broadcast, exposed so a route test can
/// overflow a slow subscriber by exactly enough to force the resync path.
#[cfg(test)]
pub(in crate::web) fn tty_broadcast_capacity() -> usize {
    TTY_BROADCAST_CAP
}

/// Build a raw (non-text) TTY event whose payload is the base64 of `bytes`, so a
/// test can prove a non-UTF8 byte sequence survives the ring and tail byte-for-byte.
#[cfg(test)]
pub(in crate::web) fn test_raw_tty_event(run_id: &str, seq: u64, bytes: &[u8]) -> AgentTtyEvent {
    use base64::Engine;
    let key = AgentRunStreamKey {
        repo: Some("owner/repo".to_string()),
        workcell_id: "owner/repo".to_string(),
        agent_run_id: run_id.to_string(),
        agent: "agent".to_string(),
        model: "local".to_string(),
    };
    let mut event = AgentTtyEvent::text(seq, 0, &key, AgentOutputStream::Pty, String::new());
    event.text = None;
    event.bytes_b64 = Some(base64::engine::general_purpose::STANDARD.encode(bytes));
    event
}

fn status_from_record(run_id: &str, record: &AgentRunRecord) -> Option<AgentRunStatusResponse> {
    if record.id != run_id {
        return None;
    }
    Some(AgentRunStatusResponse {
        agent_run_id: record.id.clone(),
        state: record.state,
        io_mode: record.io_mode,
        source: record.source.clone(),
        repo_root: record.repo_root.clone(),
        program: record.program.clone(),
        args: record.args.clone(),
        events_url: format!("/api/v1/agent-runs/{run_id}/events"),
        control_url: format!("/api/v1/agent-runs/{run_id}/control"),
        export_pr_url: format!("/api/v1/agent-runs/{run_id}/export_pr"),
        ws_scope: format!("agent_run.{run_id}"),
        tty_topic: TTY_TOPIC.to_string(),
        control_topic: CONTROL_TOPIC.to_string(),
        events: record.events.clone(),
        tty_events: record.tty_events.snapshot(),
        controls: record.controls.clone(),
        outcome: record.outcome.clone(),
        error_code: record.error_code.clone(),
        error_message: record.error_message.clone(),
    })
}

fn tty_event_for(record: &AgentRunRecord, event: &AgentRunEvent) -> AgentTtyEvent {
    let key = stream_key_for(record);
    let stream = match event.stream.as_deref() {
        Some("stdout") => AgentOutputStream::Stdout,
        Some("stderr") => AgentOutputStream::Stderr,
        Some("pty") => AgentOutputStream::Pty,
        _ => AgentOutputStream::Event,
    };
    let mut tty = if event.kind == "finished" {
        AgentTtyEvent::finished(
            event.seq,
            epoch_millis(),
            &key,
            event.exit_code,
            record
                .outcome
                .as_ref()
                .map(|outcome| outcome.enforcement_level.clone())
                .unwrap_or_else(|| "pending".to_string()),
        )
    } else {
        AgentTtyEvent::text(
            event.seq,
            epoch_millis(),
            &key,
            stream,
            event.text.clone().unwrap_or_else(|| event.kind.clone()),
        )
    };
    if let Some(limit) = event.limit {
        tty.budget = Some(AgentEventBudget {
            wall_secs: 0,
            output_bytes: limit as u64,
            used_output_bytes: event.used.unwrap_or(0) as u64,
        });
    }
    tty
}

fn stream_key_for(record: &AgentRunRecord) -> AgentRunStreamKey {
    let workcell_id = match &record.source {
        AgentRunSourceSnapshot::Workcell { workcell_id, .. } => workcell_id.clone(),
        AgentRunSourceSnapshot::Repo { repo } => repo.clone(),
        AgentRunSourceSnapshot::LocalPath { local_path } => {
            local_path.to_string_lossy().to_string()
        }
        AgentRunSourceSnapshot::Scratch { name } => {
            name.clone().unwrap_or_else(|| "scratch".to_string())
        }
    };
    let repo = match &record.source {
        AgentRunSourceSnapshot::Repo { repo } => Some(repo.clone()),
        AgentRunSourceSnapshot::Workcell { .. }
        | AgentRunSourceSnapshot::LocalPath { .. }
        | AgentRunSourceSnapshot::Scratch { .. } => None,
    };
    AgentRunStreamKey {
        repo,
        workcell_id,
        agent_run_id: record.id.clone(),
        agent: agent_label(&record.program),
        model: "local".to_string(),
    }
}

fn agent_label(program: &str) -> String {
    Path::new(program)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("agent")
        .to_string()
}

fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
