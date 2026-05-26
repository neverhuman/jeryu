//! Owner: Inspection HTTP plane - GET /api/v1/repos + /families.
//! Proof: `cargo test -p jeryu --lib inspection::repos`
//! Invariants: routes return typed repo read-model projections wrapped in
//!             `InspectionEnvelope<T>`; registry projection is read-only.

use axum::Json;
use axum::extract::State;
use chrono::Utc;

use crate::api::inspection::InspectionEnvelope;
use crate::api::read_model::{RepoFamilySummary, ReposSnapshot};

use super::state::InspectionState;

pub async fn get_repos(
    State(state): State<InspectionState>,
) -> Json<InspectionEnvelope<ReposSnapshot>> {
    let repos = repos_for_state(&state);
    Json(InspectionEnvelope::new(
        repos,
        state.snapshot_sources(),
        Utc::now(),
    ))
}

pub async fn get_families(
    State(state): State<InspectionState>,
) -> Json<InspectionEnvelope<Vec<RepoFamilySummary>>> {
    let families = repos_for_state(&state).families;
    Json(InspectionEnvelope::new(
        families,
        state.snapshot_sources(),
        Utc::now(),
    ))
}

fn repos_for_state(state: &InspectionState) -> ReposSnapshot {
    repo_snapshot_source(state).into_snapshot()
}

enum RepoSnapshotSource {
    ReadModel(ReposSnapshot),
    Registry(ReposSnapshot),
    Empty,
}

impl RepoSnapshotSource {
    fn into_snapshot(self) -> ReposSnapshot {
        match self {
            Self::ReadModel(snapshot) | Self::Registry(snapshot) => snapshot,
            Self::Empty => ReposSnapshot::default(),
        }
    }
}

fn repo_snapshot_source(state: &InspectionState) -> RepoSnapshotSource {
    let snapshot = state.read_model().repos;
    if !snapshot.repos.is_empty() || !snapshot.families.is_empty() {
        return RepoSnapshotSource::ReadModel(snapshot);
    }
    if let Some(snapshot) = repos_from_workspace_registry() {
        RepoSnapshotSource::Registry(snapshot)
    } else {
        RepoSnapshotSource::Empty
    }
}

fn repos_from_workspace_registry() -> Option<ReposSnapshot> {
    let root = inspection_workspace_root()?;
    let registry = crate::repo_fleet::load_registry_from(&root).ok()?;
    Some(ReposSnapshot::from_registry(
        crate::repo_fleet::registry_path_for(&root)
            .display()
            .to_string(),
        &registry,
    ))
}

fn inspection_workspace_root() -> Option<std::path::PathBuf> {
    if let Ok(root) = std::env::var("JERYU_WORKSPACE_ROOT") {
        let root = std::path::PathBuf::from(root);
        if root.join(crate::repo_fleet::DEFAULT_REGISTRY_PATH).exists() {
            return Some(root);
        }
    }

    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(crate::repo_fleet::DEFAULT_REGISTRY_PATH).exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::inspection::INSPECTION_API_VERSION;
    use crate::api::read_model::{RepoSummary, TuiReadModel};
    use crate::api::runtime_profile::RuntimeProfile;

    fn state_with_repos() -> InspectionState {
        let mut model = TuiReadModel::default();
        let mut repo = RepoSummary::new("core", "neverhuman/jeryu");
        repo.running_count = 1;
        model.repos = ReposSnapshot::from_repo_summaries(".jeryu/repos.toml", vec![repo]);
        InspectionState::new(model, RuntimeProfile::new("test", "sqlite", "kafka"))
    }

    #[tokio::test]
    async fn repos_route_returns_state_snapshot() {
        let Json(envelope) = get_repos(State(state_with_repos())).await;
        assert_eq!(envelope.api_version, INSPECTION_API_VERSION);
        assert_eq!(envelope.data.repos[0].alias, "core");
        assert_eq!(envelope.data.families[0].name, "neverhuman");
    }

    #[tokio::test]
    async fn families_route_returns_family_list() {
        let Json(envelope) = get_families(State(state_with_repos())).await;
        assert_eq!(envelope.api_version, INSPECTION_API_VERSION);
        assert_eq!(envelope.data[0].name, "neverhuman");
        assert_eq!(envelope.data[0].running_count, 1);
    }
}
