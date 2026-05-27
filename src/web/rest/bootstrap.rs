//! `GET /api/v1/bootstrap` — W-B-05.
//!
//! Returns the bundled `WebBootstrap` snapshot the SPA needs on first
//! paint: viewer identity, TUI read-model snapshot, recent repos, the
//! WebSocket URL, and Phase-1 feature flags. The TUI snapshot is a
//! default placeholder until W-B-* hands real read-model assembly.

use axum::{Extension, Json, extract::State};
use chrono::Utc;

use jeryu::api::read_model::TuiReadModel;
use jeryu::api::web_read_model::{Viewer as DtoViewer, WebBootstrap, WebFeatureFlags};

use crate::web::auth::Viewer as AuthViewer;
use crate::web::error::ApiError;
use crate::web::state::WebState;

/// Phase-1 schema version for the bootstrap envelope. Matches the wire
/// version on the FE (`apps/web/src/data/bootstrap.ts`); bump when the
/// shape changes in a non-backward-compatible way.
const SCHEMA_VERSION: &str = "0.1.0-alpha";

pub async fn get_bootstrap(
    State(state): State<WebState>,
    Extension(viewer): Extension<AuthViewer>,
) -> Result<Json<WebBootstrap>, ApiError> {
    let ff = &state.feature_flags;
    let mut perms_vec: Vec<String> = viewer.perms.iter().cloned().collect();
    perms_vec.sort(); // stable wire order for tests / diffs.

    Ok(Json(WebBootstrap {
        generated_at: Utc::now(),
        schema_version: SCHEMA_VERSION.into(),
        viewer: DtoViewer {
            id: viewer.id.clone(),
            login: viewer.login.clone(),
            display_name: viewer.display_name.clone(),
            avatar_url: None,
            global_permissions: perms_vec,
        },
        tui: TuiReadModel::default(),
        recent_repositories: Vec::new(),
        websocket_url: "/api/v1/ws".into(),
        feature_flags: WebFeatureFlags {
            repo_create: ff.repo_create,
            settings_write: ff.settings_write,
            merge_write: ff.merge_write,
            markdown_html: ff.markdown_html,
            agents: ff.agents,
            mcp: ff.mcp,
        },
    }))
}
