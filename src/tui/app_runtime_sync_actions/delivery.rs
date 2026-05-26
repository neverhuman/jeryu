//! Owner: Interactive TUI subsystem — Delivery PR cycling & snapshot mirror
//! Proof: `cargo nextest run -p jeryu --lib tui::`
//! Invariants: selecting/refreshing a PR mirrors the active per-pipeline DAG
//! into `workflow_snapshot` and resets DAG navigation to the PR's current
//! node so the legacy workflow renderer keeps operating on the right data.

use super::*;

impl App {
    /// Cycle to the next pull request in the Delivery view.
    pub fn delivery_next_pr(&mut self) {
        let filter = self.repo_filter().to_owned();
        self.delivery_snapshot.next_pr_matching(|pr| {
            filter.matches(pr.repo_alias.as_deref(), pr.repo_slug.as_deref())
        });
        self.mirror_selected_pr_into_workflow();
    }

    /// Cycle to the previous pull request in the Delivery view.
    pub fn delivery_prev_pr(&mut self) {
        let filter = self.repo_filter().to_owned();
        self.delivery_snapshot.prev_pr_matching(|pr| {
            filter.matches(pr.repo_alias.as_deref(), pr.repo_slug.as_deref())
        });
        self.mirror_selected_pr_into_workflow();
    }

    /// Reflect the currently-selected PR's pipeline into `workflow_snapshot`
    /// and reset DAG nav to its current node. Shared by `delivery_next_pr`,
    /// `delivery_prev_pr`, and `refresh_delivery_snapshot`.
    fn mirror_selected_pr_into_workflow(&mut self) {
        let Some(pr) = self.delivery_snapshot.selected() else {
            return;
        };
        self.workflow_snapshot = pr.snapshot.clone();
        self.workflow_nav.phase_idx = 0;
        self.workflow_nav.node_idx = 0;
        self.workflow_nav
            .compute_canvas_size(&self.workflow_snapshot);
        let current_node = pr.current_node_id.clone();
        if let Some(cn) = current_node
            && let Some((pi, ni)) = self.workflow_snapshot.locate_node(&cn)
        {
            self.workflow_nav.phase_idx = pi;
            self.workflow_nav.node_idx = ni;
        }
        let dh = self.last_dag_h();
        let dw = self.last_dag_w();
        self.workflow_nav.ensure_selected_visible(dh, dw);
    }

    /// Rebuild the workflow snapshot from the collector (called on tick).
    pub fn refresh_workflow_snapshot(&mut self) {
        self.refresh_delivery_snapshot();
    }

    /// Rebuild the Delivery (multi-PR) snapshot, mirror the selected PR's
    /// per-pipeline DAG into `workflow_snapshot` for the legacy nav/render
    /// codepath, and reapply persistent selection + follow-active.
    pub fn refresh_delivery_snapshot(&mut self) {
        // Remember the previously focused node id so selection survives the
        // rebuild (panes/cards may reshuffle as live data arrives).
        let remembered_node_id = self
            .workflow_nav
            .selected_node_id(&self.workflow_snapshot)
            .map(str::to_string);
        let remembered_pr = self.delivery_snapshot.selected().map(|pr| pr.number);

        // Live PR/CI integration plugs in here: convert PrInput from the
        // GitHub/GitLab + agent layer into delivery_snapshot. When no source
        // is configured the snapshot stays empty and the workflow tab
        // renders an explicit empty-state card. Demo mode seeds the snapshot
        // via `App::apply_demo_fixture`.
        if let Some(num) = remembered_pr {
            self.delivery_snapshot.select_by_number(num);
        }

        // Repo filter: if the selected PR no longer matches the active filter
        // (e.g. user just switched repos in the fleet bar), jump to the first
        // PR that does. PRs in other repos remain in the snapshot but are
        // hidden by render-time filtering in pr_rail.
        let filter = self.repo_filter().to_owned();
        self.delivery_snapshot.ensure_selection_matches(|pr| {
            filter.matches(pr.repo_alias.as_deref(), pr.repo_slug.as_deref())
        });

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

    /// Replace the live delivery snapshot while preserving the current PR
    /// selection when the same PR still exists in the new snapshot.
    pub fn set_delivery_snapshot(
        &mut self,
        snapshot: crate::tui::workflow::model::DeliverySnapshot,
        source_status: crate::tui::app::DeliverySourceStatus,
    ) {
        let remembered_pr = self.delivery_snapshot.selected().map(|pr| pr.number);
        self.delivery_snapshot = snapshot;
        if let Some(num) = remembered_pr {
            self.delivery_snapshot.select_by_number(num);
        }
        self.state.delivery_source_status = source_status;
        self.refresh_delivery_snapshot();
    }
}
