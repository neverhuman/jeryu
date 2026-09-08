//! Builder/fixture for a populated sample [`TuiReadModel`].
//!
//! Used by `--demo`, screenshots, and serde round-trip tests. The builder is
//! deterministic given a fixed `generated_at` so snapshots are stable.

mod builder;
mod dashboards;
mod model;

pub use builder::TuiReadModelBuilder;
pub use dashboards::{
    sample_agent_runs, sample_agents, sample_approvals, sample_codegraph, sample_evidence,
    sample_release, sample_workcells, sample_workflow,
};
pub use model::sample_read_model;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::freshness::SourceKind;

    #[test]
    fn sample_is_populated() {
        let m = sample_read_model();
        assert_eq!(m.event_cursor, 42);
        assert_eq!(m.mission.active_agents, 4);
        assert!(m.next_action.is_some());
        assert_eq!(m.system.scm.name, "scm");
        assert_eq!(m.runners.items.len(), 1);
        assert_eq!(m.source_doctor.items[0].source_kind, SourceKind::Scm);
        assert_eq!(m.approvals.items.len(), 2);
        assert_eq!(m.approvals.failing_checks(), 1);
        assert_eq!(m.evidence.denied(), 1);
        assert_eq!(m.agents.blocked(), 1);
        assert_eq!(m.release.blocked(), 1);
        assert_eq!(m.workcells.blocked(), 1);
        assert_eq!(m.workcells.claimed(), 1);
        assert_eq!(m.workcells.held(), 1);
        assert_eq!(m.workflow.blocked(), 1);
    }

    #[test]
    fn builder_overrides_defaults() {
        let m = TuiReadModelBuilder::new().event_cursor(7).build();
        assert_eq!(m.event_cursor, 7);
        assert_eq!(m.schema_version, "tui.v1.0");
    }
}
