use super::*;

impl App {
    pub fn cycle_tab_next(&mut self) {
        self.active_tab = match self.active_tab {
            ActiveTab::Workflow => ActiveTab::Mission,
            ActiveTab::Mission => ActiveTab::Release,
            ActiveTab::Release => ActiveTab::Approvals,
            ActiveTab::Approvals => ActiveTab::Jobs,
            ActiveTab::Jobs => ActiveTab::Agents,
            ActiveTab::Agents => ActiveTab::Tests,
            ActiveTab::Tests => ActiveTab::Pools,
            ActiveTab::Pools => ActiveTab::Cache,
            ActiveTab::Cache => ActiveTab::Evidence,
            ActiveTab::Evidence => ActiveTab::Secrets,
            ActiveTab::Secrets => ActiveTab::Git,
            ActiveTab::Git => ActiveTab::Workflow,
        };
    }

    pub fn cycle_pane_next(&mut self) {
        // Only Jobs is currently rendered; cycling to Pools/Pipelines would silently
        // focus invisible panes. Expand this when those panes are visible.
        self.active_pane = ActivePane::Jobs;
        self.update_log_target();
    }

    pub fn cycle_pane_prev(&mut self) {
        self.active_pane = ActivePane::Jobs;
        self.update_log_target();
    }

    pub fn up(&mut self) {
        if self.active_tab == ActiveTab::Tests {
            let limit = match self.test_view_mode {
                TestViewMode::Average => self.state.test_bottlenecks_avg.len(),
                TestViewMode::Latest => self.state.test_bottlenecks_latest.len(),
            };
            if limit > 0 {
                if self.selected_test_index > 0 {
                    self.selected_test_index -= 1;
                } else {
                    self.selected_test_index = limit - 1;
                }
                self.selected_test_history = None; // clear history when moving
            }
            return;
        }

        match self.active_pane {
            ActivePane::Pools => {
                if !self.state.pools.is_empty() {
                    if self.selected_pool_index > 0 {
                        self.selected_pool_index -= 1;
                    } else {
                        self.selected_pool_index = self.state.pools.len() - 1;
                    }
                }
            }
            ActivePane::Pipelines => {
                if !self.state.pipelines.is_empty() {
                    if self.selected_pipeline_index > 0 {
                        self.selected_pipeline_index -= 1;
                    } else {
                        self.selected_pipeline_index = self.state.pipelines.len() - 1;
                    }
                }
            }
            ActivePane::Jobs => {
                if !self.state.recent_jobs.is_empty() {
                    if self.selected_job_index > 0 {
                        self.selected_job_index -= 1;
                    } else {
                        self.selected_job_index = self.state.recent_jobs.len() - 1;
                    }
                    self.remember_selected_job();
                }
            }
        }
        self.update_log_target();
    }

    pub fn down(&mut self) {
        if self.active_tab == ActiveTab::Tests {
            let limit = match self.test_view_mode {
                TestViewMode::Average => self.state.test_bottlenecks_avg.len(),
                TestViewMode::Latest => self.state.test_bottlenecks_latest.len(),
            };
            if limit > 0 {
                self.selected_test_index = (self.selected_test_index + 1) % limit;
                self.selected_test_history = None; // clear history when moving
            }
            return;
        }

        match self.active_pane {
            ActivePane::Pools => {
                if !self.state.pools.is_empty() {
                    self.selected_pool_index =
                        (self.selected_pool_index + 1) % self.state.pools.len();
                }
            }
            ActivePane::Pipelines => {
                if !self.state.pipelines.is_empty() {
                    self.selected_pipeline_index =
                        (self.selected_pipeline_index + 1) % self.state.pipelines.len();
                }
            }
            ActivePane::Jobs => {
                if !self.state.recent_jobs.is_empty() {
                    self.selected_job_index =
                        (self.selected_job_index + 1) % self.state.recent_jobs.len();
                    self.remember_selected_job();
                }
            }
        }
        self.update_log_target();
    }

