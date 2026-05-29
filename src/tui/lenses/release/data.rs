//! Owner: Interactive TUI subsystem - Release lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::release::data`
//! Invariants: Pure projection from `TuiReadModel` to `ReleaseLensInput`.
//!             No I/O. Clones owned data so the render layer needs no lifetimes.

use crate::api::entity::HealthLevel;
use crate::api::read_model::TuiReadModel;

/// One release-tracked repository row: name plus a readiness/status cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseRepoRow {
    pub alias: String,
    pub slug: String,
    /// Repo status word from the fleet snapshot (e.g. running/failed/green).
    pub status: String,
    pub failed_count: u32,
    pub aged: bool,
}

impl ReleaseRepoRow {
    /// Low-noise readiness word: a failed job or aged checkout blocks release.
    pub fn readiness(&self) -> &'static str {
        if self.failed_count > 0 {
            "BLOCKED"
        } else if self.aged {
            "stale"
        } else {
            "ready"
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReleaseLensInput {
    pub safe_to_release: bool,
    pub overall: HealthLevel,
    pub open_capsules: u32,
    pub repos: Vec<ReleaseRepoRow>,
    pub event_cursor: u64,
}

impl ReleaseLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let repos = model
            .repos
            .repos
            .iter()
            .map(|repo| ReleaseRepoRow {
                alias: repo.alias.clone(),
                slug: repo.slug.clone(),
                status: repo.status.clone(),
                failed_count: repo.failed_count,
                aged: repo.aged,
            })
            .collect();
        Self {
            safe_to_release: model.mission.safe_to_release,
            overall: model.mission.overall,
            open_capsules: model.mission.open_capsules,
            repos,
            event_cursor: model.event_cursor,
        }
    }

    /// Count of repos whose readiness blocks a release — drives header emphasis.
    pub fn blocked_repos(&self) -> usize {
        self.repos
            .iter()
            .filter(|r| r.readiness() == "BLOCKED")
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::{RepoSummary, ReposSnapshot};

    #[test]
    fn select_from_default_read_model_is_safe_defaults() {
        let input = ReleaseLensInput::from_read_model(&TuiReadModel::default());
        assert!(!input.safe_to_release);
        assert_eq!(input.open_capsules, 0);
        assert!(input.repos.is_empty());
        assert_eq!(input.blocked_repos(), 0);
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn select_preserves_event_cursor_and_repos() {
        let mut repo = RepoSummary::new("core", "neverhuman/jeryu");
        repo.status = "running".into();
        repo.failed_count = 2;
        let mut model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        model.mission.safe_to_release = true;
        model.mission.open_capsules = 3;
        model.repos = ReposSnapshot::from_repo_summaries(".jeryu/repos.toml", vec![repo]);

        let input = ReleaseLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 42);
        assert!(input.safe_to_release);
        assert_eq!(input.open_capsules, 3);
        assert_eq!(input.repos.len(), 1);
        assert_eq!(input.repos[0].alias, "core");
        assert_eq!(input.repos[0].slug, "neverhuman/jeryu");
        assert_eq!(input.repos[0].readiness(), "BLOCKED");
        assert_eq!(input.blocked_repos(), 1);
    }

    #[test]
    fn readiness_classifies_states() {
        let ready = ReleaseRepoRow {
            alias: "a".into(),
            slug: "x/a".into(),
            status: "green".into(),
            failed_count: 0,
            aged: false,
        };
        assert_eq!(ready.readiness(), "ready");

        let stale = ReleaseRepoRow {
            aged: true,
            ..ready.clone()
        };
        assert_eq!(stale.readiness(), "stale");

        let blocked = ReleaseRepoRow {
            failed_count: 1,
            ..ready
        };
        assert_eq!(blocked.readiness(), "BLOCKED");
    }
}
