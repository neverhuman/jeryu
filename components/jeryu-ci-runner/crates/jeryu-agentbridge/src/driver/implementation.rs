use super::*;

impl AgentDriver {
    /// Create a driver with an explicit timeout and output budget.
    ///
    /// Defaults `require_cgroup` to `true` (the safe agent-job posture): the run
    /// fails closed on any host without a delegated cgroup-v2 subtree.
    pub fn new(timeout: Duration, output_budget_bytes: usize) -> Self {
        Self {
            timeout,
            output_budget_bytes,
            require_cgroup: true,
        }
    }

    /// Builder: set the wall-clock timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Builder: set the captured-output byte budget.
    #[must_use]
    pub fn with_output_budget(mut self, bytes: usize) -> Self {
        self.output_budget_bytes = bytes;
        self
    }

    /// Builder: require (or not) enforced cgroup-v2 limits for the agent job.
    ///
    /// `true` (the default) fails the launch closed when the host has no
    /// delegated cgroup subtree. Pass `false` ONLY when the run is exercising the
    /// Landlock/seccomp jail rather than cgroups and must proceed on a host
    /// without cgroup delegation.
    #[must_use]
    pub fn with_require_cgroup(mut self, require: bool) -> Self {
        self.require_cgroup = require;
        self
    }

    /// Whether this driver requires enforced cgroup-v2 limits.
    #[must_use]
    pub fn require_cgroup(&self) -> bool {
        self.require_cgroup
    }

    /// Build the confined [`JobRequest`] for an in-cell agent run.
    ///
    /// Public so callers/tests can inspect the policy inputs the driver derives,
    /// mirroring how the native runner exposes its plan. The job is pinned to the
    /// cell workspace, network-deny, and runs at `T1ProtectedInternal` so the
    /// default native-rust-hot runner class applies the full kernel sandbox.
    pub fn build_job(&self, workspace: &Path, spec: &CommandSpec) -> JobRequest {
        JobRequest {
            job_id: format!("agent-cell-{}", jeryu_runner_core::receipt::now_ms()),
            repo_id: "in-cell-agent".to_string(),
            commit_sha: "cell".to_string(),
            workspace: workspace.to_path_buf(),
            command: spec.program.clone(),
            args: spec.args.clone(),
            env: spec.env.clone(),
            trust_tier: TrustTier::T1ProtectedInternal,
            requested_runner: None,
            network_policy: NetworkPolicy::Deny,
            secret_policy: SecretPolicy::None,
            token_policy: TokenPolicy::ReadOnly,
            timeout_ms: u64::try_from(self.timeout.as_millis())
                .unwrap_or(u64::MAX)
                .max(1),
            fork: false,
        }
    }

    /// Run the agent command JAILED inside `workspace`, streaming progress to
    /// `sink` and returning the supervised outcome.
    ///
    /// The pipeline mirrors the native runner: confine -> select_runner ->
    /// `SandboxPlan::from_decision` -> `spawn_sandboxed` -> supervise. The
    /// supervisor enforces both the wall-clock timeout and the captured-output
    /// budget; exceeding either kills the child and sets the matching flag.
    pub fn run<S: AgentEventSink>(
        &self,
        workspace: &Path,
        spec: &CommandSpec,
        sink: &S,
    ) -> Result<AgentRunResult, DriverError> {
        if !workspace.is_dir() {
            return Err(DriverError::Workspace(format!(
                "{} is not an existing directory",
                workspace.display()
            )));
        }

        let job = self.build_job(workspace, spec);
        let decision = select_runner(&job).map_err(|e| DriverError::Policy(e.to_string()))?;
        // Agent jobs default to require_cgroup=true: without a delegated cgroup
        // subtree this resolves to Unavailable and spawn_sandboxed refuses to
        // launch (fail-closed), surfaced below as DriverError::SandboxUnavailable.
        let plan = SandboxPlan::from_decision(workspace, &decision)
            .with_require_cgroup(self.require_cgroup);

        let caps = cached_capabilities();
        let level = caps.enforcement_level(&plan);
        let env = self.sandbox_env(&job, &decision);

        let started = Instant::now();
        // Retry on ETXTBSY ("Text file busy", os error 26): when many cells stage
        // and spawn concurrently, another thread's fork() can transiently inherit
        // the write fd to a freshly-staged binary, so exec races with the copy.
        // This is a transient race (not a sandbox failure), so retry briefly
        // rather than mis-reporting it as SandboxUnavailable.
        let mut attempt: u32 = 0;
        let child = loop {
            match spawn_sandboxed(&job, &plan, caps, &env) {
                Ok(child) => break child,
                Err(e)
                    if attempt < 200
                        && (e.message().contains("os error 26")
                            || e.message().contains("Text file busy")) =>
                {
                    attempt += 1;
                    std::thread::sleep(Duration::from_millis(5));
                }
                // Fail-closed setup failures mean the bot never ran. Surface
                // `Unavailable` distinctly so callers can skip kernel-dependent
                // assertions honestly instead of treating it as a bot failure.
                Err(e) if e.code() == "sandbox_unavailable" => {
                    return Err(DriverError::SandboxUnavailable(e.message().to_string()));
                }
                Err(e) => {
                    return Err(DriverError::SandboxUnavailable(format!(
                        "[{}] {}",
                        e.code(),
                        e.message()
                    )));
                }
            }
        };

        sink.emit(AgentEvent::Started { pid: child.id() });

        let outcome = self.supervise(child, sink, started)?;

        sink.emit(AgentEvent::Finished {
            exit_code: outcome.exit_code,
            timed_out: outcome.timed_out,
            budget_exceeded: outcome.budget_exceeded,
        });

        Ok(AgentRunResult {
            exit_code: outcome.exit_code,
            timed_out: outcome.timed_out,
            budget_exceeded: outcome.budget_exceeded,
            stdout: outcome.stdout,
            stderr: outcome.stderr,
            captured_bytes: outcome.captured_bytes,
            enforcement_level: level.as_str().to_string(),
            elapsed: outcome.elapsed,
        })
    }

