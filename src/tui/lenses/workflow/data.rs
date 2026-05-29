//! Owner: Interactive TUI subsystem - Workflow lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::workflow::data`
//! Invariants: Pure projection from `TuiReadModel` to `WorkflowLensInput`.
//!             No I/O. Owned data (no lifetimes). Render layer reads only the
//!             resulting struct. The Workflow Atlas projects the delivery /
//!             pipeline flow across the repo fleet: per-repo posture rows plus
//!             the fleet-wide running/failing job aggregates and overall health.

use crate::api::entity::HealthLevel;
use crate::api::read_model::{RepoSummary, TuiReadModel};

/// One repo row in the Workflow Atlas delivery table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRepoRow {
    pub alias: String,
    pub family: String,
    /// Delivery posture word derived from the repo's job counts / status.
    pub posture: String,
    pub running_count: u32,
    pub failed_count: u32,
}

impl WorkflowRepoRow {
    fn from_summary(repo: &RepoSummary) -> Self {
        Self {
            alias: repo.alias.clone(),
            family: repo.family.clone(),
            posture: posture_for(repo),
            running_count: repo.running_count,
            failed_count: repo.failed_count,
        }
    }

    /// True when this repo is actively delivering or blocked — used to size the
    /// "active delivery workflows" empty-state.
    pub fn is_active(&self) -> bool {
        self.running_count > 0 || self.failed_count > 0
    }
}

/// Low-noise delivery posture: failing > running > stale > idle.
fn posture_for(repo: &RepoSummary) -> String {
    if repo.failed_count > 0 {
        "FAILING".into()
    } else if repo.running_count > 0 {
        "running".into()
    } else if repo.aged {
        "stale".into()
    } else {
        "idle".into()
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowLensInput {
    /// Per-repo delivery rows projected from `model.repos`.
    pub repos: Vec<WorkflowRepoRow>,
    /// Number of distinct repo families in the fleet.
    pub family_count: u32,
    /// Fleet-wide running jobs (`model.mission.running_jobs`).
    pub running_jobs: u32,
    /// Fleet-wide failing jobs (`model.mission.failed_jobs`).
    pub failed_jobs: u32,
    /// Overall mission health (`model.mission.overall`).
    pub overall: HealthLevel,
    pub event_cursor: u64,
}

impl WorkflowLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let repos: Vec<WorkflowRepoRow> = model
            .repos
            .repos
            .iter()
            .map(WorkflowRepoRow::from_summary)
            .collect();
        Self {
            repos,
            family_count: model.repos.families.len() as u32,
            running_jobs: model.mission.running_jobs,
            failed_jobs: model.mission.failed_jobs,
            overall: model.mission.overall,
            event_cursor: model.event_cursor,
        }
    }

    pub fn repo_count(&self) -> u32 {
        self.repos.len() as u32
    }

    /// Count of repos with live or failing delivery activity.
    pub fn active_count(&self) -> u32 {
        self.repos.iter().filter(|r| r.is_active()).count() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::ReposSnapshot;

    #[test]
    fn select_from_default_read_model_returns_empty_input() {
        let model = TuiReadModel::default();
        let input = WorkflowLensInput::from_read_model(&model);
        assert_eq!(input.repo_count(), 0);
        assert_eq!(input.family_count, 0);
        assert_eq!(input.running_jobs, 0);
        assert_eq!(input.failed_jobs, 0);
        assert_eq!(input.active_count(), 0);
        assert!(input.repos.is_empty());
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        let input = WorkflowLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 42);
    }

    #[test]
    fn projects_repo_rows_and_job_aggregates() {
        let mut model = TuiReadModel::default();
        let mut failing = RepoSummary::new("core", "neverhuman/jeryu");
        failing.failed_count = 2;
        let mut running = RepoSummary::new("docs", "neverhuman/docs");
        running.running_count = 1;
        model.repos =
            ReposSnapshot::from_repo_summaries(".jeryu/repos.toml", vec![failing, running]);
        model.mission.running_jobs = 5;
        model.mission.failed_jobs = 2;

        let input = WorkflowLensInput::from_read_model(&model);
        assert_eq!(input.repo_count(), 2);
        assert_eq!(input.family_count, 1);
        assert_eq!(input.running_jobs, 5);
        assert_eq!(input.failed_jobs, 2);
        assert_eq!(input.active_count(), 2);
        assert_eq!(input.repos[0].posture, "FAILING");
        assert_eq!(input.repos[1].posture, "running");
    }
}
