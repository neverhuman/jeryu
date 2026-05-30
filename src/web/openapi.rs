//! OpenAPI assembly for the JeRyu Web Forge BFF.
//!
//! Defines the [`WebApiOpenApi`] struct that ties together all REST handler
//! `#[utoipa::path]` annotations into a single OpenAPI 3.x document.
//!
//! The struct lives binary-side (next to the REST handlers) because the
//! `paths(...)` list must reference handler functions, and those live in
//! `crate::web::rest::*` which is gated to the `jeryu` binary (not the
//! library). The export entry point is invoked via the
//! `jeryu web export-schemas` CLI subcommand wired in [`crate::web::command`].

use std::fs;
use std::path::Path;

use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

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
    modifiers(&SecurityAddon),
    paths(
        // bootstrap
        crate::web::rest::bootstrap::get_bootstrap,
        // repos
        crate::web::rest::repos::list_repos,
        crate::web::rest::repos::get_repo,
        crate::web::rest::repos::create_repo_preview,
        crate::web::rest::repos::create_repo,
        crate::web::rest::repos::patch_repo,
        // settings
        crate::web::rest::settings::get_settings,
        crate::web::rest::settings::preview_settings_patch,
        crate::web::rest::settings::patch_settings,
        // repo_browser
        crate::web::rest::repo_browser::list_refs,
        crate::web::rest::repo_browser::get_tree,
        crate::web::rest::repo_browser::get_blob,
        crate::web::rest::repo_browser::get_raw,
        crate::web::rest::repo_browser::get_readme,
        crate::web::rest::repo_browser::compare_refs,
        crate::web::rest::repo_browser::list_commits,
        crate::web::rest::repo_browser::get_blame,
        // markdown
        crate::web::rest::markdown::render_markdown_handler,
        // merge_requests
        crate::web::rest::merge_requests::list_merge_requests,
        crate::web::rest::merge_requests::get_merge_request,
        crate::web::rest::merge_requests::create_merge_request,
        crate::web::rest::merge_requests::patch_merge_request,
        crate::web::rest::merge_requests::approve_merge_request,
        crate::web::rest::merge_requests::request_changes,
        crate::web::rest::merge_requests::merge_merge_request,
        crate::web::rest::merge_requests::close_merge_request,
        crate::web::rest::merge_requests::reopen_merge_request,
        crate::web::rest::merge_requests::rebase_merge_request,
        crate::web::rest::merge_requests::get_diff,
        crate::web::rest::merge_requests::get_checks,
        crate::web::rest::merge_requests::get_blockers,
        // reviews
        crate::web::rest::reviews::list_threads,
        crate::web::rest::reviews::create_thread,
        crate::web::rest::reviews::patch_thread,
        crate::web::rest::reviews::create_comment,
        crate::web::rest::reviews::submit_review,
        // ci
        crate::web::rest::ci::list_pipelines,
        crate::web::rest::ci::get_pipeline,
        crate::web::rest::ci::list_pipeline_jobs,
        crate::web::rest::ci::get_job_log,
        crate::web::rest::ci::retry_job,
        crate::web::rest::ci::cancel_job,
        crate::web::rest::ci::list_checks,
        // actions
        crate::web::rest::actions::preview_action,
        crate::web::rest::actions::execute_action,
        // search
        crate::web::rest::search::search,
        // activity
        crate::web::rest::activity::list_activity,
        // issues (501 stubs)
        crate::web::rest::issues::list_issues,
        crate::web::rest::issues::create_issue,
        crate::web::rest::issues::get_issue,
        crate::web::rest::issues::patch_issue,
        // agents
        crate::web::rest::agents::list_sessions,
        crate::web::rest::agents::list_evidence,
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
        rv::ReviewEvidence,
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
        // Settings patch + preview wire shapes (consumed by /repos/{id}/settings/*)
        crate::repos::settings::SettingsPatch,
        crate::repos::settings::SettingsDiffPreview,
        // Bootstrap / viewer
        wrm::WebBootstrap,
        wrm::Viewer,
        wrm::WebFeatureFlags,
        // Issues (compat DTOs, v1.5)
        is_::IssueSummary,
        is_::IssueState,
        // WebSocket frame envelopes — surfaced here so the OpenAPI doc has
        // a single source of truth for the wire types even though they're
        // canonically described in `schemas/websocket-events.schema.json`.
        ws::ClientWsMessage,
        ws::ServerWsMessage,
        ws::SubscriptionSpec,
        ws::WebEvent,
        // ── Inline REST response/request DTOs ──
        crate::web::rest::repos::RepoDetailResponse,
        crate::web::rest::repo_browser::RefsResponse,
        crate::web::rest::repo_browser::TreeResponse,
        crate::web::rest::repo_browser::CompareResponse,
        crate::web::rest::repo_browser::CompareFile,
        crate::web::rest::repo_browser::CommitsResponse,
        crate::web::rest::repo_browser::CommitItem,
        crate::web::rest::repo_browser::BlameResponse,
        crate::web::rest::repo_browser::BlameLine,
        crate::web::rest::markdown::MarkdownRenderRequest,
        crate::web::rest::markdown::MarkdownRenderContext,
        crate::web::rest::markdown::MarkdownRenderResponse,
        crate::web::rest::merge_requests::ListMrResponse,
        crate::web::rest::merge_requests::CreateMrRequest,
        crate::web::rest::merge_requests::PatchMrRequest,
        crate::web::rest::merge_requests::ApproveMrRequest,
        crate::web::rest::merge_requests::RequestChangesRequest,
        crate::web::rest::merge_requests::MergeMrRequest,
        crate::web::rest::merge_requests::ApproveMrResponse,
        crate::web::rest::merge_requests::MergeMrResponse,
        crate::web::rest::merge_requests::EmptyOk,
        crate::web::rest::merge_requests::DiffResponse,
        crate::web::rest::merge_requests::DiffFile,
        crate::web::rest::merge_requests::ChecksResponse,
        crate::web::rest::merge_requests::PipelineSummary,
        crate::web::rest::reviews::ListThreadsResponse,
        crate::web::rest::reviews::PatchThreadRequest,
        crate::web::rest::reviews::SubmitReviewResponse,
        crate::web::rest::ci::PipelineItem,
        crate::web::rest::ci::PipelinesResponse,
        crate::web::rest::ci::JobItem,
        crate::web::rest::ci::JobsResponse,
        crate::web::rest::ci::JobLogResponse,
        crate::web::rest::ci::CheckItem,
        crate::web::rest::ci::ChecksResponse,
        crate::web::rest::ci::JobActionReceipt,
        crate::web::rest::actions::PreviewRequest,
        crate::web::rest::actions::ExecuteRequest,
        crate::web::rest::actions::PreviewResponse,
        crate::web::rest::actions::ExecuteResponse,
        crate::repos::search::SearchResults,
        crate::repos::search::RepoSearchHit,
        crate::repos::search::FileSearchHit,
        crate::repos::search::CommitSearchHit,
        crate::repos::search::MrSearchHit,
        crate::repos::search::IssueSearchHit,
        crate::repos::search::UserSearchHit,
        crate::web::rest::activity::ActivityResponse,
        crate::web::rest::agents::AgentSessionsResponse,
        crate::web::rest::agents::AgentEvidenceResponse,
        crate::web::rest::agents::AgentEvidenceItem,
    )),
    tags(
        (name = "bootstrap", description = "Web Forge bootstrap snapshot."),
        (name = "repos", description = "Repository list / detail / create / patch."),
        (name = "settings", description = "Repository settings read / preview / patch."),
        (name = "repo_browser", description = "Refs, tree, blob, raw, readme, compare, commits, blame."),
        (name = "markdown", description = "Standalone markdown render."),
        (name = "merge_requests", description = "Merge requests list / detail / actions / merge passport."),
        (name = "reviews", description = "Review threads / comments / verdict."),
        (name = "ci", description = "Pipelines / jobs / checks."),
        (name = "actions", description = "Generic action preview / execute."),
        (name = "search", description = "Global search."),
        (name = "activity", description = "Activity feed."),
        (name = "issues", description = "Issues (v1.5)."),
        (name = "agents", description = "Agent sessions / evidence (read-only)."),
    ),
)]
pub struct WebApiOpenApi;