    pub(crate) fn update_log_target(&mut self) {
        if self.maximize_logs
            && let Some(job) = self.selected_job()
        {
            let target = Some(LogTarget {
                project_id: job.project_id,
                job_id: job.job_id,
            });
            if self.log_target != target {
                self.log_target = target;
                let _ = self.log_target_tx.send(target);
            }
            return;
        }
        if self.log_target.is_some() {
            self.log_target = None;
            let _ = self.log_target_tx.send(None);
        }
    }

    pub(crate) fn sync_selected_job_index(&mut self) {
        if self.state.recent_jobs.is_empty() {
            self.selected_job_index = 0;
            self.selected_job_id = None;
            return;
        }

        if let Some(job_id) = self.selected_job_id
            && let Some(index) = self
                .state
                .recent_jobs
                .iter()
                .position(|job| job.job_id == job_id)
        {
            self.selected_job_index = index;
            return;
        }

        if self.selected_job_index >= self.state.recent_jobs.len() {
            self.selected_job_index = self.state.recent_jobs.len() - 1;
        }
        self.remember_selected_job();
    }

    pub(crate) fn remember_selected_job(&mut self) {
        self.selected_job_id = self.selected_job().map(|job| job.job_id);
    }

    pub fn selected_job(&self) -> Option<&JobEvent> {
        self.state.recent_jobs.get(self.selected_job_index)
    }

    pub fn open_selected_job_log(&mut self) {
        self.active_pane = ActivePane::Jobs;
        self.remember_selected_job();
        self.maximize_logs = true;
        self.follow_log_tail = true;
        self.log_scroll_offset = u16::MAX;
        self.update_log_target();
    }

    pub fn close_log_view(&mut self) {
        self.maximize_logs = false;
        self.update_log_target();
    }

