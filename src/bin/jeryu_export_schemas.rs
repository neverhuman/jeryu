//! `jeryu_export_schemas` — regenerate OpenAPI + JSON-Schema artifacts.
//!
//! Outputs:
//! - `schemas/web-api.openapi.json` — OpenAPI 3.x spec for `/api/v1/*` HTTP
//!   routes, generated from utoipa's `#[derive(ToSchema)]` on the BFF DTOs.
//! - `schemas/websocket-events.schema.json` — JSON Schema for the
//!   WebSocket frame envelopes (`ClientWsMessage`, `ServerWsMessage`,
//!   `WebEvent`), generated from schemars.
//!
//! Registered as a generated zone in `agent/generated-zones.toml`. The
//! bin is `--features web`-only because the `utoipa::ToSchema` and
//! `schemars::JsonSchema` derives on the DTOs are gated on `feature = "web"`.

#![cfg(feature = "web")]

use std::fs;
use std::path::Path;

use utoipa::OpenApi;

use jeryu::api::{
    issues as is_, merge_request as mr, repo_browser as rb, repository as r, review as rv,
    settings as s, web_read_model as wrm, websocket as ws,
};

#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "JeRyu Web Forge API",
        version = "0.1.0-alpha",
        description = "BFF for the JeRyu Web Forge. Versioned under /api/v1/. \
                       See WEB_WORK_CLAUDE.md §35.7 for the canonical route map.",
    ),
    components(schemas(
        // Repository surface (W-F-03)
        r::RepositoryId,
        r::RepositoryVisibility,
        r::RepositoryHostKind,
        r::RepositorySummary,
        r::RepositoryListResponse,
        r::RepositoryFacets,
        r::CreateRepositoryRequest,
        r::CreateRepositoryPreview,
        // Repo browser surface
        rb::RefSelectorItem,
        rb::RefKind,
        rb::TreeEntry,
        rb::TreeEntryKind,
        rb::BlobResponse,
        rb::BlobEncoding,
        rb::RenderedMarkdown,
        rb::MarkdownHeading,
        rb::MarkdownLink,
        // Merge request surface
        mr::MergeRequestSummary,
        mr::MergeRequestDetail,
        mr::MergeRequestState,
        mr::Mergeability,
        mr::ReviewPosture,
        mr::CheckPosture,
        mr::AgentPosture,
        mr::MergePassport,
        mr::MergePassportStatus,
        mr::MergePassportBlocker,
        // Review surface
        rv::ReviewThread,
        rv::ReviewComment,
        rv::ReviewSuggestion,
        rv::CreateReviewCommentRequest,
        rv::SubmitReviewRequest,
        rv::ReviewVerdict,
        // Settings surface
        s::RepositorySettings,
        s::GeneralSettings,
        s::FeatureSettings,
        s::MergeSettings,
        s::BranchProtectionRule,
        s::CiSettings,
        s::AgentSettings,
        s::AccessSettings,
        s::SecuritySettings,
        s::NotificationSettings,
        s::RetentionSettings,
        // Bootstrap / viewer
        wrm::WebBootstrap,
        wrm::Viewer,
        wrm::WebFeatureFlags,
        // Issues (placeholder DTOs, v1.5)
        is_::IssueSummary,
        is_::IssueState,
        // WebSocket frame envelopes — surfaced here so the OpenAPI doc has
        // a single source of truth for the wire types even though they're
        // canonically described in `schemas/websocket-events.schema.json`.
        ws::ClientWsMessage,
        ws::ServerWsMessage,
        ws::SubscriptionSpec,
        ws::WebEvent,
    )),
    // Paths get filled in by W-B-* via `#[utoipa::path]` on handlers.
    // For now this is a schema-only OpenAPI doc; the SPA's contract is the
    // `components.schemas` block.
)]
struct WebApiOpenApi;

/// Helper struct that schemars walks to emit a single schema covering both
/// client and server WS message envelopes plus the `WebEvent` payload.
#[derive(schemars::JsonSchema)]
#[allow(dead_code)]
struct WsRoot {
    client: ws::ClientWsMessage,
    server: ws::ServerWsMessage,
    event: ws::WebEvent,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all("schemas")?;

    // OpenAPI (schemas-only at this stage; W-B-* adds paths later).
    let openapi = WebApiOpenApi::openapi().to_pretty_json()?;
    fs::write(Path::new("schemas/web-api.openapi.json"), &openapi)?;
    eprintln!(
        "wrote schemas/web-api.openapi.json ({} bytes)",
        openapi.len()
    );

    // JSON Schema for WS frames.
    let ws_schema = schemars::schema_for!(WsRoot);
    let ws_json = serde_json::to_string_pretty(&ws_schema)?;
    fs::write(Path::new("schemas/websocket-events.schema.json"), &ws_json)?;
    eprintln!(
        "wrote schemas/websocket-events.schema.json ({} bytes)",
        ws_json.len()
    );

    Ok(())
}