/// Adds the `session` (cookie) and `csrf` (header) security schemes that
/// the `#[utoipa::path(security(...))]` blocks reference.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("jeryu_session"))),
        );
        components.add_security_scheme(
            "csrf",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-CSRF-Token"))),
        );
    }
}

/// Build the OpenAPI document as a pretty-printed JSON string.
pub fn build_openapi_json() -> Result<String, serde_json::Error> {
    let value = serde_json::to_value(WebApiOpenApi::openapi())?;
    serde_json::to_string_pretty(&add_generated_banner(
        value,
        "Generated by: jeryu_export_schemas",
        "Source: src/web/openapi.rs",
        "Regenerate: cargo run --bin jeryu_export_schemas --features web",
        "DO NOT EDIT BY HAND",
    ))
}

/// Helper struct that schemars walks to emit a single schema covering both
/// client and server WS message envelopes plus the `WebEvent` payload.
#[derive(schemars::JsonSchema)]
#[allow(dead_code)]
struct WsRoot {
    client: ws::ClientWsMessage,
    server: ws::ServerWsMessage,
    event: ws::WebEvent,
}

/// Write the OpenAPI + WebSocket schemas to `<out_dir>/web-api.openapi.json`
/// and `<out_dir>/websocket-events.schema.json` respectively. Used by
/// `jeryu web export-schemas` (`src/web/command.rs`).
pub fn export_schemas(out_dir: &Path) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    fs::create_dir_all(out_dir)?;

    let openapi = build_openapi_json()?;
    let openapi_path = out_dir.join("web-api.openapi.json");
    fs::write(&openapi_path, &openapi)?;

    let ws_schema = schemars::schema_for!(WsRoot);
    let ws_json = serde_json::to_string_pretty(&add_generated_banner(
        serde_json::to_value(&ws_schema)?,
        "Generated by: jeryu_export_schemas",
        "Source: src/web/openapi.rs",
        "Regenerate: cargo run --bin jeryu_export_schemas --features web",
        "DO NOT EDIT BY HAND",
    ))?;
    let ws_path = out_dir.join("websocket-events.schema.json");
    fs::write(&ws_path, &ws_json)?;

    Ok((openapi.len(), ws_json.len()))
}

fn add_generated_banner(
    value: serde_json::Value,
    generated_by: &str,
    source: &str,
    regenerate: &str,
    do_not_edit: &str,
) -> serde_json::Value {
    use serde_json::{Map, Value};

    let mut out = Map::new();
    out.insert(
        "!generated_banner".to_string(),
        Value::Array(vec![
            Value::String(generated_by.to_string()),
            Value::String(source.to_string()),
            Value::String(regenerate.to_string()),
            Value::String(do_not_edit.to_string()),
        ]),
    );
    if let Value::Object(map) = value {
        for (k, v) in map {
            out.insert(k, v);
        }
    }
    Value::Object(out)
}
