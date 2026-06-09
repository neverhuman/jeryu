use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};

use crate::web::WebState;

use super::*;

pub(crate) async fn status(State(state): State<Arc<WebState>>) -> Json<ControlPlaneSnapshot> {
    Json(snapshot(&state))
}

pub(crate) async fn priorities(
    State(state): State<Arc<WebState>>,
    Query(query): Query<PriorityQuery>,
) -> Json<Vec<PriorityInsight>> {
    let mut priorities = snapshot(&state).priorities;
    if let Some(limit) = query.limit {
        priorities.truncate(limit.max(1));
    }
    Json(priorities)
}

pub(crate) async fn repo_graph(
    State(state): State<Arc<WebState>>,
    Query(query): Query<RepoGraphQuery>,
) -> Json<RepoGraphResponse> {
    Json(repo_graph_response(&state, Some(query)))
}

pub(crate) async fn artifacts_latest(
    State(state): State<Arc<WebState>>,
) -> Json<ArtifactLatestResponse> {
    Json(artifacts(&state))
}

pub(crate) async fn runners(State(state): State<Arc<WebState>>) -> Json<RunnerFabricResponse> {
    Json(runner_fabric(&state))
}
