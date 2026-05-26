//! Owner: Interactive TUI subsystem — Workflow DAG navigation and inspection.
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: Selection changes ensure the selected node remains visible; rollback
//! is policy-gated to Promote(dev|prod) nodes and defaults to dry-run.
use crate::tui::app::App;

impl App {
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
}
