//! REST handler for the activity feed (W-B-17).
//!
//! `GET /api/v1/activity?since=&limit=&scope=`
//!
//! Returns the most recent `WebEvent`s from the rolling buffer maintained
//! by [`crate::web::state::ActivityBuffer`]. The buffer is populated by a
//! tap that mirrors every event published on the `WebEventBus` (high +
//! low priority channels), capped at 500 entries.
//!
//! Filters:
//!   - `since=<u64>` returns events with `seq > since`.
//!   - `scope=<str>` returns events whose `scope` is exactly equal or starts
//!     with the supplied prefix (so `scope=repo.` matches every per-repo
//!     event). Empty / missing scope means "all".
//!   - `limit=<u32>` clamps to 500 (the buffer capacity).
//!
//! Authorization: the canonical `global.activity` scope is public-by-default
//! per `crate::web::permissions::perm_for_scope`. Authenticated viewers can
//! always read their own feed; per-scope filtering happens server-side here.

use axum::{
    Extension, Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};

use jeryu::api::websocket::WebEvent;

use crate::web::auth::Viewer;
use crate::web::error::ApiError;
use crate::web::permissions;
use crate::web::state::WebState;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ActivityQuery {
    /// Only events with `seq > since` are returned.
    pub since: Option<u64>,
    /// Exact scope or prefix to filter by (e.g. `repo.gitlab/x/y`).
    pub scope: Option<String>,
    /// Max events to return; clamped to the buffer capacity.
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ActivityResponse {
    pub events: Vec<WebEvent>,
    /// Buffer capacity at fetch time. Lets the FE detect that the rolling
    /// window has wrapped (a sudden drop in `total` vs. recent reads).
    pub buffer_capacity: usize,
    /// Number of events currently held in the buffer.
    pub buffer_len: usize,
}

#[utoipa::path(
    get,
    path = "/api/v1/activity",
    params(ActivityQuery),
    responses(
        (status = 200, description = "Recent activity events", body = ActivityResponse),
        (status = 403, description = "Forbidden scope"),
        (status = 401, description = "Unauthenticated"),
    ),
    tag = "activity",
    security(("session" = [])),
)]
pub async fn list_activity(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Query(q): Query<ActivityQuery>,
) -> Result<Json<ActivityResponse>, ApiError> {
    // §35.1.6: scope-driven authorization. The activity feed is gated on
    // the implied scope of the filter; absent scope defaults to the public
    // `global.activity` channel.
    let scope_for_perm = q
        .scope
        .clone()
        .unwrap_or_else(|| "global.activity".to_string());
    if let Some(perm) = permissions::perm_for_scope(&scope_for_perm) {
        if !viewer.perms.contains(perm) {
            return Err(ApiError::Forbidden(perm.into()));
        }
    }
    let buffer = state.activity_buffer.clone();
    let buffer_capacity = crate::web::state::ACTIVITY_BUFFER_CAPACITY;
    let mut events = buffer.recent();
    if let Some(min_seq) = q.since {
        events.retain(|e| e.seq > min_seq);
    }
    if let Some(scope_filter) = q.scope.as_deref() {
        if !scope_filter.is_empty() {
            events.retain(|e| {
                e.scope == scope_filter
                    || (scope_filter.ends_with('.') && e.scope.starts_with(scope_filter))
                    || e.scope
                        .starts_with(&format!("{scope_filter}."))
            });
        }
    }
    let limit = q
        .limit
        .map(|n| n as usize)
        .unwrap_or(50)
        .min(buffer_capacity);
    let buffer_len = buffer.len();
    events.truncate(limit);
    Ok(Json(ActivityResponse {
        events,
        buffer_capacity,
        buffer_len,
    }))
}
