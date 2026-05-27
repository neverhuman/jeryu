//! REST handlers for agents (W-B-31 — read-only).
//!
//! Exposes (per §35.7 / FINAL §6.10):
//!   - `GET /api/v1/repos/{repo_id}/agents/sessions`  list sessions
//!   - `GET /api/v1/repos/{repo_id}/agents/evidence`  evidence list
//!
//! Phase 4 returns the existing TUI agent-session shape unchanged (empty
//! Vec when no sessions exist) and a placeholder empty evidence list.
//! Writes are NOT implemented in Phase 4 — wiring lands with W-B-31′.

use axum::{
    Extension, Json,
    extract::{Path, State},
};
use serde::Serialize;

use jeryu::api::agent_session::AgentSession;

use crate::repos::models::RepoId;
use crate::web::auth::Viewer;
use crate::web::error::ApiError;
use crate::web::permissions::{perms, require};
use crate::web::state::WebState;

#[derive(Debug, Serialize)]
pub struct AgentSessionsResponse {
    pub repo_id: String,
    pub sessions: Vec<AgentSession>,
}

#[derive(Debug, Serialize)]
pub struct AgentEvidenceResponse {
    pub repo_id: String,
    pub evidence: Vec<AgentEvidenceItem>,
}

#[derive(Debug, Serialize)]
pub struct AgentEvidenceItem {
    pub id: String,
    pub kind: String,
    pub summary: String,
}

pub async fn list_sessions(
    State(_state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
) -> Result<Json<AgentSessionsResponse>, ApiError> {
    require(&viewer, perms::AGENTS_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    // Phase 4: no durable agent_session store yet. Return an empty list so
    // the SPA can render the "no active agents" empty state without a 502.
    Ok(Json(AgentSessionsResponse {
        repo_id: parsed.encode(),
        sessions: Vec::new(),
    }))
}

pub async fn list_evidence(
    State(_state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
) -> Result<Json<AgentEvidenceResponse>, ApiError> {
    require(&viewer, perms::AGENTS_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    // Phase 4: placeholder.
    Ok(Json(AgentEvidenceResponse {
        repo_id: parsed.encode(),
        evidence: Vec::new(),
    }))
}
