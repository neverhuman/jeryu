//! Owner: Interactive TUI subsystem - Repos lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::repos::data`
//! Invariants: Pure projection from `TuiReadModel` or fleet snapshot.

use crate::api::read_model::{RepoFamilySummary, RepoSummary, ReposSnapshot, TuiReadModel};

#[derive(Debug, Clone)]
pub struct ReposLensInput {
    pub repos: ReposSnapshot,
    pub event_cursor: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepoFleetCounts {
    pub repos: u32,
    pub families: u32,
    pub running: u32,
    pub failed: u32,
    pub aged: u32,
}

impl ReposLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            repos: model.repos.clone(),
            event_cursor: model.event_cursor,
        }
    }

    pub fn from_fleet_snapshot(snapshot: &crate::repo_fleet::FleetSnapshot) -> Self {
        Self {
            repos: ReposSnapshot::from_fleet_snapshot(snapshot),
            event_cursor: 0,
        }
    }

    pub fn counts(&self) -> RepoFleetCounts {
        let (running, failed, aged) = self.repos.counts();
        RepoFleetCounts {
            repos: self.repos.repos.len() as u32,
            families: self.families().len() as u32,
            running,
            failed,
            aged,
        }
    }

    pub fn families(&self) -> Vec<RepoFamilySummary> {
        self.repos.families.clone()
    }

    pub fn repositories(&self) -> &[RepoSummary] {
        &self.repos.repos
    }

    pub fn selected_repo(&self) -> Option<&RepoSummary> {
        self.repos.repos.first()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_returns_zero_repos() {
        let model = TuiReadModel::default();
        let input = ReposLensInput::from_read_model(&model);
        assert_eq!(input.counts().repos, 0);
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn select_preserves_event_cursor_and_repo_counts() {
        let mut model = TuiReadModel {
            event_cursor: 9012,
            ..Default::default()
        };
        let mut repo = RepoSummary::new("core", "neverhuman/jeryu");
        repo.running_count = 1;
        model.repos = ReposSnapshot::from_repo_summaries(".jeryu/repos.toml", vec![repo]);

        let input = ReposLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 9012);
        assert_eq!(input.counts().repos, 1);
        assert_eq!(input.counts().running, 1);
        assert_eq!(input.families()[0].name, "neverhuman");
    }
}
