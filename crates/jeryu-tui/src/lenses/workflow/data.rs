//! Workflow lens data selector.
//!
//! Invariants: pure projection from [`TuiReadModel`] to [`WorkflowLensInput`].
//! No I/O. The Workflow Atlas projects delivery posture per repo from the read
//! model's workflow dashboard: per-pipeline rows (repo, driving PR number,
//! posture, critical-path node) plus the fleet rollup from the dashboard
//! summary. GitHub PR shape: pipelines are keyed by PR number.

use jeryu_readmodel::{DeliveryPosture, TuiReadModel, WorkflowItem};

/// One delivery-pipeline row in the Workflow Atlas.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowRow {
    pub pipeline_id: String,
    pub label: String,
    pub repo_slug: String,
    pub pr_number: Option<u64>,
    pub posture: DeliveryPosture,
    pub critical_path_node: Option<String>,
}

impl WorkflowRow {
    fn from_item(item: &WorkflowItem) -> Self {
        Self {
            pipeline_id: item.pipeline_id.clone(),
            label: item.label.clone(),
            repo_slug: item.repo_slug.clone(),
            pr_number: item.pr_number,
            posture: item.posture,
            critical_path_node: item.critical_path_node.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WorkflowLensInput {
    pub rows: Vec<WorkflowRow>,
    pub running_count: u32,
    pub blocked_count: u32,
    pub longest_running_seconds: u64,
    pub event_cursor: u64,
}

impl WorkflowLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let summary = model.workflow.summary.as_ref();
        let rows: Vec<WorkflowRow> = model
            .workflow
            .items
            .iter()
            .map(WorkflowRow::from_item)
            .collect();
        let blocked_fallback = rows
            .iter()
            .filter(|r| r.posture == DeliveryPosture::Blocked)
            .count() as u32;
        let running_fallback = rows
            .iter()
            .filter(|r| r.posture == DeliveryPosture::Running)
            .count() as u32;
        Self {
            running_count: summary.map(|s| s.running_count).unwrap_or(running_fallback),
            blocked_count: summary.map(|s| s.blocked_count).unwrap_or(blocked_fallback),
            longest_running_seconds: summary.map(|s| s.longest_running_seconds).unwrap_or(0),
            rows,
            event_cursor: model.event_cursor,
        }
    }

    pub fn pipeline_count(&self) -> u32 {
        self.rows.len() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::sample_read_model;

    #[test]
    fn empty_from_default_read_model() {
        let input = WorkflowLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.pipeline_count(), 0);
        assert_eq!(input.running_count, 0);
        assert_eq!(input.blocked_count, 0);
        assert!(input.rows.is_empty());
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn projects_pipelines_from_sample() {
        let model = sample_read_model();
        let input = WorkflowLensInput::from_read_model(&model);
        assert_eq!(input.pipeline_count(), 2);
        assert_eq!(input.rows[0].repo_slug, "core/web");
        assert_eq!(input.rows[0].pr_number, Some(101));
        assert_eq!(input.rows[0].posture, DeliveryPosture::Running);
        assert_eq!(input.rows[1].posture, DeliveryPosture::Blocked);
        assert_eq!(input.running_count, 1);
        assert_eq!(input.blocked_count, 1);
        assert_eq!(input.longest_running_seconds, 2_840);
        assert_eq!(input.event_cursor, 42);
    }

    #[test]
    fn derives_counts_without_summary() {
        let mut model = TuiReadModel::default();
        let mut blocked = WorkflowItem::new("pipe-1", "core/web");
        blocked.posture = DeliveryPosture::Blocked;
        let mut running = WorkflowItem::new("pipe-2", "core/api");
        running.posture = DeliveryPosture::Running;
        model.workflow.items = vec![blocked, running];
        let input = WorkflowLensInput::from_read_model(&model);
        assert_eq!(input.blocked_count, 1);
        assert_eq!(input.running_count, 1);
    }
}
