//! Owner: Interactive TUI subsystem — workflow DAG spatial navigation
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::nav`
//! Invariants: Navigation is pure state computation; never mutates workflow data.

use super::model::WorkflowSnapshot;

/// Layout constants for the virtual canvas.
pub const NODE_CARD_W: u16 = 34;
pub const NODE_CARD_H: u16 = 5;
pub const PHASE_HEADER_H: u16 = 1;
pub const EDGE_GUTTER_H: u16 = 3;
pub const BANNER_H: u16 = 4;

/// Detail-level zoom for node cards.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WorkflowZoom {
    /// Status + label only (one content row).
    Overview,
    /// Default: status, command, badges (two content rows).
    #[default]
    Cards,
    /// Status, label, and a single progress/badge row.
    Dense,
}

impl WorkflowZoom {
    pub fn next(self) -> Self {
        match self {
            Self::Overview => Self::Cards,
            Self::Cards => Self::Dense,
            Self::Dense => Self::Overview,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Cards => "cards",
            Self::Dense => "dense",
        }
    }
}

/// Navigation state for the workflow tab.
#[derive(Debug, Clone, Default)]
pub struct WorkflowNav {
    /// Currently selected phase index.
    pub phase_idx: usize,
    /// Currently selected node index within the phase.
    pub node_idx: usize,
    /// Virtual canvas vertical scroll offset (in terminal rows).
    pub viewport_y: i32,
    /// Virtual canvas horizontal scroll offset (in terminal cols).
    pub viewport_x: i32,
    /// When true, viewport auto-centers on the first running node.
    pub follow_active: bool,
    /// Cached total canvas height (set during render).
    pub canvas_height: i32,
    /// Cached total canvas width (set during render).
    pub canvas_width: i32,
    /// Detail-level zoom for node cards.
    pub zoom: WorkflowZoom,
}

impl WorkflowNav {
    /// Move to the next phase (down).
    pub fn down(&mut self, snap: &WorkflowSnapshot) {
        if self.phase_idx + 1 < snap.phases.len() {
            self.phase_idx += 1;
            self.clamp_node(snap);
        }
    }

    /// Move to the previous phase (up).
    pub fn up(&mut self, snap: &WorkflowSnapshot) {
        if self.phase_idx > 0 {
            self.phase_idx -= 1;
            self.clamp_node(snap);
        }
    }

    /// Move to the next sibling node (right).
    pub fn right(&mut self, snap: &WorkflowSnapshot) {
        if let Some(phase) = snap.phases.get(self.phase_idx)
            && self.node_idx + 1 < phase.node_ids.len()
        {
            self.node_idx += 1;
        }
    }

    /// Move to the previous sibling node (left).
    pub fn left(&mut self, _snap: &WorkflowSnapshot) {
        if self.node_idx > 0 {
            self.node_idx -= 1;
        }
    }

    /// Pan viewport down by 50% of the visible height.
    pub fn page_down(&mut self, visible_h: u16) {
        let jump = (visible_h as i32) / 2;
        self.viewport_y = (self.viewport_y + jump).min(self.max_viewport_y(visible_h));
    }

    /// Pan viewport up by 50% of the visible height.
    pub fn page_up(&mut self, visible_h: u16) {
        let jump = (visible_h as i32) / 2;
        self.viewport_y = (self.viewport_y - jump).max(0);
    }

    /// Pan viewport right by 50% of the visible width.
    pub fn page_right(&mut self, visible_w: u16) {
        let jump = (visible_w as i32) / 2;
        self.viewport_x = (self.viewport_x + jump).min(self.max_viewport_x(visible_w));
    }

    /// Pan viewport left by 50% of the visible width.
    pub fn page_left(&mut self, visible_w: u16) {
        let jump = (visible_w as i32) / 2;
        self.viewport_x = (self.viewport_x - jump).max(0);
    }

    /// Jump viewport to the top-left origin.
    pub fn home(&mut self) {
        self.viewport_y = 0;
        self.viewport_x = 0;
    }

    /// Jump viewport to the bottom of the DAG.
    pub fn end(&mut self, visible_h: u16) {
        self.viewport_y = self.max_viewport_y(visible_h);
    }

    /// Toggle follow-active mode. When enabled, viewport auto-pans to
    /// the first running node on each render frame.
    pub fn toggle_follow(&mut self) {
        self.follow_active = !self.follow_active;
    }

    /// Auto-pan viewport so the first running node is visible.
    pub fn follow_running(&mut self, snap: &WorkflowSnapshot, visible_h: u16, visible_w: u16) {
        // Find the first running node's phase.
        for (pi, phase) in snap.phases.iter().enumerate() {
            for (ni, nid) in phase.node_ids.iter().enumerate() {
                if let Some(node) = snap.node(nid)
                    && node.status.is_active()
                {
                    let node_y = self.phase_virtual_y(pi);
                    let node_x = (ni as i32) * (NODE_CARD_W as i32);

                    // Center the node in the viewport.
                    self.viewport_y =
                        (node_y - (visible_h as i32) / 2).clamp(0, self.max_viewport_y(visible_h));
                    self.viewport_x =
                        (node_x - (visible_w as i32) / 2).clamp(0, self.max_viewport_x(visible_w));
                    return;
                }
            }
        }
    }

    /// Ensure the currently selected node is visible in the viewport.
    /// Called after navigation moves the selection.
    pub fn ensure_selected_visible(&mut self, visible_h: u16, visible_w: u16) {
        let node_y = self.phase_virtual_y(self.phase_idx);
        let node_x = (self.node_idx as i32) * (NODE_CARD_W as i32);
        let phase_h = PHASE_HEADER_H as i32 + NODE_CARD_H as i32 + EDGE_GUTTER_H as i32;

        // Vertical: ensure node is visible.
        if node_y < self.viewport_y {
            self.viewport_y = node_y;
        } else if node_y + phase_h > self.viewport_y + visible_h as i32 {
            self.viewport_y = (node_y + phase_h - visible_h as i32).max(0);
        }

        // Horizontal: ensure node card is visible.
        let card_right = node_x + NODE_CARD_W as i32;
        if node_x < self.viewport_x {
            self.viewport_x = node_x;
        } else if card_right > self.viewport_x + visible_w as i32 {
            self.viewport_x = (card_right - visible_w as i32).max(0);
        }
    }

    /// Compute the virtual Y position of a phase in the canvas.
    pub fn phase_virtual_y(&self, phase_idx: usize) -> i32 {
        let mut y = BANNER_H as i32;
        for _ in 0..phase_idx {
            y += PHASE_HEADER_H as i32 + NODE_CARD_H as i32 + EDGE_GUTTER_H as i32;
        }
        y
    }

    /// Compute full canvas dimensions from the snapshot.
    pub fn compute_canvas_size(&mut self, snap: &WorkflowSnapshot) {
        let phase_count = snap.phases.len() as i32;
        let max_nodes_in_phase = snap
            .phases
            .iter()
            .map(|p| p.node_ids.len())
            .max()
            .unwrap_or(1) as i32;

        self.canvas_height = BANNER_H as i32
            + phase_count * (PHASE_HEADER_H as i32 + NODE_CARD_H as i32 + EDGE_GUTTER_H as i32);
        self.canvas_width = (max_nodes_in_phase * NODE_CARD_W as i32).max(80);
    }

    /// Restore selection by looking up a remembered node id in the new
    /// snapshot. Falls back to (0,0) when the id is no longer present.
    pub fn restore_selection(&mut self, snap: &WorkflowSnapshot, remembered: Option<&str>) {
        if let Some(id) = remembered
            && let Some((pi, ni)) = snap.locate_node(id)
        {
            self.phase_idx = pi;
            self.node_idx = ni;
            return;
        }
        // Fallback: clamp current coords against new snapshot.
        if self.phase_idx >= snap.phases.len() {
            self.phase_idx = 0;
            self.node_idx = 0;
            return;
        }
        self.clamp_node(snap);
    }

    /// Get the currently selected node ID.
    pub fn selected_node_id<'a>(&self, snap: &'a WorkflowSnapshot) -> Option<&'a str> {
        snap.phases
            .get(self.phase_idx)
            .and_then(|p| p.node_ids.get(self.node_idx))
            .map(|s| s.as_str())
    }

    /// Ensure node_idx is within bounds for the current phase.
    fn clamp_node(&mut self, snap: &WorkflowSnapshot) {
        if let Some(phase) = snap.phases.get(self.phase_idx) {
            if self.node_idx >= phase.node_ids.len() {
                self.node_idx = phase.node_ids.len().saturating_sub(1);
            }
        } else {
            self.node_idx = 0;
        }
    }

    fn max_viewport_y(&self, visible_h: u16) -> i32 {
        (self.canvas_height - visible_h as i32).max(0)
    }

    fn max_viewport_x(&self, visible_w: u16) -> i32 {
        (self.canvas_width - visible_w as i32).max(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::workflow::builder::build_demo_snapshot;

    #[test]
    fn navigation_basics() {
        let snap = build_demo_snapshot();
        let mut nav = WorkflowNav::default();

        // Start at phase 0, node 0.
        assert_eq!(nav.phase_idx, 0);
        assert_eq!(nav.node_idx, 0);

        // Move right within phase 0 (has 3 nodes: check, fmt, clippy).
        nav.right(&snap);
        assert_eq!(nav.node_idx, 1);
        nav.right(&snap);
        assert_eq!(nav.node_idx, 2);

        // Can't go past last node.
        nav.right(&snap);
        assert_eq!(nav.node_idx, 2);

        // Move down to phase 1.
        nav.down(&snap);
        assert_eq!(nav.phase_idx, 1);
        // node_idx should clamp if phase 1 has fewer nodes.
        assert!(nav.node_idx <= snap.phases[1].node_ids.len());
    }

    #[test]
    fn up_at_top_stays() {
        let snap = build_demo_snapshot();
        let mut nav = WorkflowNav::default();
        nav.up(&snap);
        assert_eq!(nav.phase_idx, 0);
    }

    #[test]
    fn left_at_zero_stays() {
        let snap = build_demo_snapshot();
        let mut nav = WorkflowNav::default();
        nav.left(&snap);
        assert_eq!(nav.node_idx, 0);
    }

    #[test]
    fn selected_node_id() {
        let snap = build_demo_snapshot();
        let nav = WorkflowNav::default();
        let id = nav.selected_node_id(&snap);
        assert!(id.is_some());
        // The first node in phase 0 should be one of check/fmt/clippy.
        let id = id.unwrap();
        assert!(
            id == "check" || id == "fmt" || id == "clippy",
            "unexpected first node: {}",
            id
        );
    }

    #[test]
    fn down_to_last_phase() {
        let snap = build_demo_snapshot();
        let mut nav = WorkflowNav::default();
        for _ in 0..20 {
            nav.down(&snap);
        }
        assert_eq!(nav.phase_idx, snap.phases.len() - 1);
    }

    #[test]
    fn page_down_pans_viewport() {
        let snap = build_demo_snapshot();
        let mut nav = WorkflowNav::default();
        nav.compute_canvas_size(&snap);
        assert_eq!(nav.viewport_y, 0);

        // Canvas is small enough that one 50% jump clamps at max.
        // max_viewport_y = canvas_height - visible_h
        let max_y = (nav.canvas_height - 40).max(0);
        nav.page_down(40);
        // Jump is min(20, max_y).
        assert_eq!(nav.viewport_y, 20_i32.min(max_y));

        // Should clamp at max.
        for _ in 0..20 {
            nav.page_down(40);
        }
        assert!(nav.viewport_y <= nav.canvas_height);
        assert_eq!(nav.viewport_y, max_y);
    }

    #[test]
    fn page_up_clamps_at_zero() {
        let nav = WorkflowNav {
            viewport_y: 10,
            canvas_height: 100,
            ..Default::default()
        };

        let mut nav = nav;
        nav.page_up(40);
        assert_eq!(nav.viewport_y, 0); // 10 - 20 = -10, clamped to 0
    }

    #[test]
    fn home_resets_viewport() {
        let mut nav = WorkflowNav {
            viewport_y: 50,
            viewport_x: 30,
            ..Default::default()
        };
        nav.home();
        assert_eq!(nav.viewport_y, 0);
        assert_eq!(nav.viewport_x, 0);
    }

    #[test]
    fn end_jumps_to_bottom() {
        let snap = build_demo_snapshot();
        let mut nav = WorkflowNav::default();
        nav.compute_canvas_size(&snap);
        nav.end(40);
        assert_eq!(nav.viewport_y, (nav.canvas_height - 40).max(0));
    }

    #[test]
    fn canvas_size_computed() {
        let snap = build_demo_snapshot();
        let mut nav = WorkflowNav::default();
        nav.compute_canvas_size(&snap);
        assert!(nav.canvas_height > 0);
        assert!(nav.canvas_width > 0);
    }

    #[test]
    fn horizontal_panning() {
        let mut nav = WorkflowNav {
            canvas_width: 200,
            canvas_height: 100,
            ..Default::default()
        };

        nav.page_right(80);
        assert_eq!(nav.viewport_x, 40); // 50% of 80

        nav.page_left(80);
        assert_eq!(nav.viewport_x, 0); // 40 - 40 = 0
    }

    #[test]
    fn restore_selection_finds_node_by_id() {
        let snap = build_demo_snapshot();
        // Demo snapshot has "merge-gate" in the last phase.
        let mut nav = WorkflowNav::default();
        nav.restore_selection(&snap, Some("merge-gate"));
        let id = nav.selected_node_id(&snap);
        assert_eq!(id, Some("merge-gate"));
    }

    #[test]
    fn restore_selection_resets_on_missing_id() {
        let snap = build_demo_snapshot();
        let mut nav = WorkflowNav {
            phase_idx: 99,
            node_idx: 99,
            ..Default::default()
        };
        nav.restore_selection(&snap, Some("no-such-node"));
        assert_eq!(nav.phase_idx, 0);
        assert_eq!(nav.node_idx, 0);
    }

    #[test]
    fn restore_selection_clamps_without_remembered_id() {
        let snap = build_demo_snapshot();
        let mut nav = WorkflowNav {
            phase_idx: snap.phases.len() + 5,
            node_idx: 99,
            ..Default::default()
        };
        nav.restore_selection(&snap, None);
        assert_eq!(nav.phase_idx, 0);
    }

    #[test]
    fn workflow_zoom_cycles_overview_cards_dense() {
        let mut z = WorkflowZoom::Overview;
        z = z.next();
        assert_eq!(z, WorkflowZoom::Cards);
        z = z.next();
        assert_eq!(z, WorkflowZoom::Dense);
        z = z.next();
        assert_eq!(z, WorkflowZoom::Overview);
    }
}