    /// Build the sandbox environment for the in-cell agent.
    ///
    /// Starts from the same hardened base the native runner uses (scrubbed,
    /// fixed PATH/HOME, secrets disabled) and then layers the bot's requested
    /// env on top so an edit-bot can be told what to write.
    fn sandbox_env(&self, job: &JobRequest, decision: &PolicyDecision) -> BTreeMap<String, String> {
        let mut env = jeryu_runner_core::fscheck::sanitize_env(&job.env);
        env.insert("HOME".to_string(), "/tmp/jeryu-home".to_string());
        env.insert("TMPDIR".to_string(), "/tmp".to_string());
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert(
            "JERYU_NETWORK_POLICY".to_string(),
            decision.network_policy.as_str().to_string(),
        );
        if !decision.allow_secrets {
            env.insert("JERYU_SECRETS".to_string(), "disabled".to_string());
        }
        env
    }

    /// Supervise the live child: drain stdout/stderr on threads while polling for
    /// exit, the wall-clock deadline, and the output budget. The first of
    /// {exit, timeout, budget} wins; timeout/budget kill the child.
    fn supervise<S: AgentEventSink>(
        &self,
        mut child: Child,
        sink: &S,
        started: Instant,
    ) -> Result<SuperviseOutcome, DriverError> {
        let stdout_rx = child.stdout.take().map(spawn_line_reader);
        let stderr_rx = child.stderr.take().map(spawn_line_reader);

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut used = 0usize;
        let mut timed_out = false;
        let mut budget_exceeded = false;

        // Drain whatever lines are ready RIGHT NOW from one stream into its
        // buffer, emit per-line events, and grow the running byte total. Returns
        // true when the byte budget was tripped on this drain.
        let drain = |rx: &Option<Receiver<Line>>,
                     buf: &mut Vec<u8>,
                     used: &mut usize,
                     is_stdout: bool|
         -> bool {
            let Some(rx) = rx else { return false };
            loop {
                match rx.try_recv() {
                    Ok(Line::Bytes(line)) => {
                        *used += line.len();
                        buf.extend_from_slice(&line);
                        let text = String::from_utf8_lossy(&line).trim_end().to_string();
                        if is_stdout {
                            sink.emit(AgentEvent::Stdout(text));
                        } else {
                            sink.emit(AgentEvent::Stderr(text));
                        }
                        sink.emit(AgentEvent::Budget {
                            used: *used,
                            limit: self.output_budget_bytes,
                        });
                        if *used > self.output_budget_bytes {
                            return true;
                        }
                    }
                    Ok(Line::Eof) | Err(TryRecvError::Disconnected) => return false,
                    Err(TryRecvError::Empty) => return false,
                }
            }
        };

        let exit_code = loop {
            // Pull any pending output first so a budget breach is seen promptly.
            if drain(&stdout_rx, &mut stdout, &mut used, true)
                || drain(&stderr_rx, &mut stderr, &mut used, false)
            {
                budget_exceeded = true;
                let _ = child.kill();
                let status = child
                    .wait()
                    .map_err(|e| DriverError::Supervision(e.to_string()))?;
                break status.code();
            }

            match child
                .try_wait()
                .map_err(|e| DriverError::Supervision(e.to_string()))?
            {
                Some(status) => break status.code(),
                None => {
                    if started.elapsed() >= self.timeout {
                        timed_out = true;
                        let _ = child.kill();
                        let status = child
                            .wait()
                            .map_err(|e| DriverError::Supervision(e.to_string()))?;
                        break status.code();
                    }
                    thread::sleep(poll_interval(started.elapsed(), self.timeout));
                }
            }
        };

        // Final drain of any output produced between the last poll and exit. We
        // ignore a late budget trip here (the child has already exited), but we
        // still account the bytes so `captured_bytes` is honest.
        drain(&stdout_rx, &mut stdout, &mut used, true);
        drain(&stderr_rx, &mut stderr, &mut used, false);

        // Truncate the captured buffers to the budget so a final burst cannot
        // blow the cap retroactively.
        truncate_to(&mut stdout, &mut stderr, self.output_budget_bytes);

        Ok(SuperviseOutcome {
            exit_code,
            timed_out,
            budget_exceeded,
            stdout,
            stderr,
            captured_bytes: used,
            elapsed: started.elapsed(),
        })
    }
}