    pub fn scroll_logs_up(&mut self, amount: u16) {
        self.follow_log_tail = false;
        self.log_scroll_offset = self.log_scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_logs_down(&mut self, amount: u16) {
        self.follow_log_tail = false;
        self.log_scroll_offset = self.log_scroll_offset.saturating_add(amount);
    }

    pub fn follow_logs(&mut self) {
        self.follow_log_tail = true;
        self.log_scroll_offset = u16::MAX;
    }

    pub fn jump_logs_top(&mut self) {
        self.follow_log_tail = false;
        self.log_scroll_offset = 0;
    }

    pub async fn toggle_pool_paused(&mut self) -> Result<()> {
        if let Some(pool) = self.state.pools.get(self.selected_pool_index) {
            if pool.paused {
                crate::pool::resume_pool(&self.store, &self.gitlab, &pool.name).await?;
            } else {
                crate::pool::pause_pool(&self.store, &self.gitlab, &pool.name).await?;
            }
        }
        Ok(())
    }

    pub async fn remove_selected_item(&mut self) -> Result<()> {
        match self.active_pane {
            ActivePane::Pipelines => {
                if let Some(pm) = self.state.pipelines.get(self.selected_pipeline_index) {
                    let pid = pm.pipeline.pipeline_id;
                    self.store.delete_pipeline(pid).await?;
                    // Remove from local state immediately for snappy UX
                    self.state.pipelines.remove(self.selected_pipeline_index);
                    if self.selected_pipeline_index > 0 {
                        self.selected_pipeline_index -= 1;
                    }
                }
            }
            ActivePane::Jobs => {
                if let Some(j) = self.state.recent_jobs.get(self.selected_job_index) {
                    let jid = j.job_id;
                    self.store.delete_job_event(jid).await?;
                    self.state.recent_jobs.remove(self.selected_job_index);
                    if self.selected_job_index > 0 {
                        self.selected_job_index -= 1;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn requeue_selected_job(&mut self) -> Result<()> {
        if self.active_pane == ActivePane::Jobs
            && let Some(j) = self.state.recent_jobs.get(self.selected_job_index)
            && j.status == "failed"
        {
            self.gitlab.requeue_job(j.project_id, j.job_id).await?;
        }
        Ok(())
    }

    pub fn toggle_test_view_mode(&mut self) {
        self.test_view_mode = match self.test_view_mode {
            TestViewMode::Average => TestViewMode::Latest,
            TestViewMode::Latest => TestViewMode::Average,
        };
        self.selected_test_index = 0;
        self.selected_test_history = None;
    }

    pub async fn fetch_selected_test_history(&mut self) {
        let bottlenecks = match self.test_view_mode {
            TestViewMode::Average => &self.state.test_bottlenecks_avg,
            TestViewMode::Latest => &self.state.test_bottlenecks_latest,
        };
        if let Some(b) = bottlenecks.get(self.selected_test_index)
            && let Ok(hist) = self.store.get_test_history(&b.test_name, 50).await
        {
            self.selected_test_history = Some(hist);
        }
    }

    // -----------------------------------------------------------------------
    // TUI v2 — Runner feed controls
    // -----------------------------------------------------------------------

    pub fn feed_next(&mut self) {
        if !self.state.runner_feeds.is_empty() {
            self.state.active_feed_index =
                (self.state.active_feed_index + 1) % self.state.runner_feeds.len();
            self.feed_scroll_offset = 0;
            self.feed_follow_tail = true;
        }
    }

    pub fn feed_prev(&mut self) {
        if !self.state.runner_feeds.is_empty() {
            if self.state.active_feed_index > 0 {
                self.state.active_feed_index -= 1;
            } else {
                self.state.active_feed_index = self.state.runner_feeds.len() - 1;
            }
            self.feed_scroll_offset = 0;
            self.feed_follow_tail = true;
        }
    }

    pub fn feed_toggle_pin(&mut self) {
        if self.feed_pinned.is_some() {
            self.feed_pinned = None;
        } else {
            self.feed_pinned = Some(self.state.active_feed_index);
        }
    }

    pub fn feed_follow_toggle(&mut self) {
        self.feed_follow_tail = !self.feed_follow_tail;
        if self.feed_follow_tail {
            self.feed_scroll_offset = u16::MAX;
        }
    }

    // TUI v2 — Interactive actions

    pub async fn cancel_selected_job(&mut self) -> Result<()> {
        if let Some(j) = self.state.recent_jobs.get(self.selected_job_index) {
            self.gitlab.cancel_job(j.project_id, j.job_id).await?;
        }
        Ok(())
    }

    pub async fn force_refresh(&mut self) {
        self.refresh_now().await;
    }

    // -----------------------------------------------------------------------
    // Workflow DAG navigation
    // -----------------------------------------------------------------------

    pub fn workflow_up(&mut self) {
        self.workflow_nav.up(&self.workflow_snapshot);
        self.workflow_nav
            .ensure_selected_visible(self.last_dag_h(), self.last_dag_w());
    }

    pub fn workflow_down(&mut self) {
        self.workflow_nav.down(&self.workflow_snapshot);
        self.workflow_nav
            .ensure_selected_visible(self.last_dag_h(), self.last_dag_w());
    }

    pub fn workflow_left(&mut self) {
        self.workflow_nav.left(&self.workflow_snapshot);
        self.workflow_nav
            .ensure_selected_visible(self.last_dag_h(), self.last_dag_w());
    }

    pub fn workflow_right(&mut self) {
        self.workflow_nav.right(&self.workflow_snapshot);
        self.workflow_nav
            .ensure_selected_visible(self.last_dag_h(), self.last_dag_w());
    }

    /// Tab cycles to the next node in the current phase (wrapping).
    pub fn workflow_tab_next(&mut self) {
        if let Some(phase) = self
            .workflow_snapshot
            .phases
            .get(self.workflow_nav.phase_idx)
            && !phase.node_ids.is_empty()
        {
            self.workflow_nav.node_idx = (self.workflow_nav.node_idx + 1) % phase.node_ids.len();
        }
        self.workflow_nav
            .ensure_selected_visible(self.last_dag_h(), self.last_dag_w());
    }

    pub fn workflow_page_down(&mut self) {
        self.workflow_nav.page_down(self.last_dag_h());
    }

    pub fn workflow_page_up(&mut self) {
        self.workflow_nav.page_up(self.last_dag_h());
    }

    pub fn workflow_page_right(&mut self) {
        self.workflow_nav.page_right(self.last_dag_w());
    }

    pub fn workflow_page_left(&mut self) {
        self.workflow_nav.page_left(self.last_dag_w());
    }

    pub fn workflow_home(&mut self) {
        self.workflow_nav.home();
    }

    pub fn workflow_end(&mut self) {
        self.workflow_nav.end(self.last_dag_h());
    }

    pub fn workflow_toggle_follow(&mut self) {
        self.workflow_nav.toggle_follow();
        if self.workflow_nav.follow_active {
            self.workflow_nav.follow_running(
                &self.workflow_snapshot,
                self.last_dag_h(),
                self.last_dag_w(),
            );
        }
    }

    pub fn workflow_toggle_inspect(&mut self) {
        self.workflow_inspect_open = !self.workflow_inspect_open;
    }

    pub fn workflow_cycle_zoom(&mut self) {
        self.workflow_nav.zoom = self.workflow_nav.zoom.next();
    }

    /// Trigger a rollback for the selected node. When the node is a
    /// rollback-eligible Promote{Dev|Prod}, build a dry-run RollbackReport
    /// from the release ladder and surface a confirmation message. Real
    /// production rollback requires an operator step (see docs/release-policy).
    pub fn workflow_trigger_rollback(&mut self) {
        use crate::tui::workflow::model::WorkflowNodeKind;
        let Some(node_id) = self
            .workflow_nav
            .selected_node_id(&self.workflow_snapshot)
            .map(str::to_string)
        else {
            self.delivery_action_message = Some("rollback: no node selected".into());
            return;
        };
        let node = match self.workflow_snapshot.node(&node_id) {
            Some(n) => n,
            None => {
                self.delivery_action_message = Some("rollback: node not found".into());
                return;
            }
        };
        if !node.kind.is_rollback_eligible() {
            self.delivery_action_message = Some(format!(
                "rollback unavailable for {} — select a Promote(dev|prod) node",
                node.label
            ));
            return;
        }
        let env = match node.kind {
            WorkflowNodeKind::Promote { env } => env.label(),
            _ => "?",
        };
        let pr_num = self
            .delivery_snapshot
            .selected()
            .map(|p| p.number)
            .unwrap_or(0);
        let report = crate::release::build_report(
            &format!("PR-{}", pr_num),
            &format!("TUI-initiated rollback for {} → {}", node.label, env),
            true, // dry-run by default — operator confirms via release tab
        );
        self.delivery_action_message = Some(format!(
            "ROLLBACK scheduled: {} steps in ladder (dry-run); finalize via `jeryu release rollback` or the Release tab",
            report.steps.len()
        ));
    }

    pub fn inspector_cycle_next(&mut self) {
        self.inspector_tab = self.inspector_tab.next();
    }

    pub fn inspector_cycle_prev(&mut self) {
        self.inspector_tab = self.inspector_tab.prev();
    }

    /// Jump selection to the first blocker (failing/blocked node) in the
    /// current PR's pipeline. No-op when nothing is blocked.
    pub fn workflow_jump_to_blocker(&mut self) {
        use crate::tui::workflow::intelligence::compute_first_blocker;
        if let Some(node) = compute_first_blocker(&self.workflow_snapshot)
            && let Some((pi, ni)) = self.workflow_snapshot.locate_node(&node.id)
        {
            self.workflow_nav.phase_idx = pi;
            self.workflow_nav.node_idx = ni;
            self.workflow_nav
                .ensure_selected_visible(self.last_dag_h(), self.last_dag_w());
        }
    }

    /// Jump selection to the tail (furthest-out) node on the critical path.
    pub fn workflow_jump_to_critical_head(&mut self) {
        use crate::tui::workflow::intelligence::compute_critical_path;
        let path = compute_critical_path(&self.workflow_snapshot);
        if let Some(tail) = path.last()
            && let Some((pi, ni)) = self.workflow_snapshot.locate_node(tail)
        {
            self.workflow_nav.phase_idx = pi;
            self.workflow_nav.node_idx = ni;
            self.workflow_nav
                .ensure_selected_visible(self.last_dag_h(), self.last_dag_w());
        }
    }

    /// Cycle to the next pull request in the Delivery view.
    pub fn delivery_next_pr(&mut self) {
        self.delivery_snapshot.next_pr();
        // Mirror the new PR's pipeline and reset nav to its current node.
        if let Some(pr) = self.delivery_snapshot.selected() {
            self.workflow_snapshot = pr.snapshot.clone();
            self.workflow_nav.phase_idx = 0;
            self.workflow_nav.node_idx = 0;
            self.workflow_nav
                .compute_canvas_size(&self.workflow_snapshot);
            if let Some(cn) = pr.current_node_id.clone()
                && let Some((pi, ni)) = self.workflow_snapshot.locate_node(&cn)
            {
                self.workflow_nav.phase_idx = pi;
                self.workflow_nav.node_idx = ni;
            }
            self.workflow_nav
                .ensure_selected_visible(self.last_dag_h(), self.last_dag_w());
        }
    }

    /// Cycle to the previous pull request in the Delivery view.
    pub fn delivery_prev_pr(&mut self) {
        self.delivery_snapshot.prev_pr();
        if let Some(pr) = self.delivery_snapshot.selected() {
            self.workflow_snapshot = pr.snapshot.clone();
            self.workflow_nav.phase_idx = 0;
            self.workflow_nav.node_idx = 0;
            self.workflow_nav
                .compute_canvas_size(&self.workflow_snapshot);
            if let Some(cn) = pr.current_node_id.clone()
                && let Some((pi, ni)) = self.workflow_snapshot.locate_node(&cn)
            {
                self.workflow_nav.phase_idx = pi;
                self.workflow_nav.node_idx = ni;
            }
            self.workflow_nav
                .ensure_selected_visible(self.last_dag_h(), self.last_dag_w());
        }
    }

    /// Rebuild the workflow snapshot from the collector (called on tick).
    pub fn refresh_workflow_snapshot(&mut self) {
        self.refresh_delivery_snapshot();
    }

    /// Rebuild the Delivery (multi-PR) snapshot, mirror the selected PR's
    /// per-pipeline DAG into `workflow_snapshot` for the legacy nav/render
    /// codepath, and reapply persistent selection + follow-active.
    pub fn refresh_delivery_snapshot(&mut self) {
        use crate::tui::workflow::delivery::build_demo_delivery;

        // Remember the previously focused node id so selection survives the
        // rebuild (panes/cards may reshuffle as live data arrives).
        let remembered_node_id = self
            .workflow_nav
            .selected_node_id(&self.workflow_snapshot)
            .map(str::to_string);
        let remembered_pr = self.delivery_snapshot.selected().map(|pr| pr.number);

        // TODO: when live PR/CI data is wired, plug collect_delivery_snapshot
        // with PrInput from the GitLab + agent layer here. Until then the
        // demo factory tells the canonical 5-PR story.
        if self.delivery_snapshot.pull_requests.is_empty() {
            self.delivery_snapshot = build_demo_delivery();
        }
        if let Some(num) = remembered_pr {
            self.delivery_snapshot.select_by_number(num);
        }

        // Mirror the currently selected PR's per-pipeline DAG into the
        // legacy workflow_snapshot so the existing WorkflowNav helpers keep
        // operating on the right data.
        if let Some(pr) = self.delivery_snapshot.selected() {
            self.workflow_snapshot = pr.snapshot.clone();
        }

        self.workflow_nav
            .compute_canvas_size(&self.workflow_snapshot);
        self.workflow_nav
            .restore_selection(&self.workflow_snapshot, remembered_node_id.as_deref());

        if self.workflow_nav.follow_active {
            self.workflow_nav.follow_running(
                &self.workflow_snapshot,
                self.last_dag_h(),
                self.last_dag_w(),
            );
        }
    }

    /// Approximate visible DAG height (terminal height minus chrome).
    /// Used for viewport panning calculations.
    fn last_dag_h(&self) -> u16 {
        // Header(3) + Banner(4) + EventConsole(4) + Footer(2) = 13 lines of chrome
        // Remaining is DAG area; default 40 row terminal = ~27 usable.
        30 // safe default; actual is set during render
    }

    fn last_dag_w(&self) -> u16 {
        120 // safe default
    }
}
