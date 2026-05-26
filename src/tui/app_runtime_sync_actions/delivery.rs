//! Owner: Interactive TUI subsystem — Delivery PR rotation and snapshot refresh.
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: PR rotation mirrors the active PR's pipeline into `workflow_snapshot`
//! and preserves persistent node selection across rebuilds.
use crate::tui::app::App;

impl App {
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
    pub(crate) fn last_dag_h(&self) -> u16 {
        // Header(3) + Banner(4) + EventConsole(4) + Footer(2) = 13 lines of chrome
        // Remaining is DAG area; default 40 row terminal = ~27 usable.
        30 // safe default; actual is set during render
    }

    pub(crate) fn last_dag_w(&self) -> u16 {
        120 // safe default
    }
}
