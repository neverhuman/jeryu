use super::*;
use std::path::PathBuf;

#[cfg(test)]
pub(crate) async fn test_app() -> anyhow::Result<App> {
    let docker = crate::tui::test_support::docker_ctl()?;
    let gitlab = GitlabClient::new("http://127.0.0.1:9", None);
    Ok(App::new_render_only(docker, gitlab))
}
impl App {
    pub fn new(store: TuiSession, docker: DockerCtl, gitlab: GitlabClient) -> Self {
        Self::build(Some(store), docker, gitlab)
    }

    pub fn new_render_only(docker: DockerCtl, gitlab: GitlabClient) -> Self {
        Self::build(None, docker, gitlab)
    }

    fn build(store: Option<TuiSession>, docker: DockerCtl, gitlab: GitlabClient) -> Self {
        let (sync_tx, sync_rx) = mpsc::channel(4);
        let (flow_tx, flow_rx) = mpsc::channel(4);
        let (log_tx, log_rx) = mpsc::channel(8);
        let (feed_tx, feed_rx) = mpsc::channel(4);
        let (log_target_tx, _log_target_rx) = watch::channel(None);
        Self {
            store,
            docker,
            gitlab,
            autonomy_dir: std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".jeryu/autonomy"),
            llm_secret_resolver: None,
            state: TuiStateSnapshot::default(),
            active_tab: ActiveTab::default(),
            active_pane: ActivePane::default(),
            release_subpane: ReleaseSubPane::default(),
            selected_approval_index: 0,
            selected_pool_index: 0,
            selected_pipeline_index: 0,
            selected_job_index: 0,
            selected_bug_index: 0,
            selected_bug_project_index: 0,
            bug_sort_mode: crate::tui::bugs::BugSortMode::default(),
            selected_job_id: None,
            selected_secret_index: 0,
            selected_git_index: 0,
            maximize_logs: false,
            log_scroll_offset: 0,
            follow_log_tail: true,
            test_view_mode: TestViewMode::default(),
            selected_test_index: 0,
            selected_test_history: None,
            selected_evidence_index: 0,
            command_palette_open: false,
            command_palette_query: String::new(),
            selected_palette_index: 0,
            evidence_view_mode: EvidenceViewMode::default(),
            focus: crate::tui::focus::FocusState::default(),
            focus_map: crate::tui::focus::FocusMap::default(),
            tick_count: 0,
            log_target: None,
            log_target_tx,
            feed_scroll_offset: 0,
            feed_follow_tail: true,
            feed_pinned: None,
            search_active: false,
            search_query: String::new(),
            help_overlay_open: false,
            workflow_nav: crate::tui::workflow::nav::WorkflowNav::default(),
            workflow_snapshot: crate::tui::workflow::model::WorkflowSnapshot::empty(),
            workflow_inspect_open: false,
            delivery_snapshot: crate::tui::workflow::model::DeliverySnapshot::empty(),
            inspector_tab: crate::tui::workflow::inspector::InspectorTab::default(),
            delivery_hit_map: crate::tui::workflow::hit_map::DeliveryHitMap::default(),
            drag_origin: None,
            delivery_action_message: None,
            action_pane: crate::tui::workflow::actions::ActionPaneState::default(),
            action_adapter: std::sync::Arc::new(
                crate::tui::workflow::action_adapter::FakeActionAdapter::new(),
            ),
            sync_rx,
            sync_tx,
            log_rx,
            log_tx,
            flow_rx,
            flow_tx,
            feed_rx,
            feed_tx,
        }
    }

    pub fn apply_demo_fixture(&mut self) {
        app_runtime_demo::apply_demo_fixture(self);
    }

    pub fn tick_demo_state(&mut self) {
        app_runtime_demo::tick_demo_state(self);
    }

    /// Try to upgrade the default `FakeActionAdapter` to a real
    /// `ProductionActionAdapter`. Returns `Ok(())` when the database pool,
    /// GitHub token, and signing key all resolve; otherwise returns `Err`
    /// with an explanatory message and leaves the existing fake adapter in
    /// place so the TUI keeps rendering.
    ///
    /// This is intentionally non-blocking for the render loop: callers
    /// (typically `tui/runner.rs::run_tui` or `bin/autonomy.rs`) invoke it
    /// once during app construction and surface the error to the operator
    /// rather than aborting the cockpit. The TUI itself never blocks
    /// waiting for this method.
    pub async fn try_install_production_adapter(&mut self) -> anyhow::Result<()> {
        use crate::autonomy::signing::EdSigningKey;
        use crate::git_host::GitHubClient;
        use crate::llm::secrets::{SecretResolver, resolve_secret};
        use crate::state::Db;
        use crate::tui::workflow::action_adapter::ProductionActionAdapter;
        use anyhow::{Context, anyhow};
        use std::sync::Arc;

        let resolver = SecretResolver::from_env();
        let token = resolve_secret("GITHUB_TOKEN", &resolver)
            .ok_or_else(|| anyhow!("GITHUB_TOKEN not found in secret chain"))?;
        let db = Db::open()
            .await
            .context("open jeryu db for action adapter")?;
        let pool = db.pool();
        let signing_key = Arc::new(EdSigningKey::generate("tui.cockpit.v1"));
        let adapter = ProductionActionAdapter::new(
            Arc::new(GitHubClient::new(token.value)),
            pool,
            signing_key,
        );
        self.action_adapter = Arc::new(adapter);
        Ok(())
    }

    /// Toggle the visibility of the Mission Control action pane.
    pub fn toggle_action_pane(&mut self) {
        self.action_pane.visible = !self.action_pane.visible;
    }

    /// Route a key into the action pane. Returns `true` when the pane
    /// consumed the key (caller should not propagate to normal nav).
    ///
    /// Wave 6.A: this is async because `handle_delivery_action` now hits
    /// real git_host and KillBell adapters via the `ActionAdapter` trait.
    /// The App holds an `Arc<dyn ActionAdapter>` that defaults to
    /// `FakeActionAdapter`; production builds swap it in at startup via
    /// `try_install_production_adapter` so the pane immediately drives
    /// real backends without changing the per-keystroke code path.
    pub async fn action_pane_key(&mut self, key: crossterm::event::KeyCode) -> bool {
        use crate::tui::workflow::actions::dispatch_key;
        if !self.action_pane.visible {
            return false;
        }
        if let Some(action) = dispatch_key(&mut self.action_pane, &self.delivery_snapshot, key) {
            // Clone the Arc so we hold a stable handle to the installed
            // adapter even though `handle_delivery_action` borrows `self`
            // mutably. The clone is cheap (every adapter field is Arc-
            // shaped or pool-cloneable).
            let adapter = self.action_adapter.clone();
            self.handle_delivery_action(action, adapter.as_ref()).await;
        }
        true
    }

    /// Route a `DeliveryAction` from the action pane to real backends behind
    /// the `ActionAdapter` trait seam.
    ///
    /// Wave 6.A wiring (per `docs/evidence-gate-spec.md`):
    /// - `ApproveOnce`   → `post_passport_check(AllowMerge)` + signed
    ///                     `HumanDecisionRecorded` ledger event
    /// - `BlockVerdict`  → `post_passport_check(Reject)` THEN
    ///                     `post_mr_comment` with the reason; signed
    ///                     `HumanDecisionRecorded`
    /// - `RequestRepair` → `post_mr_comment` only; signed
    ///                     `HumanDecisionRecorded`
    /// - `FreezeAutonomy`→ NO adapter call (freeze is a control-plane edit
    ///                     owned by ops). Signed `HumanEscalationRequested`
    ///                     with payload `{"action":"freeze_intent","hours":H}`.
    /// - `KillBell`      → `pause_kill_bell(reason, "tui.cockpit.v1", 86400)`.
    ///   The adapter's `KillBell::pause` already appends
    ///   the `KillBellEngaged` row; we MUST NOT also
    ///   append here (regression test
    ///   `kill_bell_action_does_not_double_append_ledger_entry`).
    ///
    /// Every actor stamp is `"tui.cockpit.v1"`; that string is the
    /// audit-trail anchor for human-in-the-loop interrupts. See
    /// `tips/fullauto/tip8.txt`.
    pub async fn handle_delivery_action(
        &mut self,
        action: crate::tui::workflow::actions::DeliveryAction,
        adapter: &dyn crate::tui::workflow::action_adapter::ActionAdapter,
    ) {
        use crate::tui::workflow::action_adapter::handler_helpers as helpers;
        use crate::tui::workflow::actions::{ActionOutcome, ActionResult, DeliveryAction};

        let now = chrono::Utc::now();
        match action {
            DeliveryAction::ApproveOnce { pr_idx } => {
                let Some(ctx) = helpers::pr_ctx(&self.delivery_snapshot, pr_idx) else {
                    self.delivery_action_message = Some("APPROVE: no PR selected".into());
                    self.action_pane.last_result = Some(ActionResult {
                        action: "Approve".into(),
                        outcome: ActionOutcome::Failed("no PR selected".into()),
                        at: now,
                    });
                    return;
                };
                let summary = format!(
                    "operator approval via TUI cockpit for PR #{}",
                    ctx.pr_number
                );
                let outcome = match adapter
                    .post_passport_check(
                        &ctx.repo_slug,
                        &ctx.head_sha,
                        crate::autonomy::types::GateDecision::AllowMerge,
                        &summary,
                    )
                    .await
                {
                    Ok(_id) => {
                        let entry = helpers::ledger_entry(
                            crate::autonomy::types::LedgerKind::HumanDecisionRecorded,
                            &ctx,
                            serde_json::json!({
                                "action": "approve_once",
                                "pr_number": ctx.pr_number,
                                "head_sha": ctx.head_sha,
                            }),
                            now,
                        );
                        if let Err(e) = adapter.append_ledger(entry).await {
                            tracing::warn!(target: "tui.cockpit", err = %e, "approve: ledger append failed");
                        }
                        self.delivery_action_message =
                            Some(format!("APPROVE submitted for PR #{}", ctx.pr_number));
                        ActionOutcome::Submitted
                    }
                    Err(e) => {
                        self.delivery_action_message = Some(format!("APPROVE failed: {e}"));
                        ActionOutcome::Failed(e)
                    }
                };
                self.action_pane.last_result = Some(ActionResult {
                    action: "Approve".into(),
                    outcome,
                    at: now,
                });
            }
            DeliveryAction::BlockVerdict { pr_idx, reason } => {
                let Some(ctx) = helpers::pr_ctx(&self.delivery_snapshot, pr_idx) else {
                    self.delivery_action_message = Some("BLOCK: no PR selected".into());
                    self.action_pane.last_result = Some(ActionResult {
                        action: "Block".into(),
                        outcome: ActionOutcome::Failed("no PR selected".into()),
                        at: now,
                    });
                    return;
                };
                tracing::info!(target: "tui.cockpit", pr = ctx.pr_number, reason = %reason, "block verdict requested");
                let summary = format!(
                    "operator BLOCK via TUI cockpit for PR #{}: {}",
                    ctx.pr_number, reason
                );
                let outcome = match adapter
                    .post_passport_check(
                        &ctx.repo_slug,
                        &ctx.head_sha,
                        crate::autonomy::types::GateDecision::Reject,
                        &summary,
                    )
                    .await
                {
                    Err(e) => {
                        self.delivery_action_message =
                            Some(format!("BLOCK failed at passport: {e}"));
                        ActionOutcome::Failed(e)
                    }
                    Ok(_) => match adapter
                        .post_mr_comment(
                            &ctx.repo_slug,
                            &ctx.pr_number.to_string(),
                            &format!("Agent: BLOCK — reason: {reason}"),
                        )
                        .await
                    {
                        Err(e) => {
                            self.delivery_action_message =
                                Some(format!("BLOCK failed at comment: {e}"));
                            ActionOutcome::Failed(e)
                        }
                        Ok(_) => {
                            let entry = helpers::ledger_entry(
                                crate::autonomy::types::LedgerKind::HumanDecisionRecorded,
                                &ctx,
                                serde_json::json!({
                                    "action": "block_verdict",
                                    "pr_number": ctx.pr_number,
                                    "reason": reason,
                                }),
                                now,
                            );
                            if let Err(e) = adapter.append_ledger(entry).await {
                                tracing::warn!(target: "tui.cockpit", err = %e, "block: ledger append failed");
                            }
                            self.delivery_action_message = Some(format!(
                                "BLOCK submitted for PR #{} — reason: {}",
                                ctx.pr_number, reason
                            ));
                            ActionOutcome::Submitted
                        }
                    },
                };
                self.action_pane.last_result = Some(ActionResult {
                    action: "Block".into(),
                    outcome,
                    at: now,
                });
            }
            DeliveryAction::RequestRepair { pr_idx } => {
                let Some(ctx) = helpers::pr_ctx(&self.delivery_snapshot, pr_idx) else {
                    self.delivery_action_message = Some("REPAIR: no PR selected".into());
                    self.action_pane.last_result = Some(ActionResult {
                        action: "Repair".into(),
                        outcome: ActionOutcome::Failed("no PR selected".into()),
                        at: now,
                    });
                    return;
                };
                let body = format!(
                    "Agent: please repair this MR per evidence pack {}",
                    ctx.head_sha
                );
                let outcome = match adapter
                    .post_mr_comment(&ctx.repo_slug, &ctx.pr_number.to_string(), &body)
                    .await
                {
                    Ok(_) => {
                        let entry = helpers::ledger_entry(
                            crate::autonomy::types::LedgerKind::HumanDecisionRecorded,
                            &ctx,
                            serde_json::json!({
                                "action": "request_repair",
                                "pr_number": ctx.pr_number,
                            }),
                            now,
                        );
                        if let Err(e) = adapter.append_ledger(entry).await {
                            tracing::warn!(target: "tui.cockpit", err = %e, "repair: ledger append failed");
                        }
                        self.delivery_action_message =
                            Some(format!("REPAIR requested for PR #{}", ctx.pr_number));
                        ActionOutcome::Submitted
                    }
                    Err(e) => {
                        self.delivery_action_message = Some(format!("REPAIR failed: {e}"));
                        ActionOutcome::Failed(e)
                    }
                };
                self.action_pane.last_result = Some(ActionResult {
                    action: "Repair".into(),
                    outcome,
                    at: now,
                });
            }
            DeliveryAction::FreezeAutonomy { hours } => {
                // Freeze is a control-plane edit (owned by ops); the TUI
                // only surfaces intent. No adapter call — just a signed
                // ledger event so the audit trail captures the operator's
                // request.
                tracing::info!(target: "tui.cockpit", hours, "freeze autonomy requested");
                let entry = helpers::ledger_entry_subject(
                    crate::autonomy::types::LedgerKind::HumanEscalationRequested,
                    "autonomy.freeze",
                    None,
                    serde_json::json!({
                        "action": "freeze_intent",
                        "hours": hours,
                    }),
                    now,
                );
                let outcome = match adapter.append_ledger(entry).await {
                    Ok(()) => {
                        self.delivery_action_message = Some(format!(
                            "FREEZE intent recorded for {}h — ops must finalize via CLI",
                            hours
                        ));
                        ActionOutcome::Submitted
                    }
                    Err(e) => {
                        self.delivery_action_message =
                            Some(format!("FREEZE ledger append failed: {e}"));
                        ActionOutcome::Failed(e)
                    }
                };
                self.action_pane.last_result = Some(ActionResult {
                    action: "Freeze".into(),
                    outcome,
                    at: now,
                });
            }
            DeliveryAction::KillBell { reason } => {
                // KillBell::pause already signs + appends a
                // `KillBellEngaged` ledger row; we DO NOT append again here.
                // See regression test
                // `kill_bell_action_does_not_double_append_ledger_entry`.
                tracing::warn!(target: "tui.cockpit", reason = %reason, "kill_bell engage requested");
                let outcome = match adapter
                    .pause_kill_bell(&reason, "tui.cockpit.v1", 86_400)
                    .await
                {
                    Ok(()) => {
                        self.delivery_snapshot.kill_bell_state = "paused".into();
                        self.delivery_action_message = Some(format!(
                            "KILL-BELL engaged — reason: {} (24h TTL; ops may resume)",
                            reason
                        ));
                        ActionOutcome::Submitted
                    }
                    Err(e) => {
                        self.delivery_action_message = Some(format!("KILL-BELL failed: {e}"));
                        ActionOutcome::Failed(e)
                    }
                };
                self.action_pane.last_result = Some(ActionResult {
                    action: "KillBell".into(),
                    outcome,
                    at: now,
                });
            }
        }
    }
}

#[path = "app_runtime_demo.rs"]
mod app_runtime_demo;

#[path = "app_runtime_sync.rs"]
mod app_runtime_sync;
